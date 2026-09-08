// SPDX-License-Identifier: GPL-3.0-or-later

//! FFI declarations for libarchive 3.7.2 and later (`archive.h`, `archive_entry.h`).
//!
//! Signatures and ABI constants were checked against the 3.7.2 headers (Ubuntu
//! 24.04, the release-builder and glibc floor). Nothing else from those headers
//! is declared.

#![expect(
    unsafe_code,
    reason = "libarchive exposes archive read and write only through its C API"
)]

use std::{
    ffi::{CStr, c_char, c_int, c_long, c_uint, c_void},
    ptr,
};

use libc::{locale_t, mode_t, time_t};

/// Opaque `struct archive` from `archive.h`.
#[repr(C)]
pub struct archive {
    _opaque: [u8; 0],
}

/// Opaque `struct archive_entry` from `archive_entry.h`.
#[repr(C)]
pub struct archive_entry {
    _opaque: [u8; 0],
}

// Constants from `archive.h`. These are ABI values and have not changed.
pub const ARCHIVE_EOF: c_int = 1;
pub const ARCHIVE_OK: c_int = 0;
pub const ARCHIVE_RETRY: c_int = -10;
pub const ARCHIVE_WARN: c_int = -20;
pub const ARCHIVE_FAILED: c_int = -25;
pub const ARCHIVE_FATAL: c_int = -30;
pub const ARCHIVE_READ_FORMAT_ENCRYPTION_UNSUPPORTED: c_int = -2;
pub const ARCHIVE_READ_FORMAT_ENCRYPTION_DONT_KNOW: c_int = -1;

// File-type bits from `archive_entry.h`.
pub const AE_IFMT: mode_t = 0o170_000;
pub const AE_IFREG: mode_t = 0o100_000;
pub const AE_IFLNK: mode_t = 0o120_000;
pub const AE_IFSOCK: mode_t = 0o140_000;
pub const AE_IFCHR: mode_t = 0o020_000;
pub const AE_IFBLK: mode_t = 0o060_000;
pub const AE_IFDIR: mode_t = 0o040_000;
pub const AE_IFIFO: mode_t = 0o010_000;

/// Block size passed to [`archive_read_open_fd`]. libarchive documents 10240
/// as the POSIX default.
pub const READ_BLOCK_SIZE: usize = 10_240;

unsafe extern "C" {
    // Read lifecycle
    pub fn archive_read_new() -> *mut archive;
    pub fn archive_read_free(archive: *mut archive) -> c_int;

    // Read format allowlist
    pub fn archive_read_support_format_zip(archive: *mut archive) -> c_int;
    pub fn archive_read_support_format_7zip(archive: *mut archive) -> c_int;
    pub fn archive_read_support_format_tar(archive: *mut archive) -> c_int;
    pub fn archive_read_support_format_rar(archive: *mut archive) -> c_int;
    pub fn archive_read_support_format_rar5(archive: *mut archive) -> c_int;

    // Read filter allowlist
    pub fn archive_read_support_filter_gzip(archive: *mut archive) -> c_int;
    pub fn archive_read_support_filter_bzip2(archive: *mut archive) -> c_int;
    pub fn archive_read_support_filter_xz(archive: *mut archive) -> c_int;
    pub fn archive_read_support_filter_zstd(archive: *mut archive) -> c_int;

    // Read open / iterate
    pub fn archive_read_add_passphrase(archive: *mut archive, passphrase: *const c_char) -> c_int;
    pub fn archive_read_open_fd(archive: *mut archive, fd: c_int, block_size: usize) -> c_int;
    pub fn archive_read_next_header(archive: *mut archive, entry: *mut *mut archive_entry)
    -> c_int;
    pub fn archive_read_data(archive: *mut archive, buf: *mut c_void, len: usize) -> isize;
    pub fn archive_read_data_skip(archive: *mut archive) -> c_int;
    pub fn archive_read_has_encrypted_entries(archive: *mut archive) -> c_int;

    // Errors
    pub fn archive_errno(archive: *mut archive) -> c_int;
    pub fn archive_error_string(archive: *mut archive) -> *const c_char;

    // Entry accessors (read)
    pub fn archive_entry_pathname_utf8(entry: *mut archive_entry) -> *const c_char;
    pub fn archive_entry_pathname(entry: *mut archive_entry) -> *const c_char;
    pub fn archive_entry_filetype(entry: *mut archive_entry) -> mode_t;
    pub fn archive_entry_size(entry: *mut archive_entry) -> i64;
    pub fn archive_entry_size_is_set(entry: *mut archive_entry) -> c_int;
    pub fn archive_entry_mtime(entry: *mut archive_entry) -> time_t;
    pub fn archive_entry_mtime_is_set(entry: *mut archive_entry) -> c_int;
    pub fn archive_entry_symlink_utf8(entry: *mut archive_entry) -> *const c_char;
    pub fn archive_entry_symlink(entry: *mut archive_entry) -> *const c_char;
    pub fn archive_entry_hardlink_utf8(entry: *mut archive_entry) -> *const c_char;
    pub fn archive_entry_hardlink(entry: *mut archive_entry) -> *const c_char;
    pub fn archive_entry_is_encrypted(entry: *mut archive_entry) -> c_int;
    pub fn archive_entry_is_data_encrypted(entry: *mut archive_entry) -> c_int;
    pub fn archive_entry_is_metadata_encrypted(entry: *mut archive_entry) -> c_int;

    // Write lifecycle
    pub fn archive_write_new() -> *mut archive;
    pub fn archive_write_close(archive: *mut archive) -> c_int;
    pub fn archive_write_free(archive: *mut archive) -> c_int;

    // Write formats / filters
    pub fn archive_write_set_format_zip(archive: *mut archive) -> c_int;
    pub fn archive_write_set_format_7zip(archive: *mut archive) -> c_int;
    pub fn archive_write_set_format_pax_restricted(archive: *mut archive) -> c_int;
    pub fn archive_write_add_filter_gzip(archive: *mut archive) -> c_int;
    pub fn archive_write_add_filter_none(archive: *mut archive) -> c_int;
    pub fn archive_write_set_format_option(
        archive: *mut archive,
        module: *const c_char,
        option: *const c_char,
        value: *const c_char,
    ) -> c_int;
    pub fn archive_write_open_fd(archive: *mut archive, fd: c_int) -> c_int;
    pub fn archive_write_header(archive: *mut archive, entry: *mut archive_entry) -> c_int;
    pub fn archive_write_data(archive: *mut archive, buf: *const c_void, len: usize) -> isize;

    // Entry construction (write)
    pub fn archive_entry_new() -> *mut archive_entry;
    pub fn archive_entry_free(entry: *mut archive_entry);
    pub fn archive_entry_set_pathname_utf8(entry: *mut archive_entry, name: *const c_char);
    pub fn archive_entry_set_filetype(entry: *mut archive_entry, filetype: c_uint);
    pub fn archive_entry_set_perm(entry: *mut archive_entry, perm: mode_t);
    pub fn archive_entry_set_size(entry: *mut archive_entry, size: i64);
    pub fn archive_entry_set_symlink_utf8(entry: *mut archive_entry, target: *const c_char);
    pub fn archive_entry_set_mtime(entry: *mut archive_entry, mtime: time_t, nsec: c_long);
}

#[cfg(test)]
unsafe extern "C" {
    pub fn archive_write_add_filter_xz(archive: *mut archive) -> c_int;
    pub fn archive_write_add_filter_zstd(archive: *mut archive) -> c_int;
    pub fn archive_write_add_filter_bzip2(archive: *mut archive) -> c_int;
    pub fn archive_entry_set_hardlink_utf8(entry: *mut archive_entry, target: *const c_char);
}

/// Thread-local `LC_CTYPE=C.UTF-8` (falling back to the environment locale).
///
/// libarchive converts member names through the process `LC_CTYPE`. Rust
/// binaries start in the `C` locale, so this guard is applied around
/// `archive_read_next_header`, `archive_write_header`, and the `set_*_utf8`
/// calls. `uselocale` is per-thread and does not disturb GTK on the main
/// thread.
pub struct Utf8LocaleGuard {
    previous: locale_t,
}

struct CachedUtf8Locale {
    locale: locale_t,
}

impl CachedUtf8Locale {
    fn new() -> Self {
        Self {
            locale: new_utf8_locale(),
        }
    }
}

impl Drop for CachedUtf8Locale {
    fn drop(&mut self) {
        if self.locale.is_null() {
            return;
        }
        // SAFETY: `locale` came from `newlocale` in `new_utf8_locale` and is
        // not the global locale. This Drop runs at thread exit, after any
        // [`Utf8LocaleGuard`] on the thread has restored the previous locale.
        unsafe {
            libc::freelocale(self.locale);
        }
    }
}

thread_local! {
    static UTF8_LOCALE: CachedUtf8Locale = CachedUtf8Locale::new();
}

impl Utf8LocaleGuard {
    /// Installs a UTF-8 `LC_CTYPE` on this thread until the guard is dropped.
    ///
    /// Returns [`None`] if no UTF-8 locale could be created or installed.
    pub fn enter() -> Option<Self> {
        UTF8_LOCALE.with(|cached| {
            if cached.locale.is_null() {
                return None;
            }
            // SAFETY: `cached.locale` is a live `locale_t` from `newlocale`,
            // stored in this thread's thread-local for the thread lifetime.
            // `uselocale` only affects the calling thread.
            let previous = unsafe { libc::uselocale(cached.locale) };
            if previous.is_null() {
                return None;
            }
            Some(Self { previous })
        })
    }
}

impl Drop for Utf8LocaleGuard {
    fn drop(&mut self) {
        // SAFETY: `previous` is the locale `uselocale` returned when this
        // guard was created, so it is valid to restore on this thread.
        unsafe {
            libc::uselocale(self.previous);
        }
    }
}

fn new_utf8_locale() -> locale_t {
    let c_utf8 = c"C.UTF-8";
    // SAFETY: `c_utf8` is a valid NUL-terminated C string that outlives the
    // call. A null `base` asks `newlocale` to allocate a new locale object.
    let locale = unsafe { libc::newlocale(libc::LC_CTYPE_MASK, c_utf8.as_ptr(), ptr::null_mut()) };
    if !locale.is_null() {
        return locale;
    }
    let env = c"";
    // SAFETY: `env` is a valid empty C string meaning "the environment
    // locale". A null `base` allocates a new locale object.
    unsafe { libc::newlocale(libc::LC_CTYPE_MASK, env.as_ptr(), ptr::null_mut()) }
}

/// Copies a libarchive C string into an owned byte buffer.
///
/// The pointer is only valid until the next `archive_read_next_header` on
/// the same archive, so this copies immediately.
///
/// # Safety
///
/// `ptr` must be null or a valid NUL-terminated C string returned by
/// libarchive for the current entry.
pub unsafe fn cstr_bytes(ptr: *const c_char) -> Option<Vec<u8>> {
    if ptr.is_null() {
        return None;
    }
    // SAFETY: the caller guarantees `ptr` is a valid NUL-terminated string.
    Some(unsafe { CStr::from_ptr(ptr) }.to_bytes().to_vec())
}
