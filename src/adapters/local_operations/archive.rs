// SPDX-License-Identifier: GPL-3.0-or-later

//! Local archive compression and extraction for [`LocalOperationProvider`].
//!
//! Builds and unpacks archives through the system libarchive. Compression writes
//! through a `0o600` staging file and publishes the result only after the
//! encoder finishes, so a partial archive is never left at the destination
//! name. Extraction pins the destination directory and creates members with
//! `openat`/`mkdirat` and `NOFOLLOW`, after [`validated_archive_path`] rejects
//! absolute paths, `..`, and Windows drive prefixes. Zip-bomb limits bound
//! written bytes, member count, path depth, and expansion ratio.
//!
//! # Main entry point
//!
//! [`archive`] starts a cancellable compress or extract. Work runs on the
//! default [`glib::MainContext`] and the returned [`LoadHandle`] sets a
//! cancellation flag when dropped.
//!
//! [`LocalOperationProvider`]: super::LocalOperationProvider

#[cfg(test)]
mod tests;

mod libarchive;

use std::{
    collections::{HashMap, HashSet},
    ffi::{OsStr, OsString},
    io::{self, BufReader, Read, Write},
    os::{
        fd::{AsFd, OwnedFd},
        unix::{
            ffi::{OsStrExt, OsStringExt},
            fs::PermissionsExt,
        },
    },
    path::{Component, Path, PathBuf},
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::UNIX_EPOCH,
};

#[cfg(test)]
use core::cell::Cell;

use libarchive::{ArchiveMember, DecoderError, MemberKind, ReadArchive, WriteArchive};

use gtk::{gio, glib};

use super::{local_directory_children, open_local_child_directory, open_local_parent_directory};
use crate::{
    model::Location,
    services::{
        ArchiveAction, ArchiveFormat, ArchiveRequest, CancelledOperation, LoadHandle,
        OperationEvent, OperationRequestId, TransferConflict, validate_basename,
    },
};

/// Writes an archive through a `0o600` staging file, then publishes it at `archive_path`.
///
/// Staging keeps a partial archive off the destination name. [`FailIfExists`]
/// refuses to replace; [`ReplaceExisting`] overwrites and copies the existing
/// regular file's mode. Otherwise the published mode is `0o666` masked by the
/// process umask (read from `/proc` to avoid the `umask(2)` race).
///
/// # Errors
///
/// - [`Cancelled`] if `cancelled` is set after encoding finishes
/// - [`Failed`] if staging, encoding, permission updates, or persist fail
///
/// [`Cancelled`]: ArchiveError::Cancelled
/// [`Failed`]: ArchiveError::Failed
/// [`FailIfExists`]: TransferConflict::FailIfExists
/// [`ReplaceExisting`]: TransferConflict::ReplaceExisting
async fn write_staged_archive<F>(
    destination: &Path,
    archive_path: &Path,
    conflict: TransferConflict,
    cancelled: &AtomicBool,
    write_archive: F,
) -> Result<(), ArchiveError>
where
    F: FnOnce(std::fs::File) -> Result<(), ArchiveError> + Send + 'static,
{
    let published_permissions = if conflict == TransferConflict::ReplaceExisting {
        match std::fs::symlink_metadata(archive_path) {
            Ok(metadata) if metadata.file_type().is_file() => Some(metadata.permissions()),
            Ok(_) => None,
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(archive_failed(error)),
        }
    } else {
        None
    }
    .unwrap_or_else(umask_adjusted_file_permissions);
    let mut builder = tempfile::Builder::new();
    builder
        .prefix(".strata-compression-")
        .permissions(std::fs::Permissions::from_mode(0o600));
    let staged = builder.tempfile_in(destination).map_err(archive_failed)?;
    let file = staged.reopen().map_err(archive_failed)?;
    gio::spawn_blocking(move || write_archive(file))
        .await
        .map_err(|_| archive_failed("Compression task panicked"))??;
    check_archive_cancelled(cancelled)?;
    staged
        .as_file()
        .set_permissions(published_permissions)
        .map_err(archive_failed)?;
    match conflict {
        TransferConflict::FailIfExists => staged.persist_noclobber(archive_path),
        TransferConflict::ReplaceExisting => staged.persist(archive_path),
    }
    .map(|_| ())
    .map_err(archive_failed)
}

fn umask_adjusted_file_permissions() -> std::fs::Permissions {
    std::fs::Permissions::from_mode(0o666 & !process_umask())
}

/// Reads the process umask from `/proc/self/status`.
///
/// Avoids the process-global `umask(2)` set-and-restore race that would
/// otherwise be unsafe in a multi-threaded GUI. Returns `0o022` when `/proc`
/// is unavailable or the `Umask:` line cannot be parsed.
fn process_umask() -> u32 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status.lines().find_map(|line| {
                line.strip_prefix("Umask:")
                    .and_then(|value| u32::from_str_radix(value.trim(), 8).ok())
            })
        })
        .unwrap_or(0o022)
}

/// Starts a cancellable compress or extract from `request`.
///
/// Dispatches on [`ArchiveAction`]. Returns a [`LoadHandle`] that cancels
/// in-flight work when dropped.
pub(super) fn archive(request: ArchiveRequest, emit: Rc<dyn Fn(OperationEvent)>) -> LoadHandle {
    match request.action {
        ArchiveAction::Compress {
            sources,
            archive_name,
            format,
            conflict,
        } => compress(
            request.id,
            request.destination,
            sources,
            archive_name,
            format,
            conflict,
            emit,
        ),
        ArchiveAction::Extract { archive, password } => {
            extract(request.id, request.destination, archive, password, emit)
        }
    }
}

/// Compresses `sources` into a new local archive in `destination`.
///
/// Validates that `destination` is a local path and that the archive stem
/// passes [`validate_basename`], then writes the published filename through
/// [`write_staged_archive`]. Progress is polled every 100 ms via
/// [`archive_progress_timer`]. Returns a [`LoadHandle`] that cancels in-flight
/// work when dropped.
///
/// Emits [`ArchiveStarted`] immediately, [`ArchiveProgress`] while running,
/// then [`Archived`], [`Failed`], or [`Cancelled`] depending on the outcome.
///
/// # Concurrency
///
/// Runs on the default [`glib::MainContext`]. Archive encoding happens on a
/// worker thread via [`gio::spawn_blocking`].
///
/// [`ArchiveStarted`]: OperationEvent::ArchiveStarted
/// [`ArchiveProgress`]: OperationEvent::ArchiveProgress
/// [`Archived`]: OperationEvent::Archived
/// [`Failed`]: OperationEvent::Failed
/// [`Cancelled`]: OperationEvent::Cancelled
fn compress(
    request_id: OperationRequestId,
    destination: Location,
    sources: Vec<Location>,
    archive_name: String,
    format: ArchiveFormat,
    conflict: TransferConflict,
    emit: Rc<dyn Fn(OperationEvent)>,
) -> LoadHandle {
    let cancelled = Arc::new(AtomicBool::new(false));
    let task_cancelled = cancelled.clone();
    let work_cancelled = cancelled.clone();
    let source_locations = sources.clone();
    let _task = glib::MainContext::default().spawn_local(async move {
        let Some(dest_dir) = destination.native_path().map(Path::to_path_buf) else {
            emit(OperationEvent::Failed {
                request_id,
                message: "Archive destination must be a local path".to_owned(),
            });
            return;
        };
        if let Err(message) = validate_basename(&archive_name) {
            emit(OperationEvent::Failed {
                request_id,
                message: message.to_owned(),
            });
            return;
        }
        let archive_name = format.archive_filename(&archive_name);
        let archive_path = dest_dir.join(&archive_name);
        let entries: Vec<std::path::PathBuf> = sources
            .iter()
            .filter_map(|location| location.native_path().map(Path::to_path_buf))
            .collect();
        if entries.is_empty() {
            emit(OperationEvent::Failed {
                request_id,
                message: "Nothing to compress".to_owned(),
            });
            return;
        }
        let total = Arc::new(AtomicUsize::new(0));
        let progress = Arc::new(AtomicUsize::new(0));
        emit(OperationEvent::ArchiveStarted { request_id });
        let timer_id =
            archive_progress_timer(request_id, &progress, &total, &task_cancelled, &emit);
        let work_progress = progress.clone();
        let work_total = total.clone();
        let result = write_staged_archive(
            &dest_dir,
            &archive_path,
            conflict,
            &task_cancelled,
            move |file| {
                let count = count_archive_files(&entries, &work_cancelled)?;
                work_total.store(count, Ordering::Relaxed);
                compress_entries(file, &entries, format, &work_progress, &work_cancelled)
            },
        )
        .await;
        timer_id.remove();
        match result {
            Ok(()) => emit(OperationEvent::Archived {
                request_id,
                select_name: archive_name,
            }),
            Err(ArchiveError::Cancelled) => emit(cancelled_archive_event(
                request_id,
                destination,
                Vec::new(),
                Vec::new(),
                source_locations,
            )),
            Err(ArchiveError::Failed(error) | ArchiveError::NeedsPassword(error)) => {
                emit(OperationEvent::Failed {
                    request_id,
                    message: error,
                });
            }
        }
    });
    LoadHandle::new(move || {
        cancelled.store(true, Ordering::Relaxed);
    })
}

/// Extracts `archive` into a local `destination` directory.
///
/// Requires both the archive and destination to be local paths. Format is
/// detected by libarchive from the file bytes, not from the name. Returns a
/// [`LoadHandle`] that cancels in-flight work when dropped.
///
/// Emits [`ArchiveStarted`] immediately, [`ArchiveProgress`] while running,
/// then [`Archived`], [`PasswordRequired`], [`Failed`], or [`Cancelled`]. A
/// cancel after some members have been written reports completed, failed, and
/// not-attempted locations through [`CancelledOperation`].
///
/// # Concurrency
///
/// Runs on the default [`glib::MainContext`]. Decoding happens on a worker
/// thread via [`gio::spawn_blocking`].
///
/// [`ArchiveStarted`]: OperationEvent::ArchiveStarted
/// [`ArchiveProgress`]: OperationEvent::ArchiveProgress
/// [`Archived`]: OperationEvent::Archived
/// [`PasswordRequired`]: OperationEvent::PasswordRequired
/// [`Failed`]: OperationEvent::Failed
/// [`Cancelled`]: OperationEvent::Cancelled
fn extract(
    request_id: OperationRequestId,
    destination: Location,
    archive: Location,
    password: Option<String>,
    emit: Rc<dyn Fn(OperationEvent)>,
) -> LoadHandle {
    let cancelled = Arc::new(AtomicBool::new(false));
    let task_cancelled = cancelled.clone();
    let work_cancelled = cancelled.clone();
    let _task = glib::MainContext::default().spawn_local(async move {
        let Some(archive_path) = archive.native_path().map(Path::to_path_buf) else {
            emit(OperationEvent::Failed {
                request_id,
                message: "Archive must be a local file".to_owned(),
            });
            return;
        };
        let Some(dest_dir) = destination.native_path().map(Path::to_path_buf) else {
            emit(OperationEvent::Failed {
                request_id,
                message: "Extract destination must be a local path".to_owned(),
            });
            return;
        };
        let progress = Arc::new(AtomicUsize::new(0));
        // Extract totals stay 0: libarchive is a streaming reader, so ZIP/7z
        // central-directory counts are not available up front. The progress
        // view treats a zero total as indeterminate ("N files").
        let total = Arc::new(AtomicUsize::new(0));
        emit(OperationEvent::ArchiveStarted { request_id });
        let timer_id =
            archive_progress_timer(request_id, &progress, &total, &task_cancelled, &emit);
        let work_progress = progress.clone();
        let result = gio::spawn_blocking(move || {
            extract_archive(
                &archive_path,
                &dest_dir,
                password.as_deref(),
                &work_progress,
                &work_cancelled,
            )
        })
        .await;
        timer_id.remove();
        match result {
            Ok(Ok(ArchiveOutcome::Completed(first_name))) => emit(OperationEvent::Archived {
                request_id,
                select_name: first_name.unwrap_or_default(),
            }),
            Ok(Ok(ArchiveOutcome::Cancelled {
                completed,
                failed,
                not_attempted,
            })) => emit(cancelled_archive_event(
                request_id,
                destination,
                completed,
                failed,
                not_attempted,
            )),
            Ok(Err(ArchiveError::Cancelled)) => emit(cancelled_archive_event(
                request_id,
                destination,
                Vec::new(),
                Vec::new(),
                Vec::new(),
            )),
            Ok(Err(ArchiveError::NeedsPassword(_))) => {
                emit(OperationEvent::PasswordRequired {
                    request_id,
                    archive,
                    destination,
                });
            }
            Ok(Err(ArchiveError::Failed(error))) => emit(OperationEvent::Failed {
                request_id,
                message: error,
            }),
            Err(_) => emit(OperationEvent::Failed {
                request_id,
                message: "Extraction task panicked".to_owned(),
            }),
        }
    });
    LoadHandle::new(move || {
        cancelled.store(true, Ordering::Relaxed);
    })
}

/// An opened compression source, re-read from disk relative to its parent
/// directory rather than trusted from any earlier listing.
enum ArchiveSource {
    /// Open file description for a regular file, opened with `BENEATH`,
    /// `NO_SYMLINKS`, and `NO_MAGICLINKS`.
    File(std::fs::File),
    /// Open directory used to walk children descriptor-relative.
    Directory(std::fs::File),
    /// Symlink target as stored, archived as a link rather than followed.
    Symlink(PathBuf),
}

/// Opens the child named `name` inside `parent` without following symbolic links.
///
/// Regular files are opened with [`rustix::fs::ResolveFlags::BENEATH`],
/// [`rustix::fs::ResolveFlags::NO_SYMLINKS`], and
/// [`rustix::fs::ResolveFlags::NO_MAGICLINKS`]. Directories go through
/// [`open_local_child_directory`]. Symlinks are read with `readlinkat` and
/// stored as [`ArchiveSource::Symlink`].
///
/// # Errors
///
/// Returns an error if `name` cannot be inspected, is an unsupported file
/// type, or changes from a regular file between `statat` and `openat2`.
fn open_archive_source<Fd: AsFd>(parent: &Fd, name: &OsStr) -> Result<ArchiveSource, String> {
    let stat = rustix::fs::statat(parent, name, rustix::fs::AtFlags::SYMLINK_NOFOLLOW)
        .map_err(|error| error.to_string())?;
    match rustix::fs::FileType::from_raw_mode(stat.st_mode) {
        rustix::fs::FileType::Symlink => {
            let target = rustix::fs::readlinkat(parent, name, Vec::new())
                .map_err(|error| error.to_string())?;
            Ok(ArchiveSource::Symlink(PathBuf::from(OsString::from_vec(
                target.into_bytes(),
            ))))
        }
        rustix::fs::FileType::Directory => open_local_child_directory(parent, name)
            .map(std::fs::File::from)
            .map(ArchiveSource::Directory),
        rustix::fs::FileType::RegularFile => {
            let file = rustix::fs::openat2(
                parent,
                name,
                rustix::fs::OFlags::RDONLY
                    | rustix::fs::OFlags::NONBLOCK
                    | rustix::fs::OFlags::CLOEXEC,
                rustix::fs::Mode::empty(),
                rustix::fs::ResolveFlags::BENEATH
                    | rustix::fs::ResolveFlags::NO_SYMLINKS
                    | rustix::fs::ResolveFlags::NO_MAGICLINKS,
            )
            .map(std::fs::File::from)
            .map_err(|error| error.to_string())?;
            if !file
                .metadata()
                .map_err(|error| error.to_string())?
                .is_file()
            {
                return Err("The file type changed during compression".to_owned());
            }
            Ok(ArchiveSource::File(file))
        }
        _ => Err("Compression supports only regular files, folders, and symbolic links".to_owned()),
    }
}

/// Walks each path in `entries` and its descendants, calling `visit` per member.
///
/// Selected roots are opened from their parent directory via
/// [`open_local_parent_directory`], then recursion stays descriptor-relative
/// through [`visit_archive_entry`].
///
/// # Errors
///
/// - [`Cancelled`] if `cancelled` is set before or during the walk
/// - [`Failed`] if a root has no file name or parent, cannot be opened, or
///   `visit` fails
///
/// [`Cancelled`]: ArchiveError::Cancelled
/// [`Failed`]: ArchiveError::Failed
fn visit_archive_entries(
    entries: &[PathBuf],
    cancelled: &AtomicBool,
    visit: &mut impl FnMut(&Path, &ArchiveSource) -> Result<(), ArchiveError>,
) -> Result<(), ArchiveError> {
    for entry in entries {
        check_archive_cancelled(cancelled)?;
        let name = entry.file_name().ok_or("Entry has no file name")?;
        let parent = open_local_parent_directory(entry.parent().ok_or("Entry has no parent")?)?;
        visit_archive_entry(&parent, name, Path::new(name), cancelled, visit)?;
    }
    Ok(())
}

/// Visits `name` inside `parent` and, for directories, each child beneath it.
///
/// `archive_path` is the member path written into the archive, rooted at the
/// originally selected entry's file name.
///
/// # Errors
///
/// - [`Cancelled`] if `cancelled` is set
/// - [`Failed`] if the entry cannot be opened or `visit` fails
///
/// [`Cancelled`]: ArchiveError::Cancelled
/// [`Failed`]: ArchiveError::Failed
fn visit_archive_entry<Fd: AsFd>(
    parent: &Fd,
    name: &OsStr,
    archive_path: &Path,
    cancelled: &AtomicBool,
    visit: &mut impl FnMut(&Path, &ArchiveSource) -> Result<(), ArchiveError>,
) -> Result<(), ArchiveError> {
    check_archive_cancelled(cancelled)?;
    let source = open_archive_source(parent, name).map_err(|error| {
        archive_failed(format!(
            "Could not compress {}: {error}",
            archive_path.display()
        ))
    })?;
    visit(archive_path, &source)?;
    if let ArchiveSource::Directory(directory) = source {
        for child in local_directory_children(&directory)? {
            visit_archive_entry(
                &directory,
                &child,
                &archive_path.join(&child),
                cancelled,
                visit,
            )?;
        }
    }
    Ok(())
}

/// Writes an archive of `entries` into `file` using libarchive.
///
/// ZIP uses deflate for every regular file; libarchive cannot change ZIP
/// compression after the first header. Passwords are rejected: libarchive
/// cannot write encrypted archives. 7z rejects symbolic links. File modes are
/// preserved from the source (masked to `0o7777`). Non-UTF-8 paths and link
/// targets are refused because libarchive writes pathnames as UTF-8; one such
/// name aborts the whole archive.
///
/// # Errors
///
/// - [`Cancelled`] if `cancelled` is set during the walk or copy
/// - [`Failed`] if a member cannot be opened or encoding fails
///
/// [`Cancelled`]: ArchiveError::Cancelled
/// [`Failed`]: ArchiveError::Failed
fn compress_entries(
    file: std::fs::File,
    entries: &[PathBuf],
    format: ArchiveFormat,
    progress: &Arc<AtomicUsize>,
    cancelled: &AtomicBool,
) -> Result<(), ArchiveError> {
    let mut writer = WriteArchive::create(file, format, None)?;
    visit_archive_entries(entries, cancelled, &mut |path, source| {
        match source {
            ArchiveSource::Directory(directory) => {
                let perm = source_perm(directory, 0o755);
                writer.write_directory(path, perm)?;
            }
            ArchiveSource::Symlink(target) => {
                if format == ArchiveFormat::SevenZ {
                    return Err(archive_failed(format!(
                        "7z compression does not support symbolic links: {}. Use ZIP or TAR instead.",
                        path.display()
                    )));
                }
                if target.to_str().is_none() {
                    return Err(archive_failed(format!(
                        "Cannot preserve the non-UTF-8 link target of {}.",
                        path.display()
                    )));
                }
                writer.write_symlink(path, target.as_os_str())?;
                progress.fetch_add(1, Ordering::Relaxed);
            }
            ArchiveSource::File(file) => {
                let metadata = file.metadata().map_err(archive_failed)?;
                let perm = metadata_perm(&metadata, 0o644);
                let mtime = file_mtime(&metadata);
                let size = metadata.len();
                writer.write_file_header(path, size, perm, mtime)?;
                // Cap the copy at the header size so a file that grows after
                // `metadata()` cannot overflow libarchive's declared entry.
                let copied = copy_with_big_buf(
                    BufReader::with_capacity(COPY_BUF, file).take(size),
                    &mut writer,
                    cancelled,
                    |_| Ok(()),
                )?;
                if copied != size {
                    return Err(archive_failed(format!(
                        "Source file {} changed size while it was being archived",
                        path.display()
                    )));
                }
                writer.finish_entry()?;
                progress.fetch_add(1, Ordering::Relaxed);
            }
        }
        Ok(())
    })?;
    check_archive_cancelled(cancelled)?;
    writer.finish()?;
    Ok(())
}

fn source_perm(file: &std::fs::File, fallback: u32) -> u32 {
    file.metadata()
        .map(|metadata| metadata_perm(&metadata, fallback))
        .unwrap_or(fallback)
}

fn metadata_perm(metadata: &std::fs::Metadata, fallback: u32) -> u32 {
    use std::os::unix::fs::PermissionsExt as _;
    let mode = metadata.permissions().mode() & 0o7777;
    // A zero mode means the metadata gave nothing useful; keep the fallback
    // (`0o644` for files, `0o755` for directories) instead of writing `---`.
    if mode == 0 { fallback } else { mode }
}

fn file_mtime(metadata: &std::fs::Metadata) -> Option<i64> {
    metadata
        .modified()
        .ok()
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .and_then(|duration| i64::try_from(duration.as_secs()).ok())
}

/// Converts an archive member name into a relative path that cannot escape the destination.
///
/// Normalizes backslashes to slashes, skips empty and `.` components, and
/// rejects absolute paths, `..`, Windows drive prefixes (`C:`), and names
/// that collapse to empty.
///
/// # Errors
///
/// Returns an error if `name` is empty, absolute, contains `..`, includes a
/// drive prefix, or has no remaining components after normalization.
#[cfg(test)]
fn validated_archive_path(name: &str) -> Result<PathBuf, String> {
    validated_archive_os_path(OsStr::new(name))
}

/// Converts an archive member name into a relative path that cannot escape the destination.
///
/// Normalizes backslashes to slashes, skips empty and `.` components, and
/// rejects absolute paths, `..`, Windows drive prefixes (`C:`), and names
/// that collapse to empty. Preserves non-UTF-8 bytes in remaining components.
///
/// # Errors
///
/// Returns an error if `name` is empty, absolute, contains `..`, includes a
/// drive prefix, or has no remaining components after normalization.
fn validated_archive_os_path(name: &OsStr) -> Result<PathBuf, String> {
    let display = name.to_string_lossy();
    let bytes = name.as_bytes();
    if bytes.is_empty() || bytes[0] == b'/' || bytes[0] == b'\\' {
        return Err(format!("Refusing unsafe archive path: {display}"));
    }

    let mut path = PathBuf::new();
    for component in bytes.split(|byte| *byte == b'/' || *byte == b'\\') {
        match component {
            b"" | b"." => {}
            b".." => return Err(format!("Refusing unsafe archive path: {display}")),
            bytes if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' => {
                return Err(format!("Refusing unsafe archive path: {display}"));
            }
            other => path.push(OsStr::from_bytes(other)),
        }
    }
    if path.as_os_str().is_empty() {
        return Err(format!("Refusing empty archive path: {display}"));
    }
    Ok(path)
}

/// Returns `name` with ` ({index})` inserted before the extension.
///
/// Used by [`ExtractionDestination::available_name`] to pick `readme (2).txt`
/// when `readme.txt` already exists.
fn suffixed_name(name: &OsStr, index: u64) -> OsString {
    let path = Path::new(name);
    let mut candidate = path.file_stem().unwrap_or(name).as_bytes().to_vec();
    candidate.extend_from_slice(format!(" ({index})").as_bytes());
    if let Some(extension) = path.extension() {
        candidate.push(b'.');
        candidate.extend_from_slice(extension.as_bytes());
    }
    OsString::from_vec(candidate)
}

/// Splits a ` (N)` numeric suffix off `stem`, if present.
///
/// Returns the base stem without the suffix and `N`. Used so a second
/// collision on an already-renamed `a (2).txt` becomes `a (3).txt` rather
/// than `a (2) (2).txt`.
fn split_numeric_suffix(stem: &[u8]) -> Option<(Vec<u8>, u64)> {
    if !stem.ends_with(b")") {
        return None;
    }
    let open = stem.iter().rposition(|&byte| byte == b'(')?;
    if open == 0 || stem[open - 1] != b' ' {
        return None;
    }
    let digits = &stem[open + 1..stem.len() - 1];
    if digits.is_empty() || !digits.iter().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let base = &stem[..open - 1];
    if base.is_empty() {
        return None;
    }
    let number = std::str::from_utf8(digits).ok()?.parse::<u64>().ok()?;
    Some((base.to_vec(), number))
}

/// Returns the `index`-th candidate for `name`, incrementing an existing
/// ` (N)` suffix instead of stacking another one.
fn suffixed_candidate(name: &OsStr, index: u64) -> OsString {
    let path = Path::new(name);
    let stem = path.file_stem().unwrap_or(name);
    if let Some((base, number)) = split_numeric_suffix(stem.as_bytes()) {
        let next = number.saturating_add(index.saturating_sub(1));
        let mut candidate = base;
        candidate.extend_from_slice(format!(" ({next})").as_bytes());
        if let Some(extension) = path.extension() {
            candidate.push(b'.');
            candidate.extend_from_slice(extension.as_bytes());
        }
        OsString::from_vec(candidate)
    } else {
        suffixed_name(name, index)
    }
}

/// Pinned destination directory for extraction.
///
/// All member creates go through this root with `NOFOLLOW`, so a symlink
/// swapped into the destination tree cannot redirect writes outside it.
struct ExtractionDestination {
    root: OwnedFd,
}

impl ExtractionDestination {
    /// Opens `path` as a directory without following a final symbolic link.
    ///
    /// # Errors
    ///
    /// Returns an error if `path` cannot be opened as a directory.
    fn open(path: &Path) -> Result<Self, String> {
        let root = rustix::fs::open(
            path,
            rustix::fs::OFlags::RDONLY
                | rustix::fs::OFlags::DIRECTORY
                | rustix::fs::OFlags::NOFOLLOW
                | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )
        .map_err(|error| format!("Could not open extraction destination: {error}"))?;
        Ok(Self { root })
    }

    /// Finds a name in `directory` that does not already exist.
    ///
    /// Tries `name`, then [`suffixed_candidate`] with increasing indexes.
    /// An existing ` (N)` suffix is incremented (`a (2).txt` -> `a (3).txt`)
    /// so a duplicate of an already-renamed top level does not stack to
    /// `a (2) (2).txt`. Existing regular files and directories are skipped;
    /// special filesystem objects (devices, sockets, existing symlinks) are
    /// refused rather than overwritten.
    ///
    /// # Errors
    ///
    /// Returns an error if `directory` cannot be inspected or an existing
    /// candidate is a special filesystem object.
    fn available_name<Fd: AsFd>(&self, directory: &Fd, name: &OsStr) -> Result<OsString, String> {
        for index in 1.. {
            let candidate = if index == 1 {
                name.to_owned()
            } else {
                suffixed_candidate(name, index)
            };
            match rustix::fs::statat(directory, &candidate, rustix::fs::AtFlags::SYMLINK_NOFOLLOW) {
                Err(rustix::io::Errno::NOENT) => return Ok(candidate),
                Err(error) => {
                    return Err(format!(
                        "Could not inspect extraction path {}: {error}",
                        candidate.to_string_lossy()
                    ));
                }
                Ok(stat) => match rustix::fs::FileType::from_raw_mode(stat.st_mode) {
                    rustix::fs::FileType::RegularFile | rustix::fs::FileType::Directory => {}
                    _ => {
                        return Err(format!(
                            "Refusing to extract over special filesystem object: {}",
                            candidate.to_string_lossy()
                        ));
                    }
                },
            }
        }
        Err(format!(
            "Could not find an available extraction name for {}",
            name.to_string_lossy()
        ))
    }

    /// Creates each component of `path` under the destination root and returns the leaf directory.
    ///
    /// Existing directories are reused. Each component is opened with
    /// [`DIRECTORY`] and [`NOFOLLOW`], so a symlink cannot be followed as a
    /// directory.
    ///
    /// # Errors
    ///
    /// Returns an error if `path` contains a non-normal component, a component
    /// cannot be created, or a component exists but is not a directory.
    ///
    /// [`DIRECTORY`]: rustix::fs::OFlags::DIRECTORY
    /// [`NOFOLLOW`]: rustix::fs::OFlags::NOFOLLOW
    fn create_directories(&self, path: &Path) -> Result<OwnedFd, String> {
        let mut directory = self.root.try_clone().map_err(|error| error.to_string())?;
        for component in path.components() {
            let Component::Normal(name) = component else {
                return Err("Invalid internal extraction path".to_owned());
            };
            match rustix::fs::mkdirat(&directory, name, rustix::fs::Mode::from_raw_mode(0o777)) {
                Ok(()) | Err(rustix::io::Errno::EXIST) => {}
                Err(error) => return Err(error.to_string()),
            }
            directory = rustix::fs::openat(
                &directory,
                name,
                rustix::fs::OFlags::RDONLY
                    | rustix::fs::OFlags::DIRECTORY
                    | rustix::fs::OFlags::NOFOLLOW
                    | rustix::fs::OFlags::CLOEXEC,
                rustix::fs::Mode::empty(),
            )
            .map_err(|error| error.to_string())?;
        }
        Ok(directory)
    }

    /// Creates the file at `path`, renaming the leaf if that name is already taken.
    ///
    /// Parent directories are created with [`Self::create_directories`]. The
    /// leaf is opened with [`CREATE`], [`EXCL`], and [`NOFOLLOW`] so an existing
    /// file or symlink is never overwritten. Returns the open file and the
    /// relative path actually created, which may differ from `path` after a
    /// rename.
    ///
    /// # Errors
    ///
    /// Returns an error if `path` has no file name, a parent cannot be created,
    /// no unused name can be found, or the exclusive create fails.
    ///
    /// [`CREATE`]: rustix::fs::OFlags::CREATE
    /// [`EXCL`]: rustix::fs::OFlags::EXCL
    /// [`NOFOLLOW`]: rustix::fs::OFlags::NOFOLLOW
    fn create_file(&self, path: &Path) -> Result<(std::fs::File, PathBuf), String> {
        let parent = self.create_directories(path.parent().unwrap_or_else(|| Path::new("")))?;
        let name = path
            .file_name()
            .ok_or_else(|| "Archive entry has no file name".to_owned())?;
        let name = self.available_name(&parent, name)?;
        let mut created = PathBuf::new();
        if let Some(parent_path) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            created.push(parent_path);
        }
        created.push(&name);
        let file = rustix::fs::openat(
            parent,
            name,
            rustix::fs::OFlags::WRONLY
                | rustix::fs::OFlags::CREATE
                | rustix::fs::OFlags::EXCL
                | rustix::fs::OFlags::NOFOLLOW
                | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::from_raw_mode(0o666),
        )
        .map(std::fs::File::from)
        .map_err(|error| error.to_string())?;
        Ok((file, created))
    }

    /// Creates a symlink at `path` with the stored `target`, renaming the leaf on conflict.
    ///
    /// # Errors
    ///
    /// Returns an error if `path` has no file name, a parent cannot be created,
    /// no unused name can be found, or `symlinkat` fails.
    fn create_symlink(&self, path: &Path, target: &OsStr) -> Result<PathBuf, String> {
        let parent = self.create_directories(path.parent().unwrap_or_else(|| Path::new("")))?;
        let name = path
            .file_name()
            .ok_or_else(|| "Archive entry has no file name".to_owned())?;
        let name = self.available_name(&parent, name)?;
        let mut created = PathBuf::new();
        if let Some(parent_path) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            created.push(parent_path);
        }
        created.push(&name);
        rustix::fs::symlinkat(target, &parent, &name).map_err(|error| error.to_string())?;
        Ok(created)
    }

    /// Opens each component of `path`'s parent under the destination root without creating it.
    ///
    /// Each component is opened with [`DIRECTORY`] and [`NOFOLLOW`]. Used to
    /// inspect an already-extracted hard-link target; missing parents are an
    /// error rather than a mkdir.
    ///
    /// [`DIRECTORY`]: rustix::fs::OFlags::DIRECTORY
    /// [`NOFOLLOW`]: rustix::fs::OFlags::NOFOLLOW
    fn open_parent_directory(&self, path: &Path) -> Result<OwnedFd, String> {
        let mut directory = self.root.try_clone().map_err(|error| error.to_string())?;
        let parent = path.parent().unwrap_or_else(|| Path::new(""));
        for component in parent.components() {
            let Component::Normal(name) = component else {
                return Err("Invalid internal extraction path".to_owned());
            };
            directory = rustix::fs::openat(
                &directory,
                name,
                rustix::fs::OFlags::RDONLY
                    | rustix::fs::OFlags::DIRECTORY
                    | rustix::fs::OFlags::NOFOLLOW
                    | rustix::fs::OFlags::CLOEXEC,
                rustix::fs::Mode::empty(),
            )
            .map_err(|error| error.to_string())?;
        }
        Ok(directory)
    }

    /// Hard-links `path` to the already-extracted regular file `target`.
    ///
    /// `linkat` is used instead of a copy so backup tarballs that share inodes
    /// do not multiply disk use or the write budget. Both sides are
    /// descriptor-relative with `NOFOLLOW`; a symlink target is refused.
    ///
    /// # Errors
    ///
    /// Returns an error if `path` or `target` has no file name, a parent cannot
    /// be opened, `target` is not a regular file, no unused name can be found,
    /// or `linkat` fails.
    fn create_hard_link(&self, path: &Path, target: &Path) -> Result<PathBuf, String> {
        let target_parent = self.open_parent_directory(target)?;
        let target_name = target
            .file_name()
            .ok_or_else(|| "Archive entry has no file name".to_owned())?;
        match rustix::fs::statat(
            &target_parent,
            target_name,
            rustix::fs::AtFlags::SYMLINK_NOFOLLOW,
        ) {
            Ok(stat)
                if rustix::fs::FileType::from_raw_mode(stat.st_mode)
                    == rustix::fs::FileType::RegularFile => {}
            Ok(_) => return Err("Hard link target is not a regular file".to_owned()),
            Err(error) => {
                return Err(format!(
                    "Could not inspect hard link target {}: {error}",
                    target.display()
                ));
            }
        }
        let parent = self.create_directories(path.parent().unwrap_or_else(|| Path::new("")))?;
        let name = path
            .file_name()
            .ok_or_else(|| "Archive entry has no file name".to_owned())?;
        let name = self.available_name(&parent, name)?;
        let mut created = PathBuf::new();
        if let Some(parent_path) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            created.push(parent_path);
        }
        created.push(&name);
        rustix::fs::linkat(
            &target_parent,
            target_name,
            &parent,
            &name,
            rustix::fs::AtFlags::empty(),
        )
        .map_err(|error| error.to_string())?;
        Ok(created)
    }

    /// Unlinks the leaf of `path` under the destination root.
    ///
    /// Used to discard a partially written member after cancellation or
    /// copy failure. Does not follow a final symbolic link.
    ///
    /// # Errors
    ///
    /// Returns an error if `path` has no file name, a parent cannot be opened,
    /// or the unlink fails.
    fn remove_file(&self, path: &Path) -> Result<(), String> {
        self.unlink(path, rustix::fs::AtFlags::empty())
    }

    /// Unlinks a file or directory created during this extract.
    ///
    /// Tries a file unlink first, then `rmdir` when the leaf is a directory.
    /// Used to roll back members already written if a later member needs a
    /// password, so a retry does not collide with leftover names.
    ///
    /// # Errors
    ///
    /// Returns an error if `path` has no file name, a parent cannot be opened,
    /// or both unlink attempts fail.
    fn remove_entry(&self, path: &Path) -> Result<(), String> {
        match self.unlink(path, rustix::fs::AtFlags::empty()) {
            Ok(()) => Ok(()),
            Err(_) => self.unlink(path, rustix::fs::AtFlags::REMOVEDIR),
        }
    }

    fn unlink(&self, path: &Path, flags: rustix::fs::AtFlags) -> Result<(), String> {
        let parent = self.open_parent_directory(path)?;
        let name = path
            .file_name()
            .ok_or_else(|| "Archive entry has no file name".to_owned())?;
        rustix::fs::unlinkat(&parent, name, flags).map_err(|error| {
            format!(
                "Could not remove incomplete extraction {}: {error}",
                path.display()
            )
        })
    }
}

/// Counts non-directory members under `entries` for progress totals.
///
/// Directories are visited so their children are counted, but the directories
/// themselves are excluded from the total.
///
/// # Errors
///
/// - [`Cancelled`] if `cancelled` is set during the walk
/// - [`Failed`] if a member cannot be opened
///
/// [`Cancelled`]: ArchiveError::Cancelled
/// [`Failed`]: ArchiveError::Failed
fn count_archive_files(entries: &[PathBuf], cancelled: &AtomicBool) -> Result<usize, ArchiveError> {
    let mut count = 0;
    visit_archive_entries(entries, cancelled, &mut |_, source| {
        if !matches!(source, ArchiveSource::Directory(_)) {
            count += 1;
        }
        Ok(())
    })?;
    Ok(count)
}

const COPY_BUF: usize = 1 << 20;
const ARCHIVE_CANCELLED: &str = "Operation cancelled";
const EXTRACT_DISK_RESERVE: u64 = 64 * 1024 * 1024;
const EXTRACT_MAX_MEMBERS: u32 = 1_000_000;
const EXTRACT_MAX_PATH_DEPTH: u16 = 256;
const EXTRACT_BOMB_RATIO: u32 = 200;
/// Ratio is not applied until this much has been written. Highly compressible
/// legitimate archives (xz/zstd of zeros or sparse images) routinely exceed
/// 200:1 while still small; the free-space check already bounds real damage.
const EXTRACT_BOMB_RATIO_FLOOR: u64 = 64 * 1024 * 1024;

/// Failure or cooperative cancellation of a compress or extract step.
#[derive(Debug, PartialEq, Eq)]
enum ArchiveError {
    /// The [`LoadHandle`] cancelled the operation before it finished.
    Cancelled,
    /// Encoding, decoding, or filesystem work failed with this message.
    Failed(String),
    /// libarchive reported an encrypted member that a password can decrypt.
    /// Separate from `Failed` so policy refusals that embed the member path
    /// (e.g. `.../password-notes/...`) are never mistaken for encryption, and
    /// so encrypted 7z/RAR (unsupported) fail instead of prompting.
    NeedsPassword(String),
}

impl std::fmt::Display for ArchiveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => f.write_str(ARCHIVE_CANCELLED),
            Self::Failed(message) | Self::NeedsPassword(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for ArchiveError {}

impl From<String> for ArchiveError {
    fn from(message: String) -> Self {
        Self::Failed(message)
    }
}

impl From<&str> for ArchiveError {
    fn from(message: &str) -> Self {
        Self::Failed(message.to_owned())
    }
}

fn archive_failed(error: impl std::fmt::Display) -> ArchiveError {
    ArchiveError::Failed(error.to_string())
}

/// Maps a libarchive decoder error to [`ArchiveError`].
///
/// Classification uses encryption flags from the entry or archive, plus
/// whether a password was already supplied. libarchive cannot decrypt 7z or
/// RAR; those messages contain "not supported"/"unavailable" and become
/// [`Failed`] so the UI does not prompt in a loop. Policy and filesystem
/// errors use [`archive_failed`] directly so a member path containing
/// `password`/`encrypt` never becomes [`NeedsPassword`].
///
/// [`Failed`]: ArchiveError::Failed
/// [`NeedsPassword`]: ArchiveError::NeedsPassword
fn classify_decoder_error(error: DecoderError, password_supplied: bool) -> ArchiveError {
    classify_encrypted_failure(error.message, error.encrypted, password_supplied)
}

fn classify_encrypted_failure(
    message: String,
    encrypted: bool,
    password_supplied: bool,
) -> ArchiveError {
    if !encrypted {
        return ArchiveError::Failed(message);
    }
    if decoder_cannot_decrypt(&message) {
        return ArchiveError::Failed(
            "This archive is encrypted in a format that cannot be opened".to_owned(),
        );
    }
    if password_supplied {
        return ArchiveError::Failed("Incorrect password".to_owned());
    }
    if passphrase_requested(&message) {
        return ArchiveError::NeedsPassword(message);
    }
    ArchiveError::Failed("This archive is encrypted in a format that cannot be opened".to_owned())
}

fn decoder_cannot_decrypt(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("not supported") || lower.contains("unavailable")
}

fn passphrase_requested(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("passphrase") || lower.contains("password")
}

/// Encrypted ZIP can be decrypted after a prompt; encrypted 7z/RAR cannot.
fn zip_can_decrypt(archive_path: &Path) -> bool {
    archive_path
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(ArchiveFormat::from_extension)
        == Some(ArchiveFormat::Zip)
}

fn encrypted_without_password(archive_path: &Path) -> ArchiveError {
    if zip_can_decrypt(archive_path) {
        ArchiveError::NeedsPassword("Passphrase required".to_owned())
    } else {
        ArchiveError::Failed(
            "This archive is encrypted in a format that cannot be opened".to_owned(),
        )
    }
}

/// Removes members already written so a password retry does not collide with them.
fn discard_extracted(
    destination: &ExtractionDestination,
    created: impl IntoIterator<Item = PathBuf>,
) {
    let mut paths: Vec<PathBuf> = created.into_iter().collect();
    paths.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    for path in paths {
        let _ = destination.remove_entry(&path);
    }
}

/// Result of an extract that may stop after writing some members.
#[derive(Debug)]
enum ArchiveOutcome<T> {
    /// Every member was processed; `T` is typically the first created name.
    Completed(T),
    /// Work stopped early. Location lists feed [`CancelledOperation`].
    Cancelled {
        completed: Vec<Location>,
        failed: Vec<Location>,
        not_attempted: Vec<Location>,
    },
}

fn check_archive_cancelled(cancelled: &AtomicBool) -> Result<(), ArchiveError> {
    if cancelled.load(Ordering::Relaxed) {
        Err(ArchiveError::Cancelled)
    } else {
        Ok(())
    }
}

fn cancelled_archive_event(
    request_id: OperationRequestId,
    destination: Location,
    completed: Vec<Location>,
    failed: Vec<Location>,
    not_attempted: Vec<Location>,
) -> OperationEvent {
    OperationEvent::Cancelled {
        request_id,
        result: CancelledOperation {
            completed,
            failed,
            not_attempted,
            affected_locations: HashSet::from([destination]),
        },
    }
}

/// Builds a [`Location`] for `relative` under the extract `destination`.
fn extract_entry_location(destination: &Path, relative: &Path) -> Location {
    Location::local(destination.join(relative))
}

/// Classifies a cancelled extract after a member was partially written.
///
/// If `removed` succeeded, the interrupted path is treated as not-attempted
/// along with `remaining`. If cleanup failed, that path is recorded as failed
/// and `remaining` stays not-attempted.
fn cancelled_extract_after_partial_write(
    dest_dir: &Path,
    created: &Path,
    completed: Vec<Location>,
    remaining: Vec<Location>,
    removed: Result<(), String>,
) -> ArchiveOutcome<Option<String>> {
    let interrupted = extract_entry_location(dest_dir, created);
    if removed.is_ok() {
        let mut not_attempted = vec![interrupted];
        not_attempted.extend(remaining);
        ArchiveOutcome::Cancelled {
            completed,
            failed: Vec::new(),
            not_attempted,
        }
    } else {
        ArchiveOutcome::Cancelled {
            completed,
            failed: vec![interrupted],
            not_attempted: remaining,
        }
    }
}

/// Copies `reader` to `writer`, checking `cancelled` between 1 MiB chunks.
///
/// Returns the number of bytes written.
///
/// # Performance
///
/// Uses a [`COPY_BUF`]-sized heap buffer so large members are not copied
/// through the default 8 KiB [`std::io::copy`] path.
///
/// # Errors
///
/// - [`Cancelled`] if `cancelled` is set between reads
/// - [`Failed`] if a read or write fails
///
/// [`Cancelled`]: ArchiveError::Cancelled
/// [`Failed`]: ArchiveError::Failed
enum CopyError {
    Cancelled,
    Failed(String),
    Io(io::Error),
}

impl From<CopyError> for ArchiveError {
    fn from(error: CopyError) -> Self {
        match error {
            CopyError::Cancelled => Self::Cancelled,
            CopyError::Failed(message) => Self::Failed(message),
            CopyError::Io(error) => archive_failed(error),
        }
    }
}

impl From<ArchiveError> for CopyError {
    fn from(error: ArchiveError) -> Self {
        match error {
            ArchiveError::Cancelled => Self::Cancelled,
            ArchiveError::Failed(message) | ArchiveError::NeedsPassword(message) => {
                Self::Failed(message)
            }
        }
    }
}

fn copy_with_big_buf(
    mut reader: impl Read,
    writer: &mut (impl Write + ?Sized),
    cancelled: &AtomicBool,
    mut on_chunk: impl FnMut(u64) -> Result<(), ArchiveError>,
) -> Result<u64, CopyError> {
    let mut buf = vec![0u8; COPY_BUF];
    let mut total = 0;
    loop {
        check_archive_cancelled(cancelled).map_err(CopyError::from)?;
        let n = reader.read(&mut buf).map_err(CopyError::Io)?;
        if n == 0 {
            break;
        }
        writer.write_all(&buf[..n]).map_err(CopyError::Io)?;
        // Tests request a cancel after this write so incomplete-file cleanup
        // does not depend on racing a watcher thread.
        #[cfg(test)]
        if CANCEL_AFTER_COPY_CHUNK.get() {
            return Err(CopyError::Cancelled);
        }
        let n = n as u64;
        total += n;
        on_chunk(n).map_err(CopyError::from)?;
    }
    Ok(total)
}

#[cfg(test)]
thread_local! {
    static CANCEL_AFTER_COPY_CHUNK: Cell<bool> = const { Cell::new(false) };
}

/// Arranges for the next [`copy_with_big_buf`] write to return [`Cancelled`].
///
/// Resets when dropped so later tests on this thread are unaffected.
///
/// [`Cancelled`]: ArchiveError::Cancelled
#[cfg(test)]
#[must_use]
fn cancel_after_next_copy_chunk() -> CancelAfterCopyChunk {
    CANCEL_AFTER_COPY_CHUNK.set(true);
    CancelAfterCopyChunk
}

#[cfg(test)]
struct CancelAfterCopyChunk;

#[cfg(test)]
impl Drop for CancelAfterCopyChunk {
    fn drop(&mut self) {
        CANCEL_AFTER_COPY_CHUNK.set(false);
    }
}

/// Starts a 100 ms timer that emits [`OperationEvent::ArchiveProgress`].
///
/// The source stays attached until the caller removes the returned
/// [`glib::SourceId`]. Returning [`glib::ControlFlow::Break`] from the
/// callback would double-remove that source.
fn archive_progress_timer(
    request_id: OperationRequestId,
    progress: &Arc<AtomicUsize>,
    total: &Arc<AtomicUsize>,
    cancelled: &Arc<AtomicBool>,
    emit: &Rc<dyn Fn(OperationEvent)>,
) -> glib::SourceId {
    let timer_progress = progress.clone();
    let timer_total = total.clone();
    let timer_cancelled = cancelled.clone();
    let timer_emit = emit.clone();
    glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
        // Keep the source until the task calls remove(); Break would double-remove.
        if !timer_cancelled.load(Ordering::Relaxed) {
            timer_emit(OperationEvent::ArchiveProgress {
                request_id,
                completed: timer_progress.load(Ordering::Relaxed),
                total: timer_total.load(Ordering::Relaxed),
            });
        }
        glib::ControlFlow::Continue
    })
}

/// Tracks renamed top-level entries so nested members follow the same rename.
///
/// If `docs` already exists in the destination, a member `docs/readme.txt`
/// is extracted under `docs (2)/readme.txt` rather than merging into `docs`.
struct ExtractNameResolver {
    renames: std::collections::HashMap<OsString, OsString>,
}

impl ExtractNameResolver {
    fn new() -> Self {
        Self {
            renames: std::collections::HashMap::new(),
        }
    }

    /// Maps a validated relative member path onto a conflict-free destination path.
    ///
    /// The top-level component is passed through [`ExtractionDestination::available_name`]
    /// once and remembered for later members that share that prefix.
    ///
    /// # Errors
    ///
    /// Returns an error if `path` has no normal first component or
    /// [`ExtractionDestination::available_name`] fails.
    fn resolve(
        &mut self,
        destination: &ExtractionDestination,
        path: &Path,
    ) -> Result<PathBuf, String> {
        let top = path
            .components()
            .next()
            .and_then(|component| match component {
                Component::Normal(name) => Some(name),
                _ => None,
            })
            .ok_or_else(|| "Archive entry has no file name".to_owned())?;
        let resolved_top = if let Some(existing) = self.renames.get(top) {
            existing.clone()
        } else {
            let name = destination.available_name(&destination.root, top)?;
            self.renames.insert(top.to_owned(), name.clone());
            name
        };
        let mut resolved = PathBuf::from(resolved_top);
        resolved.extend(
            path.components()
                .skip(1)
                .map(|component| component.as_os_str()),
        );
        Ok(resolved)
    }
}

/// Resource bounds for a single extract. Production uses
/// [`ExtractLimits::for_destination`]; tests inject tighter values.
#[derive(Clone, Debug)]
struct ExtractLimits {
    max_total_bytes: u64,
    max_members: u32,
    max_path_depth: u16,
    bomb_ratio: u32,
    ratio_floor: u64,
}

impl ExtractLimits {
    fn for_destination(destination: impl AsFd) -> Result<Self, ArchiveError> {
        let available = available_bytes(destination)?.saturating_sub(EXTRACT_DISK_RESERVE);
        Ok(Self {
            max_total_bytes: available,
            max_members: EXTRACT_MAX_MEMBERS,
            max_path_depth: EXTRACT_MAX_PATH_DEPTH,
            bomb_ratio: EXTRACT_BOMB_RATIO,
            ratio_floor: EXTRACT_BOMB_RATIO_FLOOR,
        })
    }

    #[cfg(test)]
    fn for_test(
        max_total_bytes: u64,
        max_members: u32,
        max_path_depth: u16,
        bomb_ratio: u32,
    ) -> Self {
        Self {
            max_total_bytes,
            max_members,
            max_path_depth,
            bomb_ratio,
            ratio_floor: 0,
        }
    }
}

struct ExtractBudget {
    limits: ExtractLimits,
    compressed_size: u64,
    written: u64,
    members: u32,
}

impl ExtractBudget {
    fn add_member(&mut self, path: &Path) -> Result<(), ArchiveError> {
        let depth = path.components().count();
        if depth > usize::from(self.limits.max_path_depth) {
            return Err(archive_failed(format!(
                "Refusing to extract a path nested more than {} levels: {}",
                self.limits.max_path_depth,
                path.display()
            )));
        }
        self.members = self.members.saturating_add(1);
        if self.members > self.limits.max_members {
            return Err(archive_failed(format!(
                "Refusing to extract more than {} files from one archive",
                self.limits.max_members
            )));
        }
        Ok(())
    }

    fn check_claimed_size(&self, size: u64) -> Result<(), ArchiveError> {
        if self.written.saturating_add(size) > self.limits.max_total_bytes {
            return Err(archive_failed(
                "This archive is larger than the free space available at the destination",
            ));
        }
        Ok(())
    }

    fn add_bytes(&mut self, n: u64) -> Result<(), ArchiveError> {
        self.written = self.written.saturating_add(n);
        if self.written > self.limits.max_total_bytes {
            return Err(archive_failed(
                "This archive is larger than the free space available at the destination",
            ));
        }
        if self.written > self.limits.ratio_floor {
            let max = self
                .compressed_size
                .saturating_mul(u64::from(self.limits.bomb_ratio));
            if self.written > max {
                return Err(archive_failed(
                    "Refusing to extract a compressed archive that expands beyond the safety limit",
                ));
            }
        }
        Ok(())
    }
}

fn available_bytes(destination: impl AsFd) -> Result<u64, ArchiveError> {
    let stat = rustix::fs::fstatvfs(destination).map_err(archive_failed)?;
    let block = if stat.f_frsize == 0 {
        stat.f_bsize
    } else {
        stat.f_frsize
    };
    Ok(stat.f_bavail.saturating_mul(block))
}

fn is_root_placeholder(member: &ArchiveMember) -> bool {
    member.kind == MemberKind::Directory && {
        let name = member.pathname.as_bytes();
        name == b"." || name == b"./"
    }
}

fn symlink_stays_inside(member_path: &Path, target: &OsStr) -> bool {
    if target.is_empty() {
        return false;
    }
    let mut base = member_path.parent().map(PathBuf::from).unwrap_or_default();
    for component in Path::new(target).components() {
        match component {
            Component::Prefix(_) | Component::RootDir => return false,
            Component::CurDir => {}
            Component::ParentDir => {
                if !base.pop() {
                    return false;
                }
            }
            Component::Normal(name) => {
                let bytes = name.as_bytes();
                if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
                    return false;
                }
                base.push(name);
            }
        }
    }
    true
}

fn set_file_mtime(file: &std::fs::File, mtime: i64) {
    let times = rustix::fs::Timestamps {
        last_access: rustix::fs::Timespec {
            tv_sec: 0,
            tv_nsec: rustix::fs::UTIME_OMIT,
        },
        last_modification: rustix::fs::Timespec {
            tv_sec: mtime,
            tv_nsec: 0,
        },
    };
    let _ = rustix::fs::futimens(file, &times);
}

/// Extracts every member of the archive at `archive_path` into `dest_dir`.
///
/// Member names go through [`validated_archive_os_path`]. Symlinks are created
/// only when their stored target stays inside the destination. Hard links are
/// materialized with `linkat` after every regular member is on disk. Special
/// files are refused. Written bytes, member count, path depth, and expansion
/// ratio are bounded by [`ExtractLimits`].
///
/// # Errors
///
/// - [`NeedsPassword`] if a member is encrypted, no password was supplied, and
///   libarchive can decrypt that format (ZIP). Raised before that member is
///   created; members already written are discarded so a retry does not
///   collide with leftover names
/// - [`Failed`] if the destination cannot be opened, a member name is unsafe,
///   encryption is unsupported (7z/RAR), a password is wrong, a limit is
///   exceeded, or a create/copy fails for a reason other than cancellation
///
/// Cancellation is returned as [`ArchiveOutcome::Cancelled`], not [`Cancelled`].
///
/// [`Cancelled`]: ArchiveError::Cancelled
/// [`Failed`]: ArchiveError::Failed
/// [`NeedsPassword`]: ArchiveError::NeedsPassword
fn extract_archive(
    archive_path: &Path,
    dest_dir: &Path,
    password: Option<&str>,
    progress: &Arc<AtomicUsize>,
    cancelled: &AtomicBool,
) -> Result<ArchiveOutcome<Option<String>>, ArchiveError> {
    let destination = ExtractionDestination::open(dest_dir)?;
    let limits = ExtractLimits::for_destination(&destination.root)?;
    extract_archive_with_limits(
        archive_path,
        dest_dir,
        &destination,
        password,
        progress,
        cancelled,
        limits,
    )
}

#[cfg(test)]
fn extract_with_limits(
    archive_path: &Path,
    dest_dir: &Path,
    password: Option<&str>,
    progress: &Arc<AtomicUsize>,
    cancelled: &AtomicBool,
    limits: ExtractLimits,
) -> Result<ArchiveOutcome<Option<String>>, ArchiveError> {
    let destination = ExtractionDestination::open(dest_dir)?;
    extract_archive_with_limits(
        archive_path,
        dest_dir,
        &destination,
        password,
        progress,
        cancelled,
        limits,
    )
}

fn extract_archive_with_limits(
    archive_path: &Path,
    dest_dir: &Path,
    destination: &ExtractionDestination,
    password: Option<&str>,
    progress: &Arc<AtomicUsize>,
    cancelled: &AtomicBool,
    limits: ExtractLimits,
) -> Result<ArchiveOutcome<Option<String>>, ArchiveError> {
    let compressed_size = std::fs::metadata(archive_path)
        .map_err(archive_failed)?
        .len();
    let mut budget = ExtractBudget {
        limits,
        compressed_size,
        written: 0,
        members: 0,
    };
    let password_supplied = password.is_some();
    let mut archive = ReadArchive::open(archive_path, password).map_err(archive_failed)?;
    let mut resolver = ExtractNameResolver::new();
    let mut first_name = None;
    let mut completed = Vec::new();
    let mut extracted = HashMap::<PathBuf, PathBuf>::new();
    // Hard links are deferred until every regular member is on disk: nothing
    // guarantees the link target precedes the link in archive order.
    let mut pending_hardlinks = Vec::<(PathBuf, PathBuf, PathBuf)>::new();
    loop {
        if cancelled.load(Ordering::Relaxed) {
            return Ok(ArchiveOutcome::Cancelled {
                completed,
                failed: Vec::new(),
                not_attempted: Vec::new(),
            });
        }
        let Some(member) = archive
            .next_member()
            .map_err(|error| classify_decoder_error(error, password_supplied))?
        else {
            break;
        };
        if is_root_placeholder(&member) {
            archive
                .skip_data()
                .map_err(|error| classify_decoder_error(error, password_supplied))?;
            continue;
        }
        if cancelled.load(Ordering::Relaxed) {
            let not_attempted = validated_archive_os_path(&member.pathname)
                .ok()
                .map(|path| extract_entry_location(dest_dir, &path))
                .into_iter()
                .collect();
            return Ok(ArchiveOutcome::Cancelled {
                completed,
                failed: Vec::new(),
                not_attempted,
            });
        }
        let path = validated_archive_os_path(&member.pathname)?;
        budget.add_member(&path)?;
        let is_hardlink = member.kind == MemberKind::File && member.hardlink_target.is_some();
        if !is_hardlink && let Some(size) = member.size {
            budget.check_claimed_size(size)?;
        }
        let outpath = resolver.resolve(destination, &path)?;
        if first_name.is_none() {
            first_name = outpath
                .components()
                .next()
                .map(|component| component.as_os_str().to_string_lossy().into_owned());
        }
        if member.encrypted && !password_supplied {
            discard_extracted(destination, extracted.values().cloned());
            return Err(encrypted_without_password(archive_path));
        }
        if is_hardlink {
            let target_name = member.hardlink_target.as_deref().ok_or_else(|| {
                archive_failed(format!(
                    "Archive hard link {} has no target",
                    path.display()
                ))
            })?;
            let original = validated_archive_os_path(target_name)?;
            archive
                .skip_data()
                .map_err(|error| classify_decoder_error(error, password_supplied))?;
            pending_hardlinks.push((path, outpath, original));
            continue;
        }
        let created = match member.kind {
            MemberKind::Directory => {
                destination.create_directories(&outpath)?;
                archive
                    .skip_data()
                    .map_err(|error| classify_decoder_error(error, password_supplied))?;
                outpath
            }
            MemberKind::Symlink => {
                let target = member.symlink_target.as_deref().ok_or_else(|| {
                    archive_failed(format!("Archive symlink {} has no target", path.display()))
                })?;
                if !symlink_stays_inside(&outpath, target) {
                    return Err(archive_failed(format!(
                        "Refusing symlink that escapes the destination: {}",
                        path.display()
                    )));
                }
                let created = destination.create_symlink(&outpath, target)?;
                archive
                    .skip_data()
                    .map_err(|error| classify_decoder_error(error, password_supplied))?;
                created
            }
            MemberKind::Special => {
                return Err(archive_failed(format!(
                    "Refusing to extract a special filesystem object: {}",
                    path.display()
                )));
            }
            MemberKind::File => {
                let (mut outfile, created) = destination.create_file(&outpath)?;
                if let Err(error) = copy_with_big_buf(&mut archive, &mut outfile, cancelled, |n| {
                    budget.add_bytes(n)
                }) {
                    drop(outfile);
                    let removed = destination.remove_file(&created);
                    return match error {
                        CopyError::Cancelled => Ok(cancelled_extract_after_partial_write(
                            dest_dir,
                            &created,
                            completed,
                            Vec::new(),
                            removed,
                        )),
                        CopyError::Io(error) => {
                            if let Some(decoder) = error
                                .get_ref()
                                .and_then(|inner| inner.downcast_ref::<DecoderError>())
                            {
                                Err(classify_encrypted_failure(
                                    decoder.message.clone(),
                                    member.encrypted,
                                    password_supplied,
                                ))
                            } else {
                                Err(archive_failed(error))
                            }
                        }
                        CopyError::Failed(message) => Err(ArchiveError::Failed(message)),
                    };
                }
                if let Some(mtime) = member.mtime {
                    set_file_mtime(&outfile, mtime);
                }
                created
            }
        };
        extracted.insert(path, created.clone());
        completed.push(extract_entry_location(dest_dir, &created));
        progress.fetch_add(1, Ordering::Relaxed);
    }
    // Resolve deferred hard links after all regular members are on disk.
    // Iterate to allow chains (link -> link -> file); fail closed when the
    // target never appeared in the archive.
    let mut remaining = pending_hardlinks;
    while !remaining.is_empty() {
        if cancelled.load(Ordering::Relaxed) {
            return Ok(ArchiveOutcome::Cancelled {
                completed,
                failed: Vec::new(),
                not_attempted: Vec::new(),
            });
        }
        let mut progressed = false;
        let mut deferred = Vec::with_capacity(remaining.len());
        for (path, outpath, original) in remaining {
            let Some(source_rel) = extracted.get(&original).cloned() else {
                deferred.push((path, outpath, original));
                continue;
            };
            let created = destination.create_hard_link(&outpath, &source_rel)?;
            extracted.insert(path, created.clone());
            completed.push(extract_entry_location(dest_dir, &created));
            progress.fetch_add(1, Ordering::Relaxed);
            progressed = true;
        }
        if !progressed {
            let original = &deferred
                .first()
                .expect("should retain unresolved hard links when none progressed")
                .2;
            return Err(archive_failed(format!(
                "Hard link target was not extracted: {}",
                original.display()
            )));
        }
        remaining = deferred;
    }
    Ok(ArchiveOutcome::Completed(first_name))
}
