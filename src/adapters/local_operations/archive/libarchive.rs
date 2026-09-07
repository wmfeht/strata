// SPDX-License-Identifier: GPL-3.0-or-later

//! libarchive codecs via the `libarchive2` crate.
//!
//! Isolates format encoding and decoding. Filesystem policy — destination
//! pinning, path validation, zip-bomb limits — stays in the parent module.
//! Each archive object is created, used, and dropped on one worker thread.

use std::{
    ffi::{OsStr, OsString},
    fs::File,
    io::{self, Read, Write},
    os::fd::AsRawFd as _,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

use libarchive2::{
    ArchiveFormat as LibFormat, CompressionFormat, EntryMut, FileType, FormatOption,
    ReadArchive as LibReadArchive, WriteArchive as LibWriteArchive, ZipCompressionMethod,
};

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

    fn compression(self) -> CompressionFormat {
        match self {
            Self::Xz => CompressionFormat::Xz,
            Self::Zstd => CompressionFormat::Zstd,
            Self::Bzip2 => CompressionFormat::Bzip2,
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
}

/// Reader over a local archive file.
pub(super) struct ReadArchive {
    inner: LibReadArchive<'static>,
    _file: Option<File>,
}

impl ReadArchive {
    /// Opens `path` with every format and filter libarchive can read.
    ///
    /// Unencrypted archives are opened from a file descriptor so the path need
    /// not be UTF-8. A passphrase must be registered before libarchive opens
    /// the stream, so encrypted archives use the path-based API instead.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be opened, the reader cannot be
    /// allocated, the passphrase is rejected, or the archive cannot be opened.
    pub(super) fn open(path: &Path, password: Option<&str>) -> Result<Self, String> {
        if let Some(password) = password {
            let inner =
                LibReadArchive::open_with_passphrase(path, password).map_err(crate_error)?;
            return Ok(Self { inner, _file: None });
        }
        let file = File::open(path).map_err(|error| error.to_string())?;
        let inner = LibReadArchive::open_fd(file.as_raw_fd()).map_err(crate_error)?;
        Ok(Self {
            inner,
            _file: Some(file),
        })
    }

    /// Reads the next member header. `None` means the archive is exhausted.
    ///
    /// # Errors
    ///
    /// Returns an error if libarchive cannot parse the next header.
    pub(super) fn next_member(&mut self) -> Result<Option<ArchiveMember>, String> {
        let Some(entry) = self.inner.next_entry().map_err(crate_error)? else {
            return Ok(None);
        };
        Ok(Some(ArchiveMember {
            pathname: os_from_string(entry.pathname()),
            kind: member_kind(entry.file_type()),
            size: u64::try_from(entry.size()).ok(),
            symlink_target: entry.symlink().map(OsString::from),
            hardlink_target: entry.hardlink().map(OsString::from),
            mtime: unix_seconds(entry.mtime()),
        }))
    }

    /// Discards the current member body without writing it.
    ///
    /// # Errors
    ///
    /// Returns an error if libarchive cannot skip the data.
    pub(super) fn skip_data(&mut self) -> Result<(), String> {
        self.inner.skip_data().map_err(crate_error)
    }
}

impl Read for ReadArchive {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner.read_data(buf).map_err(io::Error::other)
    }
}

/// Writer that encodes members into an already-open staging file.
pub(super) struct WriteArchive {
    inner: Option<LibWriteArchive<'static>>,
    _file: File,
}

impl WriteArchive {
    /// Prepares a writer for `format` on `file`.
    ///
    /// ZIP and 7z honor `password` through libarchive's passphrase. TAR formats
    /// reject a password.
    ///
    /// # Errors
    ///
    /// Returns an error if the writer cannot be allocated, the format is not
    /// creatable, an option is rejected, or the file descriptor cannot be used.
    pub(super) fn create(
        file: File,
        format: ArchiveFormat,
        password: Option<&str>,
    ) -> Result<Self, String> {
        Self::create_with_zip_store(file, format, password, false)
    }

    /// Prepares a writer for `format` on `file`, optionally storing ZIP members
    /// uncompressed.
    ///
    /// `zip_stored` is ignored for non-ZIP formats. ZIP compression cannot be
    /// changed after the first header, so the method is chosen here.
    ///
    /// # Errors
    ///
    /// Returns an error if the writer cannot be allocated, the format is not
    /// creatable, an option is rejected, or the file descriptor cannot be used.
    pub(super) fn create_with_zip_store(
        file: File,
        format: ArchiveFormat,
        password: Option<&str>,
        zip_stored: bool,
    ) -> Result<Self, String> {
        if password.is_some() && !format.supports_password() {
            return Err("This format does not support passwords".to_owned());
        }
        let mut builder = LibWriteArchive::new();
        builder = match format {
            ArchiveFormat::Zip => {
                let method = if zip_stored {
                    ZipCompressionMethod::Store
                } else {
                    ZipCompressionMethod::Deflate
                };
                builder
                    .format(LibFormat::Zip)
                    .format_option(FormatOption::ZipCompressionMethod(method))
            }
            ArchiveFormat::SevenZ => builder.format(LibFormat::SevenZip),
            ArchiveFormat::TarGz => builder
                .format(LibFormat::TarPaxRestricted)
                .compression(CompressionFormat::Gzip),
            ArchiveFormat::Tar => builder
                .format(LibFormat::TarPaxRestricted)
                .compression(CompressionFormat::None),
        };
        if let Some(password) = password {
            builder = builder.passphrase(password);
        }
        let inner = builder.open_fd(file.as_raw_fd()).map_err(crate_error)?;
        Ok(Self {
            inner: Some(inner),
            _file: file,
        })
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
        let inner = LibWriteArchive::new()
            .format(LibFormat::TarPaxRestricted)
            .compression(filter.compression())
            .open_fd(file.as_raw_fd())
            .map_err(crate_error)?;
        Ok(Self {
            inner: Some(inner),
            _file: file,
        })
    }

    fn inner(&mut self) -> Result<&mut LibWriteArchive<'static>, String> {
        self.inner
            .as_mut()
            .ok_or_else(|| "Archive writer is closed".to_owned())
    }

    /// Writes a directory header for `path`.
    ///
    /// # Errors
    ///
    /// Returns an error if the pathname is not UTF-8 or libarchive rejects the header.
    pub(super) fn write_directory(&mut self, path: &Path) -> Result<(), String> {
        let mut entry = EntryMut::new();
        set_pathname(&mut entry, path)?;
        entry.set_file_type(FileType::Directory);
        entry.set_perm(0o755).map_err(crate_error)?;
        entry.set_size(0);
        self.inner()?.write_header(&entry).map_err(crate_error)
    }

    /// Writes a symlink header for `path` pointing at `target`.
    ///
    /// # Errors
    ///
    /// Returns an error if a name is not UTF-8 or libarchive rejects the header.
    pub(super) fn write_symlink(&mut self, path: &Path, target: &OsStr) -> Result<(), String> {
        let target = target.to_str().ok_or_else(|| {
            format!(
                "Cannot preserve the non-UTF-8 link target of {}.",
                path.display()
            )
        })?;
        let mut entry = EntryMut::new();
        set_pathname(&mut entry, path)?;
        entry.set_symlink(target).map_err(crate_error)?;
        entry.set_file_type(FileType::SymbolicLink);
        entry.set_perm(0o777).map_err(crate_error)?;
        entry.set_size(0);
        self.inner()?.write_header(&entry).map_err(crate_error)
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
        mtime: Option<i64>,
    ) -> Result<(), String> {
        let mut entry = EntryMut::new();
        set_pathname(&mut entry, path)?;
        entry.set_file_type(FileType::RegularFile);
        entry.set_perm(0o644).map_err(crate_error)?;
        entry.set_size(i64::try_from(size).unwrap_or(i64::MAX));
        if let Some(mtime) = mtime
            && let Some(time) = unix_time(mtime)
        {
            entry.set_mtime(time);
        }
        self.inner()?.write_header(&entry).map_err(crate_error)
    }

    /// Finishes the current regular-file member.
    ///
    /// libarchive closes the current entry on the next header or when the
    /// archive is finished, so this is a no-op.
    pub(super) fn finish_entry(&mut self) -> Result<(), String> {
        let _ = self.inner()?;
        Ok(())
    }

    /// Closes the archive and writes trailers. Consumes the writer.
    ///
    /// # Errors
    ///
    /// Returns an error if libarchive cannot close the writer.
    pub(super) fn finish(mut self) -> Result<(), String> {
        match self.inner.take() {
            Some(inner) => inner.finish().map_err(crate_error),
            None => Ok(()),
        }
    }
}

impl Write for WriteArchive {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let written = self
            .inner()
            .map_err(io::Error::other)?
            .write_data(buf)
            .map_err(io::Error::other)?;
        if written == 0 && !buf.is_empty() {
            return Err(io::Error::other("Archive writer accepted no data"));
        }
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn set_pathname(entry: &mut EntryMut, path: &Path) -> Result<(), String> {
    entry.set_pathname(path).map_err(|error| {
        if path.to_str().is_none() {
            format!("Archive path is not valid UTF-8: {}", path.display())
        } else {
            crate_error(error)
        }
    })
}

fn member_kind(file_type: FileType) -> MemberKind {
    match file_type {
        FileType::RegularFile => MemberKind::File,
        FileType::Directory => MemberKind::Directory,
        FileType::SymbolicLink => MemberKind::Symlink,
        FileType::BlockDevice
        | FileType::CharacterDevice
        | FileType::Fifo
        | FileType::Socket
        | FileType::Unknown => MemberKind::Special,
    }
}

fn os_from_string(value: Option<String>) -> OsString {
    OsString::from(value.unwrap_or_default())
}

fn unix_seconds(time: Option<SystemTime>) -> Option<i64> {
    time.and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .and_then(|duration| i64::try_from(duration.as_secs()).ok())
}

fn unix_time(seconds: i64) -> Option<SystemTime> {
    u64::try_from(seconds)
        .ok()
        .map(|seconds| UNIX_EPOCH + std::time::Duration::from_secs(seconds))
}

fn crate_error(error: libarchive2::Error) -> String {
    error.to_string()
}
