// SPDX-License-Identifier: GPL-3.0-or-later

use std::{
    error::Error,
    fs,
    io::Write as _,
    os::unix::fs::PermissionsExt as _,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use gtk::glib;

use super::{
    ArchiveAction, ArchiveError, ArchiveFormat, ArchiveRequest, OperationEvent, OperationRequestId,
    TransferConflict, WriteArchive, compress_request, compression_stages, extract_here,
    lock_main_context, never_cancelled, process_umask, run_archive, run_compression, write_fixture,
    write_staged_archive,
};
use crate::model::Location;

const CREATABLE_FORMATS: [ArchiveFormat; 4] = [
    ArchiveFormat::Zip,
    ArchiveFormat::SevenZ,
    ArchiveFormat::TarGz,
    ArchiveFormat::Tar,
];

/// ZIP, 7z, TAR, and TAR.GZ store a file and an empty directory that extract back.
#[test]
fn formats_round_trip() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let source = root.path().join("source");
    fs::create_dir_all(source.join("empty"))?;
    fs::write(source.join("file.txt"), b"contents")?;

    for format in CREATABLE_FORMATS {
        let archive = root.path().join("archive");
        write_fixture(&archive, std::slice::from_ref(&source), format, None)?;
        let extracted = root.path().join("extracted");
        let _ = fs::remove_dir_all(&extracted);
        fs::create_dir(&extracted)?;
        extract_here(&archive, &extracted)?;
        assert_eq!(
            fs::read(extracted.join("source/file.txt"))?,
            b"contents",
            "{format:?} should extract the file contents"
        );
        assert!(
            extracted.join("source/empty").is_dir(),
            "{format:?} should extract the empty directory"
        );
    }
    Ok(())
}

/// Compression rejects a password because libarchive cannot write encrypted archives.
#[test]
fn compress_password() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    for format in CREATABLE_FORMATS {
        let file = fs::File::create(root.path().join(format.archive_filename("archive")))?;
        match WriteArchive::create(file, format, Some("pass")) {
            Ok(_) => panic!("{format:?} should reject a password"),
            Err(error) => assert!(
                error.contains("password"),
                "{format:?} error should mention passwords, got {error}"
            ),
        }
    }
    Ok(())
}

/// Traditional ZipCrypto without a password asks the UI to prompt rather than failing.
///
/// libarchive's write passphrase does not emit ZipCrypto, so this uses a
/// `zip -P` fixture instead of [`write_fixture`].
#[test]
fn encrypted_without_password() -> Result<(), Box<dyn Error>> {
    let _serial = lock_main_context()?;
    let root = tempfile::tempdir()?;
    let archive = root.path().join("secret.zip");
    fs::write(
        &archive,
        decode_hex(
            "504b03040a0009000000dc88275de5e8a25c12000000060000000a001c007365637265742e747874\
             555409000380359f6a80359f6a75780b000104e803000004e8030000db26c604d38904646ebbc730\
             24d7950e708b504b0708e5e8a25c1200000006000000504b01021e030a0009000000dc88275de5e8\
             a25c12000000060000000a0018000000000001000000a481000000007365637265742e7478745554\
             05000380359f6a75780b000104e803000004e8030000504b05060000000001000100500000006600\
             00000000",
        )?,
    )?;
    let destination = root.path().join("extracted");
    fs::create_dir(&destination)?;
    let events = run_archive(ArchiveRequest {
        id: OperationRequestId(1),
        destination: Location::local(&destination),
        password: None,
        action: ArchiveAction::Extract {
            archive: Location::local(&archive),
        },
    });
    assert!(
        events
            .iter()
            .any(|event| matches!(event, OperationEvent::PasswordRequired { .. })),
        "events should include PasswordRequired, got {events:?}"
    );
    Ok(())
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

/// ZIP and TAR store a symbolic link as a link, not as a copy of the target.
#[test]
fn symlinks_zip_and_tar() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let source = root.path().join("source");
    fs::create_dir(&source)?;
    fs::write(source.join("file.txt"), b"data")?;
    std::os::unix::fs::symlink("file.txt", source.join("link"))?;

    for format in [ArchiveFormat::Zip, ArchiveFormat::Tar, ArchiveFormat::TarGz] {
        let archive = root.path().join("archive");
        write_fixture(&archive, std::slice::from_ref(&source), format, None)?;
        let extracted = root.path().join("extracted");
        let _ = fs::remove_dir_all(&extracted);
        fs::create_dir(&extracted)?;
        extract_here(&archive, &extracted)?;
        assert_eq!(
            fs::read_link(extracted.join("source/link"))?,
            Path::new("file.txt"),
            "{format:?} should restore the symbolic link"
        );
        assert_eq!(fs::read(extracted.join("source/file.txt"))?, b"data");
    }
    Ok(())
}

/// 7z refuses symbolic links and does not create an archive.
#[test]
fn seven_z_symlinks() -> Result<(), Box<dyn Error>> {
    let _serial = lock_main_context()?;
    let root = tempfile::tempdir()?;
    let source = root.path().join("source");
    fs::create_dir(&source)?;
    fs::write(source.join("file.txt"), b"contents")?;
    std::os::unix::fs::symlink("file.txt", source.join("link"))?;
    let destination = root.path().join("destination");
    fs::create_dir(&destination)?;

    let events = run_compression(compress_request(
        &source,
        &destination,
        "archive",
        ArchiveFormat::SevenZ,
        TransferConflict::FailIfExists,
    ));
    assert!(events.iter().any(|event| matches!(
        event,
        OperationEvent::Failed { message, .. }
            if message.contains("does not support symbolic links")
    )));
    assert!(!destination.join("archive.7z").exists());
    assert!(compression_stages(&destination)?.is_empty());
    Ok(())
}

/// The archive name is a single destination basename, not a path.
#[test]
fn escaping_archive_name() -> Result<(), Box<dyn Error>> {
    let _serial = lock_main_context()?;
    let root = tempfile::tempdir()?;
    let destination = root.path().join("destination");
    let source = root.path().join("source.txt");
    fs::create_dir(&destination)?;
    fs::write(&source, b"source")?;

    let events = run_compression(compress_request(
        &source,
        &destination,
        "../outside",
        ArchiveFormat::Zip,
        TransferConflict::ReplaceExisting,
    ));
    assert!(matches!(events.as_slice(), [OperationEvent::Failed { .. }]));
    assert!(!root.path().join("outside.zip").exists());
    assert!(compression_stages(&destination)?.is_empty());
    Ok(())
}

/// FailIfExists keeps the original archive; ReplaceExisting overwrites it.
#[test]
fn conflict_fail_and_replace() -> Result<(), Box<dyn Error>> {
    let _serial = lock_main_context()?;
    let root = tempfile::tempdir()?;
    let destination = root.path().join("destination");
    let source = root.path().join("source.txt");
    let archive = destination.join("existing.zip");
    fs::create_dir(&destination)?;
    fs::write(&source, b"replacement")?;
    fs::write(&archive, b"original")?;
    fs::set_permissions(&archive, fs::Permissions::from_mode(0o640))?;
    let request = |conflict| {
        compress_request(
            &source,
            &destination,
            "existing",
            ArchiveFormat::Zip,
            conflict,
        )
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
            .any(|event| matches!(event, OperationEvent::Archived { .. }))
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

/// A failed compression leaves an existing archive and its staging file alone.
#[test]
fn failure_preserves_existing() -> Result<(), Box<dyn Error>> {
    let _serial = lock_main_context()?;
    let root = tempfile::tempdir()?;
    let destination = root.path().join("destination");
    let archive = destination.join("existing.zip");
    fs::create_dir(&destination)?;
    fs::write(&archive, b"original")?;

    let events = run_compression(ArchiveRequest {
        id: OperationRequestId(1),
        destination: Location::local(&destination),
        password: None,
        action: ArchiveAction::Compress {
            sources: vec![Location::local(root.path().join("missing.txt"))],
            archive_name: "existing".to_owned(),
            format: ArchiveFormat::Zip,
            conflict: TransferConflict::ReplaceExisting,
        },
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

/// Staging is mode 0o600 while encoding, then publishes with the process umask.
#[test]
fn staging_private() -> Result<(), Box<dyn Error>> {
    let _serial = lock_main_context()?;
    let root = tempfile::tempdir()?;
    let destination = root.path().to_path_buf();
    let archive = destination.join("created.zip");
    let task = glib::MainContext::default().spawn_local(async move {
        write_staged_archive(
            &destination,
            &archive,
            TransferConflict::FailIfExists,
            &never_cancelled(),
            move |mut file| {
                let mode = file
                    .metadata()
                    .map_err(|error| error.to_string())?
                    .permissions()
                    .mode()
                    & 0o777;
                if mode != 0o600 {
                    return Err(ArchiveError::Failed(format!(
                        "staging mode was {mode:o}, expected 600"
                    )));
                }
                file.write_all(b"created")
                    .map_err(|error| error.to_string())?;
                Ok(())
            },
        )
        .await
    });
    assert_eq!(glib::MainContext::default().block_on(task)?, Ok(()));
    assert_eq!(fs::read(root.path().join("created.zip"))?, b"created");
    assert_eq!(
        fs::metadata(root.path().join("created.zip"))?
            .permissions()
            .mode()
            & 0o777,
        0o666 & !process_umask()
    );
    assert!(compression_stages(root.path())?.is_empty());
    Ok(())
}

/// Cancelling after the encoder finishes does not replace the destination.
#[test]
fn cancel_after_write() -> Result<(), Box<dyn Error>> {
    let _serial = lock_main_context()?;
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
