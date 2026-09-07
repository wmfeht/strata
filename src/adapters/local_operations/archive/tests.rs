// SPDX-License-Identifier: GPL-3.0-or-later

mod compress;
mod extract;

use std::{
    cell::RefCell,
    error::Error,
    ffi::OsString,
    fs,
    io::Write as _,
    path::{Path, PathBuf},
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize},
    },
};

use gtk::glib;

use super::{
    ArchiveError, ArchiveOutcome, ExtractLimits, compress_entries, extract_archive,
    extract_with_limits, libarchive::WriteArchive, process_umask, validated_archive_path,
    write_staged_archive,
};
use crate::{
    adapters::local_operations::LocalOperationProvider,
    model::{EntryKind, FileEntry, Location, MetadataValue},
    services::{
        ArchiveFormat, CompressRequest, OperationEvent, OperationProvider, OperationRequestId,
        TransferConflict,
    },
    test_support::ASYNC_MAIN_CONTEXT_DEFAULT,
};

fn file_entry(path: &Path) -> FileEntry {
    let name = path.file_name().unwrap_or_default().to_os_string();
    FileEntry {
        location: Location::local(path),
        thumbnail_path: None,
        native_name: name.clone(),
        display_name: name.to_string_lossy().into_owned(),
        kind: EntryKind::File,
        size: MetadataValue::Unknown,
        modified_unix_seconds: MetadataValue::Unknown,
        is_hidden: false,
        mode: MetadataValue::Unknown,
    }
}

fn lock_main_context() -> Result<std::sync::MutexGuard<'static, ()>, Box<dyn Error>> {
    ASYNC_MAIN_CONTEXT_DEFAULT
        .lock()
        .map_err(|error| error.to_string().into())
}

fn run_compression(request: CompressRequest) -> Vec<OperationEvent> {
    let events = Rc::new(RefCell::new(Vec::new()));
    let emitted = events.clone();
    let operation = LocalOperationProvider.compress(
        request,
        Rc::new(move |event| emitted.borrow_mut().push(event)),
    );
    while !events.borrow().iter().any(|event| {
        matches!(
            event,
            OperationEvent::Compressed { .. } | OperationEvent::Failed { .. }
        )
    }) {
        glib::MainContext::default().iteration(true);
    }
    drop(operation);
    events.borrow().clone()
}

fn compress_request(
    source: &Path,
    destination: &Path,
    name: &str,
    format: ArchiveFormat,
    conflict: TransferConflict,
) -> CompressRequest {
    CompressRequest {
        id: OperationRequestId(1),
        entries: vec![file_entry(source)],
        destination: Location::local(destination),
        archive_name: name.to_owned(),
        conflict,
        format,
        password: None,
    }
}

fn compression_stages(destination: &Path) -> Result<Vec<OsString>, Box<dyn Error>> {
    Ok(fs::read_dir(destination)?
        .filter_map(Result::ok)
        .map(|entry| entry.file_name())
        .filter(|name| name.to_string_lossy().starts_with(".strata-compression-"))
        .collect())
}

fn write_fixture(
    path: &Path,
    entries: &[PathBuf],
    format: ArchiveFormat,
    password: Option<&str>,
) -> Result<(), String> {
    let file = fs::File::create(path).map_err(|error| error.to_string())?;
    compress_entries(
        file,
        entries,
        format,
        password,
        &Arc::new(AtomicUsize::new(0)),
        &never_cancelled(),
    )
    .map_err(|error| error.to_string())
}

fn write_archive(
    path: &Path,
    format: ArchiveFormat,
    entries: &[(&str, Option<&[u8]>)],
) -> Result<(), Box<dyn Error>> {
    let mut writer = WriteArchive::create(fs::File::create(path)?, format, None)?;
    for (name, contents) in entries {
        let member = Path::new(name);
        match contents {
            None => writer.write_directory(member)?,
            Some(bytes) => {
                writer.write_file_header(member, bytes.len() as u64, None)?;
                writer.write_all(bytes)?;
                writer.finish_entry()?;
            }
        }
    }
    writer.finish()?;
    Ok(())
}

fn never_cancelled() -> Arc<AtomicBool> {
    Arc::new(AtomicBool::new(false))
}

fn always_cancelled() -> Arc<AtomicBool> {
    Arc::new(AtomicBool::new(true))
}

fn completed_extract<T>(outcome: ArchiveOutcome<T>) -> Result<T, String> {
    match outcome {
        ArchiveOutcome::Completed(value) => Ok(value),
        ArchiveOutcome::Cancelled { .. } => Err("unexpected cancellation".to_owned()),
    }
}

fn extract_here(path: &Path, destination: &Path) -> Result<Option<String>, String> {
    extract_with_password(path, destination, None)
}

fn extract_with_password(
    path: &Path,
    destination: &Path,
    password: Option<&str>,
) -> Result<Option<String>, String> {
    completed_extract(
        extract_archive(
            path,
            destination,
            password,
            &Arc::new(AtomicUsize::new(0)),
            &never_cancelled(),
        )
        .map_err(|error| error.to_string())?,
    )
}

fn extract_limited(
    archive: &Path,
    destination: &Path,
    limits: ExtractLimits,
) -> Result<ArchiveOutcome<Option<String>>, ArchiveError> {
    extract_with_limits(
        archive,
        destination,
        None,
        &Arc::new(AtomicUsize::new(0)),
        &never_cancelled(),
        limits,
    )
}
