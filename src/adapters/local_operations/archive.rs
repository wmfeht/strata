// SPDX-License-Identifier: GPL-3.0-or-later

//! Local archive compression and extraction for [`LocalOperationProvider`].
//!
//! Builds and unpacks archives through [`exarch_core`]. Compression writes
//! through a `0o600` staging file and publishes the result only after the
//! encoder finishes, so a partial archive is never left at the destination
//! name. Extraction applies [`exarch_core`]'s path, symlink, hard-link, and
//! zip-bomb checks, with quotas taken from free space at the destination.
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

use std::{
    collections::HashSet,
    io,
    os::unix::fs::PermissionsExt as _,
    path::{Component, Path, PathBuf},
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

use exarch_core::{
    ArchiveError as ExarchError, CreationConfig, ExtractionOptions, ProgressCallback,
    QuotaResource, SecurityConfig, create_archive_with_progress,
    extract_archive_with_options_and_progress, formats::detect::ArchiveType,
};

use gtk::{gio, glib};

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
    F: FnOnce(&Path) -> Result<(), ArchiveError> + Send + 'static,
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
    let staged_path = staged.path().to_path_buf();
    gio::spawn_blocking(move || write_archive(&staged_path))
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
        let entries: Vec<PathBuf> = sources
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
            move |path| {
                compress_entries(
                    path,
                    &entries,
                    format,
                    &work_progress,
                    &work_total,
                    &work_cancelled,
                )
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
            Err(ArchiveError::Failed(error)) => {
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
/// detected by `exarch_core` from the file bytes and extension. Returns a
/// [`LoadHandle`] that cancels in-flight work when dropped.
///
/// Emits [`ArchiveStarted`] immediately, [`ArchiveProgress`] while running,
/// then [`Archived`], [`Failed`], or [`Cancelled`]. Encrypted archives fail
/// instead of prompting: `exarch_core` rejects password-protected ZIP and 7z.
///
/// # Concurrency
///
/// Runs on the default [`glib::MainContext`]. Decoding happens on a worker
/// thread via [`gio::spawn_blocking`].
///
/// [`ArchiveStarted`]: OperationEvent::ArchiveStarted
/// [`ArchiveProgress`]: OperationEvent::ArchiveProgress
/// [`Archived`]: OperationEvent::Archived
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
        let total = Arc::new(AtomicUsize::new(0));
        emit(OperationEvent::ArchiveStarted { request_id });
        let timer_id =
            archive_progress_timer(request_id, &progress, &total, &task_cancelled, &emit);
        let work_progress = progress.clone();
        let work_total = total.clone();
        let result = gio::spawn_blocking(move || {
            extract_archive(
                &archive_path,
                &dest_dir,
                password.as_deref(),
                &work_progress,
                &work_total,
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

/// Writes an archive of `entries` through `exarch_core`.
///
/// Selected names are mirrored into a private layout directory so a folder
/// `Photos` is stored as `Photos/...` rather than dumping its children at the
/// archive root. ZIP, TAR, and TAR.GZ are created; 7z creation is refused.
/// Hidden files are included and default exclude patterns are cleared so a
/// user-selected `.git` is archived.
///
/// # Errors
///
/// - [`Cancelled`] if `cancelled` is set before encoding starts
/// - [`Failed`] if a member cannot be opened, 7z is requested, or encoding fails
///
/// [`Cancelled`]: ArchiveError::Cancelled
/// [`Failed`]: ArchiveError::Failed
fn compress_entries(
    output: &Path,
    entries: &[PathBuf],
    format: ArchiveFormat,
    progress: &Arc<AtomicUsize>,
    total: &Arc<AtomicUsize>,
    cancelled: &AtomicBool,
) -> Result<(), ArchiveError> {
    let archive_type = creatable_archive_type(format)?;
    check_archive_cancelled(cancelled)?;
    let layout = tempfile::Builder::new()
        .prefix(".strata-archive-layout-")
        .tempdir()
        .map_err(archive_failed)?;
    for entry in entries {
        check_archive_cancelled(cancelled)?;
        let name = entry.file_name().ok_or("Entry has no file name")?;
        mirror_source(entry, &layout.path().join(name))?;
    }
    let config = creation_config(archive_type);
    let mut tracker = ArchiveProgress::new(progress, total);
    create_archive_with_progress(output, &[layout.path()], &config, &mut tracker)
        .map_err(map_exarch_error)?;
    check_archive_cancelled(cancelled)?;
    Ok(())
}

fn creatable_archive_type(format: ArchiveFormat) -> Result<ArchiveType, ArchiveError> {
    match format {
        ArchiveFormat::Zip => Ok(ArchiveType::Zip),
        ArchiveFormat::Tar => Ok(ArchiveType::Tar),
        ArchiveFormat::TarGz => Ok(ArchiveType::TarGz),
        ArchiveFormat::SevenZ => Err(archive_failed(
            "7z compression is not supported. Use ZIP or TAR instead.",
        )),
    }
}

fn creation_config(format: ArchiveType) -> CreationConfig {
    CreationConfig::default()
        .with_include_hidden(true)
        .with_exclude_patterns(Vec::new())
        .with_follow_symlinks(false)
        .with_preserve_permissions(true)
        .with_format(Some(format))
}

/// Copies `source` into `destination` without following a final symbolic link.
///
/// Regular files are hard-linked when possible so large selections are not
/// duplicated before encoding. Directories are recreated and walked with
/// `symlink_metadata`. Symbolic links are stored as links.
///
/// # Errors
///
/// Returns an error if `source` cannot be inspected or `destination` cannot
/// be created.
fn mirror_source(source: &Path, destination: &Path) -> Result<(), ArchiveError> {
    let metadata = std::fs::symlink_metadata(source).map_err(archive_failed)?;
    if metadata.file_type().is_symlink() {
        let target = std::fs::read_link(source).map_err(archive_failed)?;
        std::os::unix::fs::symlink(target, destination).map_err(archive_failed)?;
        return Ok(());
    }
    if metadata.is_dir() {
        std::fs::create_dir(destination).map_err(archive_failed)?;
        for child in std::fs::read_dir(source).map_err(archive_failed)? {
            let child = child.map_err(archive_failed)?;
            mirror_source(&child.path(), &destination.join(child.file_name()))?;
        }
        return Ok(());
    }
    if metadata.is_file() {
        match std::fs::hard_link(source, destination) {
            Ok(()) => Ok(()),
            Err(error) if error.raw_os_error() == Some(cross_device_errno()) => {
                std::fs::copy(source, destination).map_err(archive_failed)?;
                Ok(())
            }
            Err(error) => Err(archive_failed(error)),
        }
    } else {
        Err(archive_failed(
            "Compression supports only regular files, folders, and symbolic links",
        ))
    }
}

fn cross_device_errno() -> i32 {
    rustix::io::Errno::XDEV.raw_os_error()
}

const EXTRACT_DISK_RESERVE: u64 = 64 * 1024 * 1024;
const EXTRACT_MAX_MEMBERS: u32 = 1_000_000;
const EXTRACT_MAX_PATH_DEPTH: u16 = 256;
const EXTRACT_BOMB_RATIO: u32 = 200;

/// Resource bounds for a single extract. Production uses
/// [`ExtractLimits::for_destination`]; tests inject tighter values.
#[derive(Clone, Debug)]
struct ExtractLimits {
    max_total_bytes: u64,
    max_members: u32,
    max_path_depth: u16,
    bomb_ratio: u32,
}

impl ExtractLimits {
    fn for_destination(destination: &Path) -> Result<Self, ArchiveError> {
        let available = available_bytes(destination)?.saturating_sub(EXTRACT_DISK_RESERVE);
        Ok(Self {
            max_total_bytes: available.max(1),
            max_members: EXTRACT_MAX_MEMBERS,
            max_path_depth: EXTRACT_MAX_PATH_DEPTH,
            bomb_ratio: EXTRACT_BOMB_RATIO,
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
            max_total_bytes: max_total_bytes.max(1),
            max_members: max_members.max(1),
            max_path_depth: max_path_depth.max(1),
            bomb_ratio: bomb_ratio.max(1),
        }
    }
}

fn available_bytes(destination: &Path) -> Result<u64, ArchiveError> {
    let stat = rustix::fs::statvfs(destination).map_err(archive_failed)?;
    let block = if stat.f_frsize == 0 {
        stat.f_bsize
    } else {
        stat.f_frsize
    };
    Ok(stat.f_bavail.saturating_mul(block))
}

fn security_config(limits: &ExtractLimits) -> SecurityConfig {
    SecurityConfig::default()
        .with_max_file_size(limits.max_total_bytes)
        .with_max_total_size(limits.max_total_bytes)
        .with_max_file_count(usize::try_from(limits.max_members).unwrap_or(usize::MAX))
        .with_max_path_depth(usize::from(limits.max_path_depth))
        .with_max_compression_ratio(f64::from(limits.bomb_ratio))
        .with_allow_symlinks(true)
        .with_allow_hardlinks(true)
        .with_allow_world_writable(true)
        .with_preserve_permissions(true)
        .with_banned_path_components(Vec::new())
        .with_allow_solid_archives(true)
}

/// Failure or cooperative cancellation of a compress or extract step.
#[derive(Debug, PartialEq, Eq)]
enum ArchiveError {
    /// The [`LoadHandle`] cancelled the operation before it finished.
    Cancelled,
    /// Encoding, decoding, or filesystem work failed with this message.
    Failed(String),
}

impl std::fmt::Display for ArchiveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => f.write_str("Operation cancelled"),
            Self::Failed(message) => f.write_str(message),
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

fn map_exarch_error(error: ExarchError) -> ArchiveError {
    match error {
        ExarchError::PartialExtraction { source, .. } => map_exarch_error(*source),
        ExarchError::PathTraversal { path } => {
            archive_failed(format!("Refusing unsafe archive path: {}", path.display()))
        }
        ExarchError::SymlinkEscape { path } => archive_failed(format!(
            "Refusing symlink that escapes the destination: {}",
            path.display()
        )),
        ExarchError::HardlinkEscape { path } => archive_failed(format!(
            "Refusing hard link that escapes the destination: {}",
            path.display()
        )),
        ExarchError::ZipBomb { .. } => archive_failed(
            "Refusing to extract a compressed archive that expands beyond the safety limit",
        ),
        ExarchError::QuotaExceeded { resource } => match resource {
            QuotaResource::FileCount { max, .. } => archive_failed(format!(
                "Refusing to extract more than {max} files from one archive"
            )),
            QuotaResource::TotalSize { .. } | QuotaResource::FileSize { .. } => archive_failed(
                "This archive is larger than the free space available at the destination",
            ),
            QuotaResource::IntegerOverflow => archive_failed("Archive quota overflow"),
            _ => archive_failed("Archive quota exceeded"),
        },
        ExarchError::SecurityViolation { reason } => {
            let lower = reason.to_ascii_lowercase();
            if lower.contains("password") || lower.contains("encrypt") {
                archive_failed("This archive is encrypted in a format that cannot be opened")
            } else if lower.contains("depth") || lower.contains("nested") {
                archive_failed(format!(
                    "Refusing to extract a path nested more than the allowed depth: {reason}"
                ))
            } else {
                archive_failed(reason)
            }
        }
        ExarchError::UnknownFormat { path } => {
            archive_failed(format!("Unrecognized archive format: {}", path.display()))
        }
        other => archive_failed(other),
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

struct ArchiveProgress {
    progress: Arc<AtomicUsize>,
    total: Arc<AtomicUsize>,
    first_name: Option<String>,
}

impl ArchiveProgress {
    fn new(progress: &Arc<AtomicUsize>, total: &Arc<AtomicUsize>) -> Self {
        Self {
            progress: Arc::clone(progress),
            total: Arc::clone(total),
            first_name: None,
        }
    }
}

impl ProgressCallback for ArchiveProgress {
    fn on_entry_start(&mut self, path: &Path, total: usize, current: usize) {
        self.total.store(total, Ordering::Relaxed);
        self.progress
            .store(current.saturating_sub(1), Ordering::Relaxed);
        if self.first_name.is_none() {
            self.first_name = path.components().next().and_then(|component| {
                matches!(component, Component::Normal(_))
                    .then(|| component.as_os_str().to_string_lossy().into_owned())
            });
        }
    }

    fn on_bytes_written(&mut self, _bytes: u64) {}

    fn on_entry_complete(&mut self, _path: &Path) {
        self.progress.fetch_add(1, Ordering::Relaxed);
    }

    fn on_complete(&mut self) {}
}

/// Extracts every member of the archive at `archive_path` into `dest_dir`.
///
/// Quotas come from [`ExtractLimits`]. Symlinks and hard links that stay
/// inside the destination are restored. Encrypted archives are refused.
/// `password` is accepted for API compatibility and ignored: `exarch_core`
/// cannot decrypt ZIP or 7z.
///
/// # Errors
///
/// - [`Failed`] if the destination cannot be opened, a member name is unsafe,
///   encryption is present, a limit is exceeded, or extraction fails
///
/// Cancellation before work starts is returned as [`ArchiveOutcome::Cancelled`].
///
/// [`Failed`]: ArchiveError::Failed
fn extract_archive(
    archive_path: &Path,
    dest_dir: &Path,
    password: Option<&str>,
    progress: &Arc<AtomicUsize>,
    total: &Arc<AtomicUsize>,
    cancelled: &AtomicBool,
) -> Result<ArchiveOutcome<Option<String>>, ArchiveError> {
    let limits = ExtractLimits::for_destination(dest_dir)?;
    extract_archive_with_limits(
        archive_path,
        dest_dir,
        password,
        progress,
        total,
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
    extract_archive_with_limits(
        archive_path,
        dest_dir,
        password,
        progress,
        &Arc::new(AtomicUsize::new(0)),
        cancelled,
        limits,
    )
}

fn extract_archive_with_limits(
    archive_path: &Path,
    dest_dir: &Path,
    _password: Option<&str>,
    progress: &Arc<AtomicUsize>,
    total: &Arc<AtomicUsize>,
    cancelled: &AtomicBool,
    limits: ExtractLimits,
) -> Result<ArchiveOutcome<Option<String>>, ArchiveError> {
    if cancelled.load(Ordering::Relaxed) {
        return Ok(ArchiveOutcome::Cancelled {
            completed: Vec::new(),
            failed: Vec::new(),
            not_attempted: Vec::new(),
        });
    }
    let config = security_config(&limits);
    let options = ExtractionOptions::default().with_skip_duplicates(true);
    let mut tracker = ArchiveProgress::new(progress, total);
    extract_archive_with_options_and_progress(
        archive_path,
        dest_dir,
        &config,
        &options,
        &mut tracker,
    )
    .map_err(map_exarch_error)?;
    if cancelled.load(Ordering::Relaxed) {
        return Ok(ArchiveOutcome::Cancelled {
            completed: Vec::new(),
            failed: Vec::new(),
            not_attempted: Vec::new(),
        });
    }
    Ok(ArchiveOutcome::Completed(tracker.first_name))
}
