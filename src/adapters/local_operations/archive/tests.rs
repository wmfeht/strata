// SPDX-License-Identifier: GPL-3.0-or-later

use std::{
    cell::RefCell,
    collections::BTreeMap,
    error::Error,
    ffi::{OsStr, OsString},
    fs,
    io::Write,
    os::unix::{
        ffi::{OsStrExt, OsStringExt},
        fs::PermissionsExt,
    },
    path::{Path, PathBuf},
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

use gtk::glib;

use crate::test_support::ASYNC_MAIN_CONTEXT_DEFAULT;

use super::{
    ArchiveError, ArchiveOutcome, ExtractLimits, compress_entries, copy_with_big_buf,
    count_archive_files, extract_archive, extract_with_limits, libarchive::WriteArchive,
    process_umask, validated_archive_path, write_staged_archive,
};
use crate::{
    adapters::local_operations::LocalOperationProvider,
    model::{EntryKind, FileEntry, Location, MetadataValue},
    services::{
        ArchiveFormat, CompressRequest, ExtractRequest, OperationEvent, OperationProvider,
        OperationRequestId, TransferConflict,
    },
};

fn test_file_entry(path: &Path) -> FileEntry {
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

fn compression_stages(destination: &Path) -> Result<Vec<OsString>, Box<dyn Error>> {
    Ok(fs::read_dir(destination)?
        .filter_map(Result::ok)
        .map(|entry| entry.file_name())
        .filter(|name| name.to_string_lossy().starts_with(".strata-compression-"))
        .collect())
}

fn compression_stage_mode(destination: &Path) -> Result<u32, Box<dyn Error>> {
    let mut stages = compression_stages(destination)?;
    let name = stages.pop().ok_or("no compression staging file")?;
    if !stages.is_empty() {
        return Err("expected a single compression staging file".into());
    }
    Ok(fs::metadata(destination.join(name))?.permissions().mode() & 0o777)
}

#[test]
fn compression_provider_rejects_escaping_archive_names() -> Result<(), Box<dyn Error>> {
    let _serial = ASYNC_MAIN_CONTEXT_DEFAULT
        .lock()
        .map_err(|error| error.to_string())?;
    let root = tempfile::tempdir()?;
    let destination = root.path().join("destination");
    let source = root.path().join("source.txt");
    fs::create_dir(&destination)?;
    fs::write(&source, b"source")?;

    let events = run_compression(CompressRequest {
        id: OperationRequestId(1),
        entries: vec![test_file_entry(&source)],
        destination: Location::local(&destination),
        archive_name: "../outside".to_owned(),
        conflict: TransferConflict::ReplaceExisting,
        format: ArchiveFormat::Zip,
        password: None,
    });

    assert!(matches!(events.as_slice(), [OperationEvent::Failed { .. }]));
    assert!(!root.path().join("outside.zip").exists());
    assert!(compression_stages(&destination)?.is_empty());
    Ok(())
}

#[test]
fn compression_conflict_choices_preserve_or_replace_the_destination() -> Result<(), Box<dyn Error>>
{
    let _serial = ASYNC_MAIN_CONTEXT_DEFAULT
        .lock()
        .map_err(|error| error.to_string())?;
    let root = tempfile::tempdir()?;
    let destination = root.path().join("destination");
    let source = root.path().join("source.txt");
    let archive = destination.join("existing.zip");
    fs::create_dir(&destination)?;
    fs::write(&source, b"replacement")?;
    fs::write(&archive, b"original")?;
    fs::set_permissions(&archive, fs::Permissions::from_mode(0o640))?;
    let request = |conflict| CompressRequest {
        id: OperationRequestId(1),
        entries: vec![test_file_entry(&source)],
        destination: Location::local(&destination),
        archive_name: "existing".to_owned(),
        conflict,
        format: ArchiveFormat::Zip,
        password: None,
    };

    let refused = run_compression(request(TransferConflict::FailIfExists));
    assert!(
        refused
            .iter()
            .any(|event| matches!(event, OperationEvent::Failed { .. }))
    );
    assert_eq!(fs::read(&archive)?, b"original");
    assert_eq!(fs::metadata(&archive)?.permissions().mode() & 0o777, 0o640);

    let replaced = run_compression(request(TransferConflict::ReplaceExisting));
    assert!(
        replaced
            .iter()
            .any(|event| matches!(event, OperationEvent::Compressed { .. }))
    );
    let extracted = destination.join("extracted");
    fs::create_dir(&extracted)?;
    assert_eq!(
        extract_here(&archive, &extracted)?,
        Some("source.txt".to_owned())
    );
    assert_eq!(fs::metadata(&archive)?.permissions().mode() & 0o777, 0o640);
    assert!(compression_stages(&destination)?.is_empty());
    Ok(())
}

#[test]
fn compression_staging_stays_private_while_encoding() -> Result<(), Box<dyn Error>> {
    let _serial = ASYNC_MAIN_CONTEXT_DEFAULT
        .lock()
        .map_err(|error| error.to_string())?;
    let root = tempfile::tempdir()?;
    let destination = root.path().to_path_buf();
    let archive = destination.join("existing.zip");
    fs::write(&archive, b"original")?;
    fs::set_permissions(&archive, fs::Permissions::from_mode(0o640))?;
    let started = Arc::new(AtomicBool::new(false));
    let release = Arc::new(AtomicBool::new(false));
    let worker_started = started.clone();
    let worker_release = release.clone();
    let worker_destination = destination.clone();
    let worker_archive = archive.clone();
    let task = glib::MainContext::default().spawn_local(async move {
        write_staged_archive(
            &worker_destination,
            &worker_archive,
            TransferConflict::ReplaceExisting,
            &never_cancelled(),
            move |mut file| {
                file.write_all(b"replacement")
                    .map_err(|error| error.to_string())?;
                worker_started.store(true, Ordering::Release);
                while !worker_release.load(Ordering::Acquire) {
                    std::thread::yield_now();
                }
                Ok(())
            },
        )
        .await
    });
    let context = glib::MainContext::default();
    while !started.load(Ordering::Acquire) {
        context.iteration(false);
        std::thread::yield_now();
    }
    assert_eq!(compression_stage_mode(&destination)?, 0o600);

    release.store(true, Ordering::Release);
    assert_eq!(context.block_on(task)?, Ok(()));
    assert_eq!(fs::read(&archive)?, b"replacement");
    assert_eq!(fs::metadata(&archive)?.permissions().mode() & 0o777, 0o640);
    assert!(compression_stages(&destination)?.is_empty());
    Ok(())
}

#[test]
fn compression_new_archive_staging_stays_private_until_publish() -> Result<(), Box<dyn Error>> {
    let _serial = ASYNC_MAIN_CONTEXT_DEFAULT
        .lock()
        .map_err(|error| error.to_string())?;
    let root = tempfile::tempdir()?;
    let destination = root.path().to_path_buf();
    let archive = destination.join("created.zip");
    let started = Arc::new(AtomicBool::new(false));
    let release = Arc::new(AtomicBool::new(false));
    let worker_started = started.clone();
    let worker_release = release.clone();
    let worker_destination = destination.clone();
    let worker_archive = archive.clone();
    let task = glib::MainContext::default().spawn_local(async move {
        write_staged_archive(
            &worker_destination,
            &worker_archive,
            TransferConflict::FailIfExists,
            &never_cancelled(),
            move |mut file| {
                file.write_all(b"created")
                    .map_err(|error| error.to_string())?;
                worker_started.store(true, Ordering::Release);
                while !worker_release.load(Ordering::Acquire) {
                    std::thread::yield_now();
                }
                Ok(())
            },
        )
        .await
    });
    let context = glib::MainContext::default();
    while !started.load(Ordering::Acquire) {
        context.iteration(false);
        std::thread::yield_now();
    }
    assert_eq!(compression_stage_mode(&destination)?, 0o600);

    release.store(true, Ordering::Release);
    assert_eq!(context.block_on(task)?, Ok(()));
    assert_eq!(fs::read(&archive)?, b"created");
    assert_eq!(
        fs::metadata(&archive)?.permissions().mode() & 0o777,
        0o666 & !process_umask()
    );
    assert!(compression_stages(&destination)?.is_empty());
    Ok(())
}

#[test]
fn compression_failure_preserves_an_existing_archive() -> Result<(), Box<dyn Error>> {
    let _serial = ASYNC_MAIN_CONTEXT_DEFAULT
        .lock()
        .map_err(|error| error.to_string())?;
    let root = tempfile::tempdir()?;
    let destination = root.path().join("destination");
    let missing = root.path().join("missing.txt");
    let archive = destination.join("existing.zip");
    fs::create_dir(&destination)?;
    fs::write(&archive, b"original")?;

    let events = run_compression(CompressRequest {
        id: OperationRequestId(1),
        entries: vec![test_file_entry(&missing)],
        destination: Location::local(&destination),
        archive_name: "existing".to_owned(),
        conflict: TransferConflict::ReplaceExisting,
        format: ArchiveFormat::Zip,
        password: None,
    });

    assert!(
        events
            .iter()
            .any(|event| matches!(event, OperationEvent::Failed { .. }))
    );
    assert_eq!(fs::read(&archive)?, b"original");
    assert!(compression_stages(&destination)?.is_empty());
    Ok(())
}

#[test]
fn every_compression_format_commits_a_readable_archive() -> Result<(), Box<dyn Error>> {
    let _serial = ASYNC_MAIN_CONTEXT_DEFAULT
        .lock()
        .map_err(|error| error.to_string())?;
    let root = tempfile::tempdir()?;
    let destination = root.path().join("destination");
    let source = root.path().join("source.txt");
    let mode_reference = root.path().join("mode-reference");
    fs::create_dir(&destination)?;
    fs::write(&source, b"contents")?;
    fs::File::create(&mode_reference)?;
    let expected_mode = fs::metadata(&mode_reference)?.permissions().mode() & 0o777;

    for format in [
        ArchiveFormat::Zip,
        ArchiveFormat::SevenZ,
        ArchiveFormat::TarGz,
        ArchiveFormat::Tar,
    ] {
        let base = format!("archive-{}", format.extension().replace('.', "-"));
        let events = run_compression(CompressRequest {
            id: OperationRequestId(1),
            entries: vec![test_file_entry(&source)],
            destination: Location::local(&destination),
            archive_name: base.clone(),
            conflict: TransferConflict::FailIfExists,
            format,
            password: None,
        });
        assert!(
            events
                .iter()
                .any(|event| matches!(event, OperationEvent::Compressed { .. }))
        );
        let archive = destination.join(format!("{base}.{}", format.extension()));
        let extracted = destination.join(format!("extracted-{base}"));
        fs::create_dir(&extracted)?;
        extract_archive(
            &archive,
            &extracted,
            None,
            &Arc::new(AtomicUsize::new(0)),
            &never_cancelled(),
        )?;
        assert_eq!(fs::read(extracted.join("source.txt"))?, b"contents");
        assert_eq!(
            fs::metadata(&archive)?.permissions().mode() & 0o777,
            expected_mode
        );
    }
    assert!(compression_stages(&destination)?.is_empty());
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
enum CompressedEntry {
    Directory,
    File(Vec<u8>),
    Symlink(PathBuf),
}

fn read_compressed_entries(
    path: &Path,
    _format: ArchiveFormat,
    password: Option<&str>,
) -> Result<BTreeMap<PathBuf, CompressedEntry>, Box<dyn Error>> {
    use std::io::Read as _;

    use super::libarchive::{MemberKind, ReadArchive};

    let mut archive = ReadArchive::open(path, password)?;
    let mut result = BTreeMap::new();
    while let Some(member) = archive.next_member()? {
        let mut name = PathBuf::from(&member.pathname);
        if let Some(stripped) = name.as_os_str().as_bytes().strip_suffix(b"/") {
            name = PathBuf::from(OsStr::from_bytes(stripped));
        }
        let value = match member.kind {
            MemberKind::Directory => {
                archive.skip_data()?;
                CompressedEntry::Directory
            }
            MemberKind::Symlink => {
                archive.skip_data()?;
                CompressedEntry::Symlink(PathBuf::from(member.symlink_target.unwrap_or_default()))
            }
            MemberKind::File => {
                let mut bytes = Vec::new();
                archive.read_to_end(&mut bytes)?;
                CompressedEntry::File(bytes)
            }
            MemberKind::Special => {
                archive.skip_data()?;
                continue;
            }
        };
        assert!(
            result.insert(name, value).is_none(),
            "duplicate archive member"
        );
    }
    Ok(result)
}

fn write_compression_fixture(
    path: &Path,
    entries: &[PathBuf],
    format: ArchiveFormat,
    password: Option<&str>,
) -> Result<usize, String> {
    let file = fs::File::create(path).map_err(|error| error.to_string())?;
    let progress = Arc::new(AtomicUsize::new(0));
    let cancelled = never_cancelled();
    compress_entries(file, entries, format, password, &progress, &cancelled)
        .map_err(|error| error.to_string())?;
    Ok(progress.load(Ordering::Relaxed))
}

#[test]
fn compression_preserves_links_in_zip_and_tar() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let source = root.path().join("source");
    fs::create_dir_all(source.join("nested"))?;
    fs::create_dir(source.join("empty"))?;
    fs::write(source.join("file.txt"), b"file contents")?;
    fs::write(source.join("nested/child.txt"), b"child contents")?;
    let mut expected = BTreeMap::from([
        (PathBuf::from("source"), CompressedEntry::Directory),
        (PathBuf::from("source/nested"), CompressedEntry::Directory),
        (PathBuf::from("source/empty"), CompressedEntry::Directory),
        (
            PathBuf::from("source/file.txt"),
            CompressedEntry::File(b"file contents".to_vec()),
        ),
        (
            PathBuf::from("source/nested/child.txt"),
            CompressedEntry::File(b"child contents".to_vec()),
        ),
    ]);
    for (name, target) in [
        ("file-link", "file.txt"),
        ("directory-link", "nested"),
        ("broken-link", "missing.txt"),
        ("current-directory-link", "."),
    ] {
        std::os::unix::fs::symlink(target, source.join(name))?;
        expected.insert(
            PathBuf::from("source").join(name),
            CompressedEntry::Symlink(PathBuf::from(target)),
        );
    }
    let selected_link = root.path().join("selected-link");
    std::os::unix::fs::symlink("source/nested", &selected_link)?;
    expected.insert(
        PathBuf::from("selected-link"),
        CompressedEntry::Symlink(PathBuf::from("source/nested")),
    );
    let entries = [source, selected_link];
    assert_eq!(count_archive_files(&entries, &never_cancelled())?, 7);
    for (format, password) in [
        (ArchiveFormat::Zip, None),
        (ArchiveFormat::Zip, Some("test-password")),
        (ArchiveFormat::Tar, None),
        (ArchiveFormat::TarGz, None),
    ] {
        let archive = root.path().join("archive");
        assert_eq!(
            write_compression_fixture(&archive, &entries, format, password)?,
            7
        );
        assert_eq!(
            read_compressed_entries(&archive, format, password)?,
            expected,
            "{format:?}"
        );
    }
    Ok(())
}

#[test]
fn seven_z_compression_preserves_files_and_empty_directories() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let source = root.path().join("source");
    fs::create_dir_all(source.join("empty"))?;
    fs::write(source.join("one.txt"), b"one")?;
    fs::write(source.join("two.png"), b"two")?;
    let expected = BTreeMap::from([
        (PathBuf::from("source"), CompressedEntry::Directory),
        (PathBuf::from("source/empty"), CompressedEntry::Directory),
        (
            PathBuf::from("source/one.txt"),
            CompressedEntry::File(b"one".to_vec()),
        ),
        (
            PathBuf::from("source/two.png"),
            CompressedEntry::File(b"two".to_vec()),
        ),
    ]);
    for password in [None, Some("test-password")] {
        let archive = root.path().join("archive");
        assert_eq!(
            write_compression_fixture(
                &archive,
                std::slice::from_ref(&source),
                ArchiveFormat::SevenZ,
                password
            )?,
            2
        );
        assert_eq!(
            read_compressed_entries(&archive, ArchiveFormat::SevenZ, password)?,
            expected,
        );
    }
    Ok(())
}

#[test]
fn compression_reports_unsupported_7z_links_without_committing() -> Result<(), Box<dyn Error>> {
    let _serial = ASYNC_MAIN_CONTEXT_DEFAULT
        .lock()
        .map_err(|error| error.to_string())?;
    let root = tempfile::tempdir()?;
    let source = root.path().join("source");
    fs::create_dir(&source)?;
    fs::write(source.join("file.txt"), b"contents")?;
    let link = source.join("link");
    std::os::unix::fs::symlink("file.txt", &link)?;
    for entry in [&source, &link] {
        for conflict in [
            TransferConflict::FailIfExists,
            TransferConflict::ReplaceExisting,
        ] {
            let destination = tempfile::tempdir()?;
            let archive = destination.path().join("archive.7z");
            if conflict == TransferConflict::ReplaceExisting {
                fs::write(&archive, b"original archive")?;
            }
            let events = run_compression(CompressRequest {
                id: OperationRequestId(1),
                entries: vec![test_file_entry(entry)],
                destination: Location::local(destination.path()),
                archive_name: "archive".to_owned(),
                conflict,
                format: ArchiveFormat::SevenZ,
                password: None,
            });
            assert!(events.iter().any(|event| matches!(event, OperationEvent::Failed { message, .. } if message.contains("does not support symbolic links") && message.contains("Use ZIP or TAR instead"))));
            assert!(
                !events
                    .iter()
                    .any(|event| matches!(event, OperationEvent::Compressed { .. }))
            );
            if conflict == TransferConflict::ReplaceExisting {
                assert_eq!(fs::read(&archive)?, b"original archive");
            } else {
                assert!(!archive.exists());
            }
            assert!(compression_stages(destination.path())?.is_empty());
        }
    }
    Ok(())
}

#[test]
fn compression_handles_non_utf8_link_targets_without_loss() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let link = root.path().join("link");
    let target = PathBuf::from(OsString::from_vec(b"target-\xff".to_vec()));
    std::os::unix::fs::symlink(&target, &link)?;
    for format in [ArchiveFormat::Tar, ArchiveFormat::TarGz, ArchiveFormat::Zip] {
        let error = write_compression_fixture(
            &root.path().join("archive"),
            std::slice::from_ref(&link),
            format,
            None,
        )
        .expect_err("non-UTF-8 link targets cannot be encoded");
        assert!(
            error.contains("non-UTF-8 link target"),
            "{format:?}: {error}"
        );
    }
    Ok(())
}

#[test]
fn cancelling_staged_compression_unlinks_the_partial_output() -> Result<(), Box<dyn Error>> {
    let _serial = ASYNC_MAIN_CONTEXT_DEFAULT
        .lock()
        .map_err(|error| error.to_string())?;
    let root = tempfile::tempdir()?;
    let destination = root.path().to_path_buf();
    let archive = destination.join("existing.zip");
    fs::write(&archive, b"original")?;
    let started = Arc::new(AtomicBool::new(false));
    let release = Arc::new(AtomicBool::new(false));
    let finished = Arc::new(AtomicBool::new(false));
    let worker_started = started.clone();
    let worker_release = release.clone();
    let worker_finished = finished.clone();
    let worker_destination = destination.clone();
    let worker_archive = archive.clone();
    let task = glib::MainContext::default().spawn_local(async move {
        write_staged_archive(
            &worker_destination,
            &worker_archive,
            TransferConflict::ReplaceExisting,
            &never_cancelled(),
            move |mut file| {
                file.write_all(b"partial")
                    .map_err(|error| error.to_string())?;
                worker_started.store(true, Ordering::Release);
                while !worker_release.load(Ordering::Acquire) {
                    std::thread::yield_now();
                }
                worker_finished.store(true, Ordering::Release);
                Ok(())
            },
        )
        .await
    });
    let context = glib::MainContext::default();
    while !started.load(Ordering::Acquire) {
        context.iteration(false);
        std::thread::yield_now();
    }
    assert_eq!(compression_stages(&destination)?.len(), 1);

    task.abort();
    drop(task);
    while context.pending() {
        context.iteration(false);
    }
    let stage_was_removed = compression_stages(&destination)?.is_empty();
    let destination_was_preserved = fs::read(&archive)? == b"original";
    release.store(true, Ordering::Release);
    while !finished.load(Ordering::Acquire) {
        std::thread::yield_now();
    }

    assert!(stage_was_removed);
    assert!(destination_was_preserved);
    Ok(())
}

#[test]
fn write_staged_archive_does_not_publish_when_cancelled_after_write() -> Result<(), Box<dyn Error>>
{
    let _serial = ASYNC_MAIN_CONTEXT_DEFAULT
        .lock()
        .map_err(|error| error.to_string())?;
    let root = tempfile::tempdir()?;
    let destination = root.path().to_path_buf();
    let archive = destination.join("existing.zip");
    fs::write(&archive, b"original")?;
    let cancelled = Arc::new(AtomicBool::new(false));
    let persist_cancelled = cancelled.clone();
    let worker_cancelled = cancelled.clone();
    let worker_destination = destination.clone();
    let worker_archive = archive.clone();
    let task = glib::MainContext::default().spawn_local(async move {
        write_staged_archive(
            &worker_destination,
            &worker_archive,
            TransferConflict::ReplaceExisting,
            &persist_cancelled,
            move |mut file| {
                file.write_all(b"replacement")
                    .map_err(|error| error.to_string())?;
                worker_cancelled.store(true, Ordering::Release);
                Ok(())
            },
        )
        .await
    });
    let result = glib::MainContext::default().block_on(task)?;
    assert!(matches!(result, Err(ArchiveError::Cancelled)));
    assert_eq!(fs::read(&archive)?, b"original");
    assert!(compression_stages(&destination)?.is_empty());
    Ok(())
}

fn write_named_archive(
    path: &Path,
    format: ArchiveFormat,
    stored: bool,
    entries: &[(&str, Option<&[u8]>)],
) -> Result<(), Box<dyn Error>> {
    let mut writer =
        WriteArchive::create_with_zip_store(fs::File::create(path)?, format, None, stored)?;
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

fn write_zip(path: &Path, entries: &[(&str, &[u8])]) -> Result<(), Box<dyn Error>> {
    let mapped: Vec<(&str, Option<&[u8]>)> = entries
        .iter()
        .map(|(name, contents)| (*name, Some(*contents)))
        .collect();
    write_named_archive(path, ArchiveFormat::Zip, false, &mapped)
}

fn write_zip_stored(path: &Path, entries: &[(&str, &[u8])]) -> Result<(), Box<dyn Error>> {
    let mapped: Vec<(&str, Option<&[u8]>)> = entries
        .iter()
        .map(|(name, contents)| (*name, Some(*contents)))
        .collect();
    write_named_archive(path, ArchiveFormat::Zip, true, &mapped)
}

fn write_tar(path: &Path, name: &str, contents: &[u8], gzip: bool) -> Result<(), Box<dyn Error>> {
    let format = if gzip {
        ArchiveFormat::TarGz
    } else {
        ArchiveFormat::Tar
    };
    write_named_archive(path, format, false, &[(name, Some(contents))])
}

fn write_7z(path: &Path, name: &str, contents: &[u8]) -> Result<(), Box<dyn Error>> {
    write_named_archive(
        path,
        ArchiveFormat::SevenZ,
        false,
        &[(name, Some(contents))],
    )
}

fn write_7z_entries(path: &Path, entries: &[(&str, &[u8])]) -> Result<(), Box<dyn Error>> {
    let mapped: Vec<(&str, Option<&[u8]>)> = entries
        .iter()
        .map(|(name, contents)| (*name, Some(*contents)))
        .collect();
    write_named_archive(path, ArchiveFormat::SevenZ, false, &mapped)
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

#[test]
fn seven_z_extraction_preserves_all_file_contents() -> Result<(), Box<dyn Error>> {
    let entries = [
        ("folder/one.txt", b"first contents".as_slice()),
        ("folder/two.txt", b"second contents".as_slice()),
        ("folder/nested/three.txt", b"third contents".as_slice()),
    ];
    let root = tempfile::tempdir()?;
    let archive_path = root.path().join("files.7z");
    let destination = root.path().join("extracted");
    fs::create_dir(&destination)?;
    write_7z_entries(&archive_path, &entries)?;
    let progress = Arc::new(AtomicUsize::new(0));

    assert_eq!(
        completed_extract(extract_archive(
            &archive_path,
            &destination,
            None,
            &progress,
            &never_cancelled(),
        )?)?,
        Some("folder".to_owned())
    );
    for (name, contents) in &entries {
        assert_eq!(fs::read(destination.join(name))?, *contents);
    }
    assert_eq!(progress.load(Ordering::Relaxed), entries.len());
    Ok(())
}

#[test]
fn seven_z_extraction_preserves_all_empty_files_and_directories() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let archive_path = root.path().join("empty-entries.7z");
    let destination = root.path().join("extracted");
    fs::create_dir(&destination)?;
    write_named_archive(
        &archive_path,
        ArchiveFormat::SevenZ,
        false,
        &[
            ("folder", None),
            ("folder/one.txt", Some(b"".as_slice())),
            ("folder/empty", None),
            ("folder/two.txt", Some(b"".as_slice())),
        ],
    )?;
    let progress = Arc::new(AtomicUsize::new(0));

    assert_eq!(
        completed_extract(extract_archive(
            &archive_path,
            &destination,
            None,
            &progress,
            &never_cancelled(),
        )?)?,
        Some("folder".to_owned())
    );
    assert!(destination.join("folder/empty").is_dir());
    for name in ["folder/one.txt", "folder/two.txt"] {
        assert!(fs::read(destination.join(name))?.is_empty());
    }
    assert_eq!(progress.load(Ordering::Relaxed), 4);
    Ok(())
}

fn extract_here(path: &Path, destination: &Path) -> Result<Option<String>, String> {
    completed_extract(
        extract_archive(
            path,
            destination,
            None,
            &Arc::new(AtomicUsize::new(0)),
            &never_cancelled(),
        )
        .map_err(|error| error.to_string())?,
    )
}

#[test]
fn tar_extraction_skips_root_directories_and_preserves_contents() -> Result<(), Box<dyn Error>> {
    for format in [ArchiveFormat::Tar, ArchiveFormat::TarGz] {
        for root_entry in [None, Some("."), Some("./")] {
            let root = tempfile::tempdir()?;
            let destination = root.path().join("destination");
            fs::create_dir_all(destination.join("folder"))?;
            fs::write(destination.join("folder/keep.txt"), b"keep")?;
            let archive = root.path().join("content.tar");
            let mut entries = Vec::new();
            if let Some(name) = root_entry {
                entries.push((name, None));
            }
            entries.extend([
                ("./folder/", None),
                ("./folder/item.txt", Some(b"contents".as_slice())),
                ("./empty.txt", Some(b"".as_slice())),
            ]);
            write_named_archive(&archive, format, false, &entries)?;
            let progress = Arc::new(AtomicUsize::new(0));
            assert_eq!(
                completed_extract(extract_archive(
                    &archive,
                    &destination,
                    None,
                    &progress,
                    &never_cancelled(),
                )?)?,
                Some("folder (2)".to_owned()),
            );
            assert_eq!(progress.load(Ordering::Relaxed), 3);
            assert_eq!(
                fs::read(destination.join("folder (2)/item.txt"))?,
                b"contents"
            );
            assert_eq!(fs::read(destination.join("folder/keep.txt"))?, b"keep");
            assert_eq!(fs::metadata(destination.join("empty.txt"))?.len(), 0);
            assert_eq!(fs::read_dir(&destination)?.count(), 3);
        }
    }
    Ok(())
}

#[test]
fn tar_extraction_root_only_completes_without_a_name_and_respects_cancellation()
-> Result<(), Box<dyn Error>> {
    for format in [ArchiveFormat::Tar, ArchiveFormat::TarGz] {
        let root = tempfile::tempdir()?;
        let destination = root.path().join("destination");
        fs::create_dir(&destination)?;
        let archive = root.path().join("content.tar");
        write_named_archive(&archive, format, false, &[("./", None)])?;
        let progress = Arc::new(AtomicUsize::new(0));
        assert_eq!(
            completed_extract(extract_archive(
                &archive,
                &destination,
                None,
                &progress,
                &never_cancelled(),
            )?)?,
            None,
        );
        assert!(matches!(
            extract_archive(&archive, &destination, None, &progress, &always_cancelled())?,
            ArchiveOutcome::Cancelled { completed, failed, not_attempted }
                if completed.is_empty() && failed.is_empty() && not_attempted.is_empty()
        ));
        assert_eq!(progress.load(Ordering::Relaxed), 0);
        assert!(fs::read_dir(&destination)?.next().is_none());
    }
    Ok(())
}

#[test]
fn tar_extraction_rejects_empty_paths_and_root_file_entries() -> Result<(), Box<dyn Error>> {
    for (directory, name) in [
        (true, ""),
        (true, "/"),
        (false, ""),
        (false, "."),
        (false, "././"),
    ] {
        let root = tempfile::tempdir()?;
        let destination = root.path().join("destination");
        fs::create_dir(&destination)?;
        let archive = root.path().join("content.tar");
        write_raw_tar(&archive, name, directory)?;
        let progress = Arc::new(AtomicUsize::new(0));
        assert!(
            matches!(
                extract_archive(&archive, &destination, None, &progress, &never_cancelled()),
                Err(ArchiveError::Failed(_))
            ),
            "accepted directory={directory} {name:?}"
        );
        assert_eq!(progress.load(Ordering::Relaxed), 0);
        assert!(fs::read_dir(&destination)?.next().is_none());
    }
    Ok(())
}

fn write_raw_tar(path: &Path, name: &str, directory: bool) -> Result<(), Box<dyn Error>> {
    let mut builder = tar::Builder::new(fs::File::create(path)?);
    let mut header = tar::Header::new_gnu();
    let bytes = name.as_bytes();
    if !bytes.is_empty() {
        header.as_old_mut().name[..bytes.len()].copy_from_slice(bytes);
    }
    header.set_mode(0o644);
    header.set_size(0);
    header.set_entry_type(if directory {
        tar::EntryType::Directory
    } else {
        tar::EntryType::Regular
    });
    header.set_cksum();
    builder.append(&header, &[] as &[u8])?;
    builder.finish()?;
    Ok(())
}

#[test]
fn archive_paths_must_be_nonempty_confined_relative_paths() -> Result<(), Box<dyn Error>> {
    for path in [
        "",
        ".",
        "./",
        "././",
        "../marker",
        "safe/../marker",
        "/tmp/marker",
        "\\tmp\\marker",
        "C:\\tmp\\marker",
        "C:marker",
        "safe/C:/marker",
        "\\\\server\\share\\marker",
        "//server/share/marker",
    ] {
        assert!(validated_archive_path(path).is_err(), "accepted {path:?}");
    }
    assert_eq!(
        validated_archive_path("folder/./nested//item.txt")?,
        Path::new("folder/nested/item.txt")
    );
    Ok(())
}

#[test]
fn every_archive_format_rejects_parent_traversal() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let destination = root.path().join("destination");
    fs::create_dir(&destination)?;
    let zip_path = root.path().join("malicious.zip");
    let tar_path = root.path().join("malicious.tar");
    let tar_gz_path = root.path().join("malicious.tar.gz");
    let seven_z_path = root.path().join("malicious.7z");
    write_zip(&zip_path, &[("../zip-marker", b"escaped")])?;
    write_tar(&tar_path, "../tar-marker", b"escaped", false)?;
    write_tar(&tar_gz_path, "../tar-gz-marker", b"escaped", true)?;
    write_7z(&seven_z_path, "../seven-z-marker", b"escaped")?;

    assert!(extract_here(&zip_path, &destination).is_err());
    assert!(extract_here(&tar_path, &destination).is_err());
    assert!(extract_here(&tar_gz_path, &destination).is_err());
    assert!(extract_here(&seven_z_path, &destination).is_err());

    for marker in [
        "zip-marker",
        "tar-marker",
        "tar-gz-marker",
        "seven-z-marker",
    ] {
        assert!(!root.path().join(marker).exists(), "created {marker}");
    }
    Ok(())
}

#[test]
fn compression_accepts_a_symlink_in_the_parent_path() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let actual = root.path().join("actual");
    let alias = root.path().join("alias");
    fs::create_dir_all(actual.join("tree"))?;
    fs::write(actual.join("tree/file.txt"), b"contents")?;
    std::os::unix::fs::symlink("actual", &alias)?;
    let entries = [alias.join("tree")];
    let expected = BTreeMap::from([
        (PathBuf::from("tree"), CompressedEntry::Directory),
        (
            PathBuf::from("tree/file.txt"),
            CompressedEntry::File(b"contents".to_vec()),
        ),
    ]);

    for format in [
        ArchiveFormat::Zip,
        ArchiveFormat::Tar,
        ArchiveFormat::TarGz,
        ArchiveFormat::SevenZ,
    ] {
        let archive = root.path().join("archive");
        assert_eq!(count_archive_files(&entries, &never_cancelled())?, 1);
        assert_eq!(
            write_compression_fixture(&archive, &entries, format, None)?,
            1
        );
        assert_eq!(
            read_compressed_entries(&archive, format, None)?,
            expected,
            "{format:?}"
        );
    }
    Ok(())
}

#[test]
fn extraction_rejects_final_and_intermediate_symlinks() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let destination = root.path().join("destination");
    let external = root.path().join("external");
    fs::create_dir(&destination)?;
    fs::create_dir(&external)?;
    std::os::unix::fs::symlink(root.path().join("missing"), destination.join("dangling"))?;
    std::os::unix::fs::symlink(&external, destination.join("redirect"))?;
    let final_archive = root.path().join("final.zip");
    let intermediate_archive = root.path().join("intermediate.zip");
    write_zip(&final_archive, &[("dangling", b"escaped")])?;
    write_zip(&intermediate_archive, &[("redirect/marker", b"escaped")])?;

    assert!(extract_here(&final_archive, &destination).is_err());
    assert!(extract_here(&intermediate_archive, &destination).is_err());
    assert!(!root.path().join("missing").exists());
    assert!(!external.join("marker").exists());
    Ok(())
}

#[test]
fn extraction_supports_nesting_and_regular_conflicts() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let destination = root.path().join("destination");
    fs::create_dir(&destination)?;
    fs::write(destination.join("report.txt"), b"original")?;
    fs::create_dir(destination.join("existing"))?;
    fs::write(destination.join("existing/old.txt"), b"old")?;
    let archive_path = root.path().join("content.zip");
    write_zip(
        &archive_path,
        &[
            ("folder/nested/item.txt", b"nested"),
            ("report.txt", b"replacement"),
            ("existing/new.txt", b"new"),
        ],
    )?;

    assert_eq!(
        extract_here(&archive_path, &destination)?.as_deref(),
        Some("folder")
    );
    assert_eq!(
        fs::read(destination.join("folder/nested/item.txt"))?,
        b"nested"
    );
    assert_eq!(fs::read(destination.join("report.txt"))?, b"original");
    assert_eq!(
        fs::read(destination.join("report (2).txt"))?,
        b"replacement"
    );
    assert_eq!(fs::read(destination.join("existing/old.txt"))?, b"old");
    assert_eq!(fs::read(destination.join("existing (2)/new.txt"))?, b"new");
    Ok(())
}

#[test]
fn copy_with_big_buf_stops_when_cancelled() {
    let cancelled = AtomicBool::new(true);
    let mut destination = Vec::new();
    let error = copy_with_big_buf(&b"payload"[..], &mut destination, &cancelled, |_| Ok(()))
        .expect_err("cancelled copy must stop");
    assert!(matches!(error, ArchiveError::Cancelled));
    assert!(destination.is_empty());
}

#[test]
fn zip_extraction_stops_and_drops_incomplete_output_when_cancelled() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let destination = root.path().join("destination");
    fs::create_dir(&destination)?;
    let archive_path = root.path().join("content.zip");
    write_zip(
        &archive_path,
        &[("first.bin", b"early"), ("second.txt", b"late")],
    )?;

    let outcome = extract_archive(
        &archive_path,
        &destination,
        None,
        &Arc::new(AtomicUsize::new(0)),
        &always_cancelled(),
    )?;

    match outcome {
        ArchiveOutcome::Cancelled {
            completed,
            failed,
            not_attempted,
        } => {
            assert!(completed.is_empty());
            assert!(failed.is_empty());
            assert!(not_attempted.len() <= 1);
        }
        ArchiveOutcome::Completed(_) => panic!("extraction continued after cancellation"),
    }
    assert!(destination.read_dir()?.next().is_none());
    Ok(())
}

#[test]
fn tar_extraction_stops_without_scanning_remaining_entries() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let destination = root.path().join("destination");
    fs::create_dir(&destination)?;
    let archive_path = root.path().join("content.tar");
    write_named_archive(
        &archive_path,
        ArchiveFormat::Tar,
        false,
        &[
            ("first.bin", Some(b"early".as_slice())),
            ("second.txt", Some(b"late".as_slice())),
        ],
    )?;

    let outcome = extract_archive(
        &archive_path,
        &destination,
        None,
        &Arc::new(AtomicUsize::new(0)),
        &always_cancelled(),
    )?;

    match outcome {
        ArchiveOutcome::Cancelled {
            completed,
            failed,
            not_attempted,
        } => {
            assert!(completed.is_empty());
            assert!(failed.is_empty());
            assert!(not_attempted.len() <= 1);
        }
        ArchiveOutcome::Completed(_) => panic!("extraction continued after cancellation"),
    }
    assert!(destination.read_dir()?.next().is_none());
    Ok(())
}

#[test]
fn sevenz_extraction_reports_remaining_entries_when_cancelled() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let destination = root.path().join("destination");
    fs::create_dir(&destination)?;
    let archive_path = root.path().join("content.7z");
    write_7z_entries(
        &archive_path,
        &[("first.bin", b"early"), ("second.txt", b"late")],
    )?;

    let outcome = extract_archive(
        &archive_path,
        &destination,
        None,
        &Arc::new(AtomicUsize::new(0)),
        &always_cancelled(),
    )?;

    match outcome {
        ArchiveOutcome::Cancelled {
            completed,
            failed,
            not_attempted,
        } => {
            assert!(completed.is_empty());
            assert!(failed.is_empty());
            assert!(not_attempted.len() <= 1);
        }
        ArchiveOutcome::Completed(_) => panic!("extraction continued after cancellation"),
    }
    assert!(destination.read_dir()?.next().is_none());
    Ok(())
}

#[test]
fn cancelling_extraction_waits_for_the_worker_and_reports_incomplete_output()
-> Result<(), Box<dyn Error>> {
    let _serial = ASYNC_MAIN_CONTEXT_DEFAULT
        .lock()
        .map_err(|error| error.to_string())?;
    let root = tempfile::tempdir()?;
    let destination = root.path().join("destination");
    fs::create_dir(&destination)?;
    let archive_path = root.path().join("content.zip");
    let first = vec![0x3c_u8; 2 * 1024 * 1024];
    write_zip_stored(
        &archive_path,
        &[("first.bin", first.as_slice()), ("second.txt", b"late")],
    )?;

    let events = Rc::new(RefCell::new(Vec::new()));
    let emitted = events.clone();
    let handle = LocalOperationProvider.extract(
        ExtractRequest {
            id: OperationRequestId(11),
            entry: test_file_entry(&archive_path),
            destination: Location::local(&destination),
            password: None,
        },
        Rc::new(move |event| emitted.borrow_mut().push(event)),
    );
    while !events
        .borrow()
        .iter()
        .any(|event| matches!(event, OperationEvent::ArchiveStarted { .. }))
    {
        glib::MainContext::default().iteration(true);
    }
    drop(handle);
    while !events.borrow().iter().any(|event| {
        matches!(
            event,
            OperationEvent::Cancelled { .. }
                | OperationEvent::Extracted { .. }
                | OperationEvent::Failed { .. }
        )
    }) {
        glib::MainContext::default().iteration(true);
    }

    assert!(
        events
            .borrow()
            .iter()
            .any(|event| matches!(event, OperationEvent::Cancelled { .. })),
        "expected cancellation after the worker stopped: {:?}",
        events.borrow()
    );
    assert!(
        !events
            .borrow()
            .iter()
            .any(|event| matches!(event, OperationEvent::Extracted { .. }))
    );
    let result = events
        .borrow()
        .iter()
        .find_map(|event| match event {
            OperationEvent::Cancelled { result, .. } => Some(result.clone()),
            _ => None,
        })
        .expect("terminal cancellation result");
    assert!(
        result
            .affected_locations
            .contains(&Location::local(&destination))
    );
    assert!(!destination.join("second.txt").exists());
    Ok(())
}

#[test]
fn extraction_refuses_a_small_high_ratio_archive() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let destination = root.path().join("destination");
    fs::create_dir(&destination)?;
    let archive_path = root.path().join("bomb.zip");
    let zeros = vec![0_u8; 64 * 1024];
    write_named_archive(
        &archive_path,
        ArchiveFormat::Zip,
        false,
        &[("zeros.bin", Some(zeros.as_slice()))],
    )?;
    let limits = ExtractLimits::for_test(10 * 1024 * 1024, 100, 16, 1024 * 1024, 2);
    let error = extract_with_limits(
        &archive_path,
        &destination,
        None,
        &Arc::new(AtomicUsize::new(0)),
        &never_cancelled(),
        limits,
    )
    .expect_err("zip bomb should be refused");
    assert!(
        matches!(error, ArchiveError::Failed(ref message) if message.contains("expands beyond the safety limit")),
        "{error:?}"
    );
    assert!(!destination.join("zeros.bin").exists());
    Ok(())
}

#[test]
fn extraction_refuses_excessive_member_counts() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let destination = root.path().join("destination");
    fs::create_dir(&destination)?;
    let archive_path = root.path().join("many.zip");
    write_zip(
        &archive_path,
        &[("one.txt", b"a"), ("two.txt", b"b"), ("three.txt", b"c")],
    )?;
    let limits = ExtractLimits::for_test(10 * 1024 * 1024, 2, 16, 1024 * 1024, 200);
    let error = extract_with_limits(
        &archive_path,
        &destination,
        None,
        &Arc::new(AtomicUsize::new(0)),
        &never_cancelled(),
        limits,
    )
    .expect_err("member-count bomb should be refused");
    assert!(
        matches!(error, ArchiveError::Failed(ref message) if message.contains("more than 2 files")),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn extraction_refuses_excessive_path_depth() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let destination = root.path().join("destination");
    fs::create_dir(&destination)?;
    let archive_path = root.path().join("deep.zip");
    write_zip(&archive_path, &[("a/b/c/d.txt", b"nested")])?;
    let limits = ExtractLimits::for_test(10 * 1024 * 1024, 100, 2, 1024 * 1024, 200);
    let error = extract_with_limits(
        &archive_path,
        &destination,
        None,
        &Arc::new(AtomicUsize::new(0)),
        &never_cancelled(),
        limits,
    )
    .expect_err("deep path should be refused");
    assert!(
        matches!(error, ArchiveError::Failed(ref message) if message.contains("nested more than")),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn extraction_restores_confined_symlinks_and_refuses_escaping_targets() -> Result<(), Box<dyn Error>>
{
    let root = tempfile::tempdir()?;
    let destination = root.path().join("destination");
    fs::create_dir(&destination)?;
    let safe = root.path().join("safe.tar");
    let mut writer = WriteArchive::create(fs::File::create(&safe)?, ArchiveFormat::Tar, None)?;
    writer.write_file_header(Path::new("file.txt"), 4, None)?;
    writer.write_all(b"data")?;
    writer.finish_entry()?;
    writer.write_symlink(Path::new("link"), OsStr::new("file.txt"))?;
    writer.finish()?;
    extract_here(&safe, &destination)?;
    assert_eq!(fs::read(destination.join("file.txt"))?, b"data");
    assert_eq!(
        fs::read_link(destination.join("link"))?,
        Path::new("file.txt")
    );

    let destination = root.path().join("escape");
    fs::create_dir(&destination)?;
    let unsafe_archive = root.path().join("unsafe.tar");
    let mut writer =
        WriteArchive::create(fs::File::create(&unsafe_archive)?, ArchiveFormat::Tar, None)?;
    writer.write_symlink(Path::new("escape"), OsStr::new("../outside"))?;
    writer.finish()?;
    assert!(extract_here(&unsafe_archive, &destination).is_err());
    assert!(!destination.join("escape").exists());
    assert!(!root.path().join("outside").exists());
    Ok(())
}
