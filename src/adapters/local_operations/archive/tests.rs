// SPDX-License-Identifier: GPL-3.0-or-later

mod compress;
mod extract;

use std::{
    cell::RefCell,
    error::Error,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize},
    },
};

use gtk::glib;

use super::{
    ArchiveError, ArchiveOutcome, ExtractLimits, compress_entries, creation_config,
    extract_archive, extract_with_limits, process_umask, write_staged_archive,
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
use exarch_core::{create_archive, formats::detect::ArchiveType};

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

fn write_fixture(path: &Path, entries: &[PathBuf], format: ArchiveFormat) -> Result<(), String> {
    compress_entries(
        path,
        entries,
        format,
        &Arc::new(AtomicUsize::new(0)),
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
    let archive_type = match format {
        ArchiveFormat::Zip => ArchiveType::Zip,
        ArchiveFormat::Tar => ArchiveType::Tar,
        ArchiveFormat::TarGz => ArchiveType::TarGz,
        ArchiveFormat::SevenZ => {
            return Err("7z fixtures cannot be created with exarch_core".into());
        }
    };
    write_typed_archive(path, archive_type, entries)
}

fn write_typed_archive(
    path: &Path,
    format: ArchiveType,
    entries: &[(&str, Option<&[u8]>)],
) -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    for (name, contents) in entries {
        let dest = root.path().join(name);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        match contents {
            None => {
                fs::create_dir_all(&dest)?;
            }
            Some(bytes) => {
                fs::write(&dest, bytes)?;
            }
        }
    }
    create_archive(path, &[root.path()], &creation_config(format))?;
    Ok(())
}

fn write_tar_members(
    path: &Path,
    build: impl FnOnce(&mut tar::Builder<fs::File>) -> Result<(), Box<dyn Error>>,
) -> Result<(), Box<dyn Error>> {
    let file = fs::File::create(path)?;
    let mut builder = tar::Builder::new(file);
    build(&mut builder)?;
    builder.finish()?;
    Ok(())
}

fn append_tar_file(
    builder: &mut tar::Builder<fs::File>,
    name: &str,
    bytes: &[u8],
) -> Result<(), Box<dyn Error>> {
    let mut header = tar::Header::new_gnu();
    header.set_size(bytes.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    builder.append_data(&mut header, name, bytes)?;
    Ok(())
}

/// Writes a tar member whose name the `tar` crate would otherwise reject (`..`).
fn append_tar_named(
    builder: &mut tar::Builder<fs::File>,
    name: &str,
    bytes: &[u8],
) -> Result<(), Box<dyn Error>> {
    let mut header = tar::Header::new_gnu();
    header.set_size(bytes.len() as u64);
    header.set_mode(0o644);
    let name_bytes = name.as_bytes();
    if name_bytes.len() >= header.as_old().name.len() {
        return Err("tar member name is too long for the ustar name field".into());
    }
    header.as_old_mut().name[..name_bytes.len()].copy_from_slice(name_bytes);
    header.set_cksum();
    builder.append(&header, bytes)?;
    Ok(())
}

fn append_tar_symlink(
    builder: &mut tar::Builder<fs::File>,
    name: &str,
    target: &str,
) -> Result<(), Box<dyn Error>> {
    let mut header = tar::Header::new_gnu();
    header.set_entry_type(tar::EntryType::Symlink);
    header.set_size(0);
    header.set_mode(0o777);
    header.set_cksum();
    builder.append_link(&mut header, name, target)?;
    Ok(())
}

fn append_tar_hardlink(
    builder: &mut tar::Builder<fs::File>,
    name: &str,
    target: &str,
) -> Result<(), Box<dyn Error>> {
    let mut header = tar::Header::new_gnu();
    header.set_entry_type(tar::EntryType::Link);
    header.set_size(0);
    header.set_mode(0o644);
    header.set_cksum();
    builder.append_link(&mut header, name, target)?;
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
            &Arc::new(AtomicUsize::new(0)),
            &never_cancelled(),
        )
        .map_err(|error| error.to_string())?,
    )
}

fn decode_hex(hex: &str) -> Result<Vec<u8>, Box<dyn Error>> {
    let hex: String = hex.chars().filter(|ch| !ch.is_whitespace()).collect();
    if !hex.len().is_multiple_of(2) {
        return Err("hex fixture must have an even length".into());
    }
    (0..hex.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&hex[index..index + 2], 16).map_err(Into::into))
        .collect()
}

fn extract_request(archive: &Path, destination: &Path, password: Option<String>) -> ArchiveRequest {
    ArchiveRequest {
        id: OperationRequestId(1),
        destination: Location::local(destination),
        action: ArchiveAction::Extract {
            archive: Location::local(archive),
            password,
        },
    }
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
