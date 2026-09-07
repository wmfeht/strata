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
    extract_with_limits,
    libarchive::{TarFilter, WriteArchive},
    process_umask, validated_archive_path, write_staged_archive,
};
use crate::{
    adapters::local_operations::LocalOperationProvider,
    model::Location,
    services::{
        ArchiveAction, ArchiveFormat, ArchiveRequest, OperationEvent, OperationProvider,
        OperationRequestId, TransferConflict,
    },
    test_support::ASYNC_MAIN_CONTEXT_DEFAULT,
};

fn lock_main_context() -> Result<std::sync::MutexGuard<'static, ()>, Box<dyn Error>> {
    ASYNC_MAIN_CONTEXT_DEFAULT
        .lock()
        .map_err(|error| error.to_string().into())
}

fn run_archive(request: ArchiveRequest) -> Vec<OperationEvent> {
    let events = Rc::new(RefCell::new(Vec::new()));
    let emitted = events.clone();
    let operation = LocalOperationProvider.archive(
        request,
        Rc::new(move |event| emitted.borrow_mut().push(event)),
    );
    while !events.borrow().iter().any(|event| {
        matches!(
            event,
            OperationEvent::Archived { .. }
                | OperationEvent::Failed { .. }
                | OperationEvent::PasswordRequired { .. }
        )
    }) {
        glib::MainContext::default().iteration(true);
    }
    drop(operation);
    events.borrow().clone()
}

fn run_compression(request: ArchiveRequest) -> Vec<OperationEvent> {
    run_archive(request)
}

fn compress_request(
    source: &Path,
    destination: &Path,
    name: &str,
    format: ArchiveFormat,
    conflict: TransferConflict,
) -> ArchiveRequest {
    ArchiveRequest {
        id: OperationRequestId(1),
        destination: Location::local(destination),
        password: None,
        action: ArchiveAction::Compress {
            sources: vec![Location::local(source)],
            archive_name: name.to_owned(),
            format,
            conflict,
        },
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
            None => writer.write_directory(member, 0o755)?,
            Some(bytes) => {
                writer.write_file_header(member, bytes.len() as u64, 0o644, None)?;
                writer.write_all(bytes)?;
                writer.finish_entry()?;
            }
        }
    }
    writer.finish()?;
    Ok(())
}

fn write_tar_filter(
    path: &Path,
    filter: TarFilter,
    entries: &[(&str, Option<&[u8]>)],
) -> Result<(), Box<dyn Error>> {
    let mut writer = WriteArchive::create_tar_filter(fs::File::create(path)?, filter)?;
    for (name, contents) in entries {
        let member = Path::new(name);
        match contents {
            None => writer.write_directory(member, 0o755)?,
            Some(bytes) => {
                writer.write_file_header(member, bytes.len() as u64, 0o644, None)?;
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

/// Expansion ratio still applies when the archive is larger than the old 64 MiB window.
#[test]
fn bomb_ratio_large_compressed_size() {
    let mut budget = super::ExtractBudget {
        limits: ExtractLimits::for_test(u64::MAX, 100, 16, 200),
        compressed_size: 65 * 1024 * 1024,
        written: 0,
        members: 0,
    };
    budget
        .add_bytes(65 * 1024 * 1024 * 200)
        .expect("an expansion of exactly 200× should be allowed");
    let error = budget
        .add_bytes(1)
        .expect_err("a 65 MiB archive should still be refused past 200×");
    assert!(
        matches!(
            error,
            ArchiveError::Failed(ref message)
                if message.contains("expands beyond the safety limit")
        ),
        "{error:?}"
    );
}
