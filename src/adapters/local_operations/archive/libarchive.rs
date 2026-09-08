// SPDX-License-Identifier: GPL-3.0-or-later

//! libarchive codecs via the system `libarchive.so.13`.
//!
//! Isolates format encoding and decoding. Filesystem policy — destination
//! pinning, path validation, zip-bomb limits — stays in the parent module.
//! Each archive object is created, used, and dropped on one worker thread.

#![expect(
    unsafe_code,
    reason = "libarchive exposes archive read and write only through its C API"
)]

mod ffi;

use std::{
    ffi::{CString, OsStr, OsString, c_int, c_uint},
    fs::File,
    io::{self, Read, Write},
    marker::PhantomData,
    os::{fd::AsRawFd as _, unix::ffi::OsStrExt as _},
    path::Path,
};

use libc::mode_t;

use crate::services::ArchiveFormat;

/// Tar filters that tests can encode but the compress dialog cannot create.
#[cfg(test)]
#[derive(Clone, Copy, Debug)]
pub(super) enum TarFilter {
    Xz,
    Zstd,
    Bzip2,
}

#[cfg(test)]
impl TarFilter {
    pub(super) fn extension(self) -> &'static str {
        match self {
            Self::Xz => "tar.xz",
            Self::Zstd => "tar.zst",
            Self::Bzip2 => "tar.bz2",
        }
    }
}

/// Kind of archive member reported by libarchive.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum MemberKind {
    File,
    Directory,
    Symlink,
    Special,
}

/// Metadata for one archive member. Entry data is read separately.
#[derive(Debug)]
pub(super) struct ArchiveMember {
    pub pathname: OsString,
    pub kind: MemberKind,
    pub size: Option<u64>,
    pub symlink_target: Option<OsString>,
    pub hardlink_target: Option<OsString>,
    pub mtime: Option<i64>,
    /// libarchive marked this member's data or metadata as encrypted.
    pub encrypted: bool,
}

/// libarchive decoder failure, with whether the archive reported encryption.
#[derive(Clone, Debug)]
pub(super) struct DecoderError {
    pub code: c_int,
    pub message: String,
    pub encrypted: bool,
}

impl std::fmt::Display for DecoderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.message.is_empty() {
            write!(f, "libarchive error {}", self.code)
        } else {
            f.write_str(&self.message)
        }
    }
}

impl std::error::Error for DecoderError {}

/// libarchive status codes from `archive.h`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Status {
    Ok,
    Eof,
    Warn,
    Retry,
    Failed,
    Fatal,
}

const fn status(code: c_int) -> Status {
    match code {
        ffi::ARCHIVE_OK => Status::Ok,
        ffi::ARCHIVE_EOF => Status::Eof,
        ffi::ARCHIVE_WARN => Status::Warn,
        ffi::ARCHIVE_RETRY => Status::Retry,
        ffi::ARCHIVE_FAILED => Status::Failed,
        _ if code <= ffi::ARCHIVE_FATAL => Status::Fatal,
        _ if code < 0 => Status::Failed,
        _ => Status::Ok,
    }
}

const fn saw_encrypted_entries(code: c_int) -> bool {
    match code {
        ffi::ARCHIVE_READ_FORMAT_ENCRYPTION_UNSUPPORTED
        | ffi::ARCHIVE_READ_FORMAT_ENCRYPTION_DONT_KNOW
        | 0 => false,
        n => n > 0,
    }
}

const fn member_kind(filetype: mode_t) -> MemberKind {
    match filetype & ffi::AE_IFMT {
        ffi::AE_IFREG => MemberKind::File,
        ffi::AE_IFDIR => MemberKind::Directory,
        ffi::AE_IFLNK => MemberKind::Symlink,
        ffi::AE_IFBLK | ffi::AE_IFCHR | ffi::AE_IFIFO | ffi::AE_IFSOCK => MemberKind::Special,
        _ => MemberKind::Special,
    }
}

/// Reader over a local archive file.
pub(super) struct ReadArchive {
    archive: Option<*mut ffi::archive>,
    path: std::path::PathBuf,
    _file: File,
    _not_send_sync: PhantomData<*mut ()>,
}

impl ReadArchive {
    /// Opens `path` with the format and filter allowlist.
    ///
    /// Unencrypted and password-protected archives are both opened from a file
    /// descriptor, so the path need not be UTF-8. The passphrase is registered
    /// before the descriptor is opened.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be opened, the reader cannot be
    /// allocated, the passphrase is rejected, or the archive cannot be opened.
    pub(super) fn open(path: &Path, password: Option<&str>) -> Result<Self, String> {
        let file = File::open(path).map_err(|error| error.to_string())?;
        // SAFETY: `archive_read_new` allocates a reader or returns null.
        let archive = unsafe { ffi::archive_read_new() };
        if archive.is_null() {
            return Err("Failed to allocate archive reader".to_owned());
        }
        let reader = Self {
            archive: Some(archive),
            path: path.to_path_buf(),
            _file: file,
            _not_send_sync: PhantomData,
        };
        reader.enable_read_formats()?;
        if let Some(password) = password {
            let c_password = CString::new(password)
                .map_err(|_| "Password contains an interior NUL".to_owned())?;
            // SAFETY: `archive` is the live handle from `archive_read_new`.
            // `c_password` is NUL-terminated and outlives this call; libarchive
            // copies the passphrase.
            let code = unsafe { ffi::archive_read_add_passphrase(archive, c_password.as_ptr()) };
            check_setup(archive, code, "passphrase")?;
        }
        let fd = reader._file.as_raw_fd();
        // SAFETY: `archive` is live and `fd` belongs to `reader._file`, which
        // outlives the archive handle.
        let code = unsafe { ffi::archive_read_open_fd(archive, fd, ffi::READ_BLOCK_SIZE) };
        check_setup(archive, code, "open")?;
        Ok(reader)
    }

    fn enable_read_formats(&self) -> Result<(), String> {
        let archive = self.handle()?;
        for (what, support) in [
            ("zip", ffi::archive_read_support_format_zip as FormatFn),
            ("7z", ffi::archive_read_support_format_7zip as FormatFn),
            ("tar", ffi::archive_read_support_format_tar as FormatFn),
            ("rar", ffi::archive_read_support_format_rar as FormatFn),
            ("rar5", ffi::archive_read_support_format_rar5 as FormatFn),
            ("gzip", ffi::archive_read_support_filter_gzip as FormatFn),
            ("bzip2", ffi::archive_read_support_filter_bzip2 as FormatFn),
            ("xz", ffi::archive_read_support_filter_xz as FormatFn),
            ("zstd", ffi::archive_read_support_filter_zstd as FormatFn),
        ] {
            // SAFETY: `archive` is the live handle from `archive_read_new` on
            // this thread. Each support function only registers a format or
            // filter on that handle.
            let code = unsafe { support(archive) };
            check_setup(archive, code, what)?;
        }
        Ok(())
    }

    fn handle(&self) -> Result<*mut ffi::archive, String> {
        self.archive
            .ok_or_else(|| "Archive reader is closed".to_owned())
    }

    /// Reads the next member header. `None` means the archive is exhausted.
    ///
    /// # Errors
    ///
    /// Returns a [`DecoderError`] if libarchive cannot parse the next header.
    /// `encrypted` is taken from the archive when the header itself fails
    /// (RAR5 encrypted headers never yield an entry).
    pub(super) fn next_member(&mut self) -> Result<Option<ArchiveMember>, DecoderError> {
        let archive = self.handle().map_err(|message| DecoderError {
            code: ffi::ARCHIVE_FATAL,
            message,
            encrypted: false,
        })?;
        loop {
            let mut entry: *mut ffi::archive_entry = std::ptr::null_mut();
            let _locale = ffi::Utf8LocaleGuard::enter();
            // SAFETY: `archive` is live on this thread. `entry` points at a
            // local that libarchive writes; the returned entry is owned by
            // the archive and is only borrowed until the next header call.
            let code = unsafe { ffi::archive_read_next_header(archive, &mut entry) };
            match status(code) {
                Status::Eof => return Ok(None),
                Status::Ok => return self.member_from_entry(entry),
                Status::Warn => {
                    tracing::warn!(
                        path = %self.path.display(),
                        message = archive_error_message(archive).as_str(),
                        "libarchive reports a warning while reading a member header"
                    );
                    return self.member_from_entry(entry);
                }
                Status::Retry => continue,
                Status::Failed | Status::Fatal => {
                    return Err(self.decoder_error(code));
                }
            }
        }
    }

    fn member_from_entry(
        &self,
        entry: *mut ffi::archive_entry,
    ) -> Result<Option<ArchiveMember>, DecoderError> {
        if entry.is_null() {
            return Err(self.decoder_error(ffi::ARCHIVE_FAILED));
        }
        let hardlink_target = optional_os_string(
            // SAFETY: `entry` is the current header from `archive_read_next_header`
            // and is valid until the next header call; `cstr_bytes` copies.
            unsafe { ffi::archive_entry_hardlink_utf8(entry) },
            // SAFETY: same lifetime as the UTF-8 accessor above.
            unsafe { ffi::archive_entry_hardlink(entry) },
        );
        // libarchive reports hard-link members as Unknown, not RegularFile.
        let kind = if hardlink_target.is_some() {
            MemberKind::File
        } else {
            // SAFETY: `entry` is the current live header.
            member_kind(unsafe { ffi::archive_entry_filetype(entry) })
        };
        Ok(Some(ArchiveMember {
            pathname: entry_pathname(entry),
            kind,
            size: entry_size(entry),
            symlink_target: optional_os_string(
                // SAFETY: `entry` is the current live header.
                unsafe { ffi::archive_entry_symlink_utf8(entry) },
                // SAFETY: same lifetime as the UTF-8 accessor above.
                unsafe { ffi::archive_entry_symlink(entry) },
            ),
            hardlink_target,
            mtime: entry_mtime(entry),
            encrypted: entry_encrypted(entry),
        }))
    }

    /// Discards the current member body without writing it.
    ///
    /// # Errors
    ///
    /// Returns an error if libarchive cannot skip the data.
    pub(super) fn skip_data(&mut self) -> Result<(), DecoderError> {
        let archive = self.handle().map_err(|message| DecoderError {
            code: ffi::ARCHIVE_FATAL,
            message,
            encrypted: self.saw_encrypted_entries(),
        })?;
        // SAFETY: `archive` is live on this thread and positioned at a member.
        let code = unsafe { ffi::archive_read_data_skip(archive) };
        accept_warn(archive, code).map_err(|message| DecoderError {
            code,
            message,
            encrypted: self.saw_encrypted_entries(),
        })
    }

    /// Whether libarchive has observed an encrypted member on this archive.
    fn saw_encrypted_entries(&self) -> bool {
        let Some(archive) = self.archive else {
            return false;
        };
        // SAFETY: `archive` is the live handle from `archive_read_new`.
        saw_encrypted_entries(unsafe { ffi::archive_read_has_encrypted_entries(archive) })
    }

    fn decoder_error(&self, code: c_int) -> DecoderError {
        let message = self
            .archive
            .map(archive_error_message)
            .filter(|message| !message.is_empty())
            .unwrap_or_else(|| format!("libarchive error {code}"));
        DecoderError {
            code,
            message,
            encrypted: self.saw_encrypted_entries(),
        }
    }
}

impl Drop for ReadArchive {
    fn drop(&mut self) {
        let Some(archive) = self.archive.take() else {
            return;
        };
        // SAFETY: `archive` came from `archive_read_new` and has not been
        // freed; this is the unique free.
        unsafe {
            ffi::archive_read_free(archive);
        }
    }
}

impl Read for ReadArchive {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let archive = self.handle().map_err(io::Error::other)?;
        // SAFETY: `archive` is live on this thread. `buf` is a valid mutable
        // slice that outlives the call.
        let n = unsafe { ffi::archive_read_data(archive, buf.as_mut_ptr().cast(), buf.len()) };
        if n < 0 {
            let code = c_int::try_from(n).unwrap_or(ffi::ARCHIVE_FATAL);
            return Err(io::Error::other(self.decoder_error(code)));
        }
        usize::try_from(n).map_err(|_| {
            io::Error::other("libarchive returned a byte count that does not fit in usize")
        })
    }
}

/// Writer that encodes members into an already-open staging file.
pub(super) struct WriteArchive {
    archive: Option<*mut ffi::archive>,
    _file: File,
    _not_send_sync: PhantomData<*mut ()>,
}

impl WriteArchive {
    /// Prepares a writer for `format` on `file`.
    ///
    /// Passwords are rejected: libarchive cannot write encrypted archives.
    /// ZIP always uses deflate; libarchive cannot change ZIP compression after
    /// the first header.
    ///
    /// # Errors
    ///
    /// Returns an error if the writer cannot be allocated, the format is not
    /// creatable, an option is rejected, a password is supplied, or the file
    /// descriptor cannot be used.
    pub(super) fn create(
        file: File,
        format: ArchiveFormat,
        password: Option<&str>,
    ) -> Result<Self, String> {
        if password.is_some() {
            return Err("This format does not support passwords".to_owned());
        }
        let writer = Self::new_handle(file)?;
        match format {
            ArchiveFormat::Zip => {
                writer.setup(ffi::archive_write_set_format_zip, "zip")?;
                writer.set_zip_deflate()?;
            }
            ArchiveFormat::SevenZ => {
                writer.setup(ffi::archive_write_set_format_7zip, "7z")?;
            }
            ArchiveFormat::TarGz => {
                writer.setup(ffi::archive_write_set_format_pax_restricted, "tar")?;
                writer.setup(ffi::archive_write_add_filter_gzip, "gzip")?;
            }
            ArchiveFormat::Tar => {
                writer.setup(ffi::archive_write_set_format_pax_restricted, "tar")?;
                writer.setup(ffi::archive_write_add_filter_none, "none")?;
            }
        }
        writer.open_fd()?;
        Ok(writer)
    }

    /// Prepares a writer for a tar filter that tests can extract but the
    /// compress dialog cannot create.
    ///
    /// # Errors
    ///
    /// Returns an error if the writer cannot be allocated, an option is
    /// rejected, or the file descriptor cannot be used.
    #[cfg(test)]
    pub(super) fn create_tar_filter(file: File, filter: TarFilter) -> Result<Self, String> {
        let writer = Self::new_handle(file)?;
        writer.setup(ffi::archive_write_set_format_pax_restricted, "tar")?;
        match filter {
            TarFilter::Xz => writer.setup(ffi::archive_write_add_filter_xz, "xz")?,
            TarFilter::Zstd => writer.setup(ffi::archive_write_add_filter_zstd, "zstd")?,
            TarFilter::Bzip2 => writer.setup(ffi::archive_write_add_filter_bzip2, "bzip2")?,
        }
        writer.open_fd()?;
        Ok(writer)
    }

    fn new_handle(file: File) -> Result<Self, String> {
        // SAFETY: `archive_write_new` allocates a writer or returns null.
        let archive = unsafe { ffi::archive_write_new() };
        if archive.is_null() {
            return Err("Failed to allocate archive writer".to_owned());
        }
        Ok(Self {
            archive: Some(archive),
            _file: file,
            _not_send_sync: PhantomData,
        })
    }

    fn setup(
        &self,
        operation: unsafe extern "C" fn(*mut ffi::archive) -> c_int,
        what: &str,
    ) -> Result<(), String> {
        let archive = self.inner_ptr()?;
        // SAFETY: `archive` is the live handle from `archive_write_new` on
        // this thread. `operation` is a libarchive setter for that handle.
        let code = unsafe { operation(archive) };
        check_setup(archive, code, what)
    }

    fn set_zip_deflate(&self) -> Result<(), String> {
        let archive = self.inner_ptr()?;
        let module = c"zip";
        let option = c"compression";
        let value = c"deflate";
        // SAFETY: `archive` is live. The three C strings are static and
        // outlive the call.
        let code = unsafe {
            ffi::archive_write_set_format_option(
                archive,
                module.as_ptr(),
                option.as_ptr(),
                value.as_ptr(),
            )
        };
        check_setup(archive, code, "zip deflate")
    }

    fn open_fd(&self) -> Result<(), String> {
        let archive = self.inner_ptr()?;
        let fd = self._file.as_raw_fd();
        // SAFETY: `archive` is live and `fd` belongs to `_file`, which
        // outlives the archive handle.
        let code = unsafe { ffi::archive_write_open_fd(archive, fd) };
        check_setup(archive, code, "open")
    }

    fn inner_ptr(&self) -> Result<*mut ffi::archive, String> {
        self.archive
            .ok_or_else(|| "Archive writer is closed".to_owned())
    }

    /// Writes a directory header for `path`.
    ///
    /// # Errors
    ///
    /// Returns an error if the pathname is not UTF-8 or libarchive rejects the header.
    pub(super) fn write_directory(&mut self, path: &Path, perm: u32) -> Result<(), String> {
        self.write_configured_entry(|entry| {
            let _pathname = set_pathname(entry, path)?;
            set_filetype(entry, ffi::AE_IFDIR);
            set_perm(entry, perm);
            set_size(entry, 0);
            Ok(())
        })
    }

    /// Writes a symlink header for `path` pointing at `target`.
    ///
    /// # Errors
    ///
    /// Returns an error if a name is not UTF-8 or libarchive rejects the header.
    pub(super) fn write_symlink(&mut self, path: &Path, target: &OsStr) -> Result<(), String> {
        let target_text = target.to_str().ok_or_else(|| {
            format!(
                "Cannot preserve the non-UTF-8 link target of {}.",
                path.display()
            )
        })?;
        let c_target = CString::new(target_text).map_err(|_| {
            format!(
                "Cannot preserve the link target of {} because it contains an interior NUL.",
                path.display()
            )
        })?;
        self.write_configured_entry(|entry| {
            let _pathname = set_pathname(entry, path)?;
            // SAFETY: `entry` is a live `archive_entry` from `archive_entry_new`.
            // `c_target` is NUL-terminated and outlives this call and the
            // subsequent `archive_write_header`.
            unsafe { ffi::archive_entry_set_symlink_utf8(entry, c_target.as_ptr()) };
            set_filetype(entry, ffi::AE_IFLNK);
            set_perm(entry, 0o777);
            set_size(entry, 0);
            Ok(())
        })
    }

    /// Writes a hard-link header for `path` pointing at an earlier member `target`.
    ///
    /// # Errors
    ///
    /// Returns an error if a name is not UTF-8 or libarchive rejects the header.
    #[cfg(test)]
    pub(super) fn write_hardlink(&mut self, path: &Path, target: &OsStr) -> Result<(), String> {
        let target_text = target.to_str().ok_or_else(|| {
            format!(
                "Cannot preserve the non-UTF-8 hard link target of {}.",
                path.display()
            )
        })?;
        let c_target = CString::new(target_text).map_err(|_| {
            format!(
                "Cannot preserve the hard link target of {} because it contains an interior NUL.",
                path.display()
            )
        })?;
        self.write_configured_entry(|entry| {
            let _pathname = set_pathname(entry, path)?;
            // SAFETY: `entry` is a live `archive_entry` from `archive_entry_new`.
            // `c_target` is NUL-terminated and outlives this call and the
            // subsequent `archive_write_header`.
            unsafe { ffi::archive_entry_set_hardlink_utf8(entry, c_target.as_ptr()) };
            set_filetype(entry, ffi::AE_IFREG);
            set_perm(entry, 0o644);
            set_size(entry, 0);
            Ok(())
        })
    }

    /// Writes a regular-file header.
    ///
    /// # Errors
    ///
    /// Returns an error if the pathname is not UTF-8 or libarchive rejects the header.
    pub(super) fn write_file_header(
        &mut self,
        path: &Path,
        size: u64,
        perm: u32,
        mtime: Option<i64>,
    ) -> Result<(), String> {
        self.write_configured_entry(|entry| {
            let _pathname = set_pathname(entry, path)?;
            set_filetype(entry, ffi::AE_IFREG);
            set_perm(entry, perm);
            set_size(entry, i64::try_from(size).unwrap_or(i64::MAX));
            if let Some(mtime) = mtime.filter(|mtime| *mtime >= 0) {
                // SAFETY: `entry` is a live `archive_entry` from `archive_entry_new`.
                unsafe { ffi::archive_entry_set_mtime(entry, mtime, 0) };
            }
            Ok(())
        })
    }

    fn write_configured_entry(
        &mut self,
        configure: impl FnOnce(*mut ffi::archive_entry) -> Result<(), String>,
    ) -> Result<(), String> {
        let entry = Entry::new()?;
        let archive = self.inner_ptr()?;
        let _locale = ffi::Utf8LocaleGuard::enter();
        configure(entry.raw)?;
        // SAFETY: `archive` is live on this thread. `entry.raw` is a live
        // `archive_entry` from `archive_entry_new` that we free on drop after
        // this call. libarchive copies the header.
        let code = unsafe { ffi::archive_write_header(archive, entry.raw) };
        accept_warn(archive, code)
    }

    /// Finishes the current regular-file member.
    ///
    /// libarchive closes the current entry on the next header or when the
    /// archive is finished, so this is a no-op.
    pub(super) fn finish_entry(&mut self) -> Result<(), String> {
        let _ = self.inner_ptr()?;
        Ok(())
    }

    /// Closes the archive and writes trailers. Consumes the writer.
    ///
    /// # Errors
    ///
    /// Returns an error if libarchive cannot close the writer.
    pub(super) fn finish(mut self) -> Result<(), String> {
        let Some(archive) = self.archive.take() else {
            return Ok(());
        };
        // SAFETY: `archive` came from `archive_write_new` and has been taken
        // out of `self` so `Drop` will not free it.
        let close = unsafe { ffi::archive_write_close(archive) };
        let result = accept_warn(archive, close);
        // SAFETY: unique free of the handle taken above.
        unsafe {
            ffi::archive_write_free(archive);
        }
        result
    }
}

impl Drop for WriteArchive {
    fn drop(&mut self) {
        let Some(archive) = self.archive.take() else {
            return;
        };
        // SAFETY: `archive` came from `archive_write_new` and has not been
        // freed; this is the unique free. `archive_write_free` closes first
        // if the writer is still open.
        unsafe {
            ffi::archive_write_free(archive);
        }
    }
}

impl Write for WriteArchive {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let archive = self.inner_ptr().map_err(io::Error::other)?;
        // SAFETY: `archive` is live on this thread. `buf` is a valid slice
        // that outlives the call.
        let n = unsafe { ffi::archive_write_data(archive, buf.as_ptr().cast(), buf.len()) };
        if n < 0 {
            return Err(io::Error::other(archive_error_message(archive)));
        }
        if n == 0 && !buf.is_empty() {
            return Err(io::Error::other(
                "Archive member exceeded the size recorded in its header",
            ));
        }
        usize::try_from(n).map_err(|_| {
            io::Error::other("libarchive accepted a byte count that does not fit in usize")
        })
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

type FormatFn = unsafe extern "C" fn(*mut ffi::archive) -> c_int;

struct Entry {
    raw: *mut ffi::archive_entry,
}

impl Entry {
    fn new() -> Result<Self, String> {
        // SAFETY: `archive_entry_new` allocates an entry or returns null.
        let raw = unsafe { ffi::archive_entry_new() };
        if raw.is_null() {
            return Err("Failed to allocate an archive entry".to_owned());
        }
        Ok(Self { raw })
    }
}

impl Drop for Entry {
    fn drop(&mut self) {
        // SAFETY: `raw` came from `archive_entry_new` and is freed exactly once.
        unsafe {
            ffi::archive_entry_free(self.raw);
        }
    }
}

fn set_pathname(entry: *mut ffi::archive_entry, path: &Path) -> Result<CString, String> {
    let text = path
        .to_str()
        .ok_or_else(|| format!("Archive path is not valid UTF-8: {}", path.display()))?;
    let c_path = CString::new(text)
        .map_err(|_| format!("Archive path contains an interior NUL: {}", path.display()))?;
    // SAFETY: `entry` is a live `archive_entry` from `archive_entry_new`.
    // `c_path` is NUL-terminated and is returned so it outlives
    // `archive_write_header`.
    unsafe { ffi::archive_entry_set_pathname_utf8(entry, c_path.as_ptr()) };
    Ok(c_path)
}

fn set_filetype(entry: *mut ffi::archive_entry, filetype: mode_t) {
    // SAFETY: `entry` is a live `archive_entry` from `archive_entry_new`.
    unsafe { ffi::archive_entry_set_filetype(entry, c_uint::from(filetype)) };
}

fn set_perm(entry: *mut ffi::archive_entry, perm: u32) {
    // SAFETY: `entry` is a live `archive_entry` from `archive_entry_new`.
    unsafe { ffi::archive_entry_set_perm(entry, perm) };
}

fn set_size(entry: *mut ffi::archive_entry, size: i64) {
    // SAFETY: `entry` is a live `archive_entry` from `archive_entry_new`.
    unsafe { ffi::archive_entry_set_size(entry, size) };
}

fn check_setup(archive: *mut ffi::archive, code: c_int, what: &str) -> Result<(), String> {
    accept_warn(archive, code).map_err(|message| {
        if message.is_empty() {
            format!("libarchive could not enable {what}")
        } else {
            message
        }
    })
}

fn accept_warn(archive: *mut ffi::archive, code: c_int) -> Result<(), String> {
    match status(code) {
        Status::Ok | Status::Eof => Ok(()),
        Status::Warn => {
            tracing::warn!(
                message = archive_error_message(archive).as_str(),
                "libarchive reports a warning"
            );
            Ok(())
        }
        Status::Retry | Status::Failed | Status::Fatal => Err(archive_error_message(archive)),
    }
}

fn archive_error_message(archive: *mut ffi::archive) -> String {
    // SAFETY: `archive` is a live libarchive handle on this thread.
    let ptr = unsafe { ffi::archive_error_string(archive) };
    // SAFETY: `ptr` is null or a libarchive error string valid until the
    // next error is set on this handle; `cstr_bytes` does not retain it.
    if let Some(bytes) = unsafe { ffi::cstr_bytes(ptr) } {
        return String::from_utf8_lossy(&bytes).into_owned();
    }
    // SAFETY: `archive` is still the live handle queried above.
    let errno = unsafe { ffi::archive_errno(archive) };
    format!("libarchive error {errno}")
}

fn entry_pathname(entry: *mut ffi::archive_entry) -> OsString {
    let utf8 = optional_os_string(
        // SAFETY: `entry` is the current header from `archive_read_next_header`.
        unsafe { ffi::archive_entry_pathname_utf8(entry) },
        // SAFETY: same lifetime as the UTF-8 accessor above.
        unsafe { ffi::archive_entry_pathname(entry) },
    );
    utf8.unwrap_or_default()
}

fn optional_os_string(
    utf8: *const std::ffi::c_char,
    fallback: *const std::ffi::c_char,
) -> Option<OsString> {
    // SAFETY: `utf8` is null or a libarchive string for the current entry.
    let utf8_bytes = unsafe { ffi::cstr_bytes(utf8) };
    if let Some(bytes) = utf8_bytes.filter(|bytes| !bytes.is_empty()) {
        return Some(os_from_bytes(&bytes));
    }
    // SAFETY: `fallback` is null or a libarchive string for the current entry.
    let fallback_bytes = unsafe { ffi::cstr_bytes(fallback) };
    fallback_bytes
        .filter(|bytes| !bytes.is_empty())
        .map(|bytes| os_from_bytes(&bytes))
}

fn os_from_bytes(bytes: &[u8]) -> OsString {
    OsStr::from_bytes(bytes).to_os_string()
}

fn entry_size(entry: *mut ffi::archive_entry) -> Option<u64> {
    // SAFETY: `entry` is the current live header.
    let set = unsafe { ffi::archive_entry_size_is_set(entry) };
    if set == 0 {
        return None;
    }
    // SAFETY: `entry` is still the current live header.
    let size = unsafe { ffi::archive_entry_size(entry) };
    u64::try_from(size).ok()
}

fn entry_mtime(entry: *mut ffi::archive_entry) -> Option<i64> {
    // SAFETY: `entry` is the current live header.
    let set = unsafe { ffi::archive_entry_mtime_is_set(entry) };
    if set == 0 {
        return None;
    }
    // SAFETY: `entry` is still the current live header.
    Some(unsafe { ffi::archive_entry_mtime(entry) })
}

fn entry_encrypted(entry: *mut ffi::archive_entry) -> bool {
    // SAFETY: `entry` is the current live header.
    if unsafe { ffi::archive_entry_is_encrypted(entry) } != 0 {
        return true;
    }
    // SAFETY: `entry` is still the current live header.
    if unsafe { ffi::archive_entry_is_data_encrypted(entry) } != 0 {
        return true;
    }
    // SAFETY: `entry` is still the current live header.
    let metadata = unsafe { ffi::archive_entry_is_metadata_encrypted(entry) };
    metadata != 0
}

#[cfg(test)]
mod tests {
    use std::{
        error::Error,
        ffi::OsString,
        fs::File,
        io::{Read as _, Write as _},
        path::Path,
    };

    use super::{
        ArchiveFormat, MemberKind, ReadArchive, Status, WriteArchive, ffi, member_kind,
        saw_encrypted_entries, status,
    };

    /// `ARCHIVE_OK` and unknown non-negative codes continue.
    #[test]
    fn status_ok() {
        assert_eq!(status(ffi::ARCHIVE_OK), Status::Ok);
        assert_eq!(status(2), Status::Ok);
    }

    /// `ARCHIVE_EOF` ends iteration.
    #[test]
    fn status_eof() {
        assert_eq!(status(ffi::ARCHIVE_EOF), Status::Eof);
    }

    /// `ARCHIVE_WARN` is distinct from failure so headers can continue.
    #[test]
    fn status_warn() {
        assert_eq!(status(ffi::ARCHIVE_WARN), Status::Warn);
    }

    /// `ARCHIVE_RETRY` is distinct so `next_header` can retry.
    #[test]
    fn status_retry() {
        assert_eq!(status(ffi::ARCHIVE_RETRY), Status::Retry);
    }

    /// Failed and fatal codes, and any other negative, are errors.
    #[test]
    fn status_failed_fatal() {
        assert_eq!(status(ffi::ARCHIVE_FAILED), Status::Failed);
        assert_eq!(status(ffi::ARCHIVE_FATAL), Status::Fatal);
        assert_eq!(status(ffi::ARCHIVE_FATAL - 1), Status::Fatal);
        assert_eq!(status(-1), Status::Failed);
    }

    /// Regular files, directories, and symlinks map to [`MemberKind`].
    #[test]
    fn member_kind_file_types() {
        assert_eq!(member_kind(ffi::AE_IFREG), MemberKind::File);
        assert_eq!(member_kind(ffi::AE_IFDIR), MemberKind::Directory);
        assert_eq!(member_kind(ffi::AE_IFLNK), MemberKind::Symlink);
        assert_eq!(member_kind(ffi::AE_IFBLK), MemberKind::Special);
        assert_eq!(member_kind(ffi::AE_IFCHR), MemberKind::Special);
        assert_eq!(member_kind(ffi::AE_IFIFO), MemberKind::Special);
        assert_eq!(member_kind(ffi::AE_IFSOCK), MemberKind::Special);
        assert_eq!(member_kind(0), MemberKind::Special);
    }

    /// Only a positive `archive_read_has_encrypted_entries` result counts.
    #[test]
    fn has_encrypted_entries_mapping() {
        assert!(saw_encrypted_entries(1));
        assert!(saw_encrypted_entries(2));
        assert!(!saw_encrypted_entries(0));
        assert!(!saw_encrypted_entries(
            ffi::ARCHIVE_READ_FORMAT_ENCRYPTION_DONT_KNOW
        ));
        assert!(!saw_encrypted_entries(
            ffi::ARCHIVE_READ_FORMAT_ENCRYPTION_UNSUPPORTED
        ));
    }

    /// Non-ASCII member names extract with the correct name under `LC_ALL=C`.
    #[test]
    fn non_ascii_member_name_under_c_locale() -> Result<(), Box<dyn Error>> {
        let created = c_locale_guard();
        let dir = tempfile::tempdir()?;
        let archive_path = dir.path().join("names.zip");
        let name = Path::new("café.txt");
        {
            let file = File::create(&archive_path)?;
            let mut writer = WriteArchive::create(file, ArchiveFormat::Zip, None)?;
            writer.write_file_header(name, 4, 0o644, None)?;
            writer.write_all(b"cafe")?;
            writer.finish_entry()?;
            writer.finish()?;
        }
        let mut reader = ReadArchive::open(&archive_path, None)?;
        let member = reader
            .next_member()?
            .ok_or("archive should contain one member")?;
        assert_eq!(
            member.pathname,
            OsString::from("café.txt"),
            "member name should survive a C locale"
        );
        let mut body = Vec::new();
        reader.read_to_end(&mut body)?;
        assert_eq!(body, b"cafe", "member body should match what was written");
        drop(created);
        Ok(())
    }

    struct CLocaleGuard {
        previous: libc::locale_t,
        created: libc::locale_t,
    }

    fn c_locale_guard() -> Option<CLocaleGuard> {
        // SAFETY: `c"C"` is a valid locale name. A null base allocates a new
        // locale object.
        let created =
            unsafe { libc::newlocale(libc::LC_ALL_MASK, c"C".as_ptr(), std::ptr::null_mut()) };
        if created.is_null() {
            return None;
        }
        // SAFETY: `created` is a live locale from `newlocale`. `uselocale`
        // only affects this thread.
        let previous = unsafe { libc::uselocale(created) };
        if previous.is_null() {
            // SAFETY: `created` is unused after a failed `uselocale`.
            unsafe { libc::freelocale(created) };
            return None;
        }
        Some(CLocaleGuard { previous, created })
    }

    impl Drop for CLocaleGuard {
        fn drop(&mut self) {
            // SAFETY: `previous` is the locale `uselocale` returned when this
            // guard was created.
            unsafe {
                libc::uselocale(self.previous);
            }
            // SAFETY: `created` came from `newlocale` and is no longer in use
            // after the restore above.
            unsafe {
                libc::freelocale(self.created);
            }
        }
    }
}
