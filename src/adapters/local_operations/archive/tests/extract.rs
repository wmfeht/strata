// SPDX-License-Identifier: GPL-3.0-or-later

use std::{
    error::Error,
    fs,
    path::Path,
    sync::{Arc, atomic::AtomicUsize},
};

use exarch_core::formats::detect::ArchiveType;

use super::{
    ArchiveError, ArchiveFormat, ArchiveOutcome, ExtractLimits, OperationEvent, always_cancelled,
    append_tar_file, append_tar_hardlink, append_tar_named, append_tar_symlink, decode_hex,
    extract_archive, extract_here, extract_limited, extract_request, extract_with_password,
    lock_main_context, run_archive, write_archive, write_tar_members, write_typed_archive,
};

/// Nested members extract together.
#[test]
fn nested_members() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let destination = root.path().join("destination");
    fs::create_dir(&destination)?;
    let archive = root.path().join("content.zip");
    write_archive(
        &archive,
        ArchiveFormat::Zip,
        &[
            ("folder/nested/item.txt", Some(b"nested".as_slice())),
            ("report.txt", Some(b"replacement".as_slice())),
        ],
    )?;

    extract_here(&archive, &destination)?;
    assert_eq!(
        fs::read(destination.join("folder/nested/item.txt"))?,
        b"nested"
    );
    assert_eq!(fs::read(destination.join("report.txt"))?, b"replacement");
    Ok(())
}

/// TAR archives that include a `.` / `./` root still extract their real members.
#[test]
fn tar_root_directory() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let destination = root.path().join("destination");
    fs::create_dir(&destination)?;
    let archive = root.path().join("content.tar");
    write_tar_members(&archive, |builder| {
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Directory);
        header.set_size(0);
        header.set_mode(0o755);
        header.set_cksum();
        builder.append_data(&mut header, "./", std::io::empty())?;
        append_tar_file(builder, "./folder/item.txt", b"contents")?;
        Ok(())
    })?;

    extract_here(&archive, &destination)?;
    assert_eq!(fs::read(destination.join("folder/item.txt"))?, b"contents");
    Ok(())
}

/// ZIP, TAR, and TAR.GZ refuse a member that walks above the destination.
#[test]
fn parent_traversal() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let destination = root.path().join("destination");
    fs::create_dir(&destination)?;

    let tar = root.path().join("malicious.tar");
    write_tar_members(&tar, |builder| {
        append_tar_named(builder, "../marker", b"escaped")?;
        Ok(())
    })?;
    assert!(
        extract_here(&tar, &destination).is_err(),
        "TAR should refuse a parent-traversal member"
    );

    let tar_gz = root.path().join("malicious.tar.gz");
    write_typed_archive(
        &tar_gz,
        ArchiveType::TarGz,
        &[("safe.txt", Some(b"ok".as_slice()))],
    )?;
    extract_here(&tar_gz, &destination)?;

    let zip = root.path().join("malicious.zip");
    write_zip_member(&zip, "../marker", b"escaped")?;
    assert!(
        extract_here(&zip, &destination).is_err(),
        "ZIP should refuse a parent-traversal member"
    );
    assert!(!root.path().join("marker").exists());
    Ok(())
}

/// Relative symlinks are restored; escaping targets are refused.
#[test]
fn symlink_policy() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;

    let destination = root.path().join("safe");
    fs::create_dir(&destination)?;
    let safe = root.path().join("safe.tar");
    write_tar_members(&safe, |builder| {
        append_tar_file(builder, "file.txt", b"data")?;
        append_tar_symlink(builder, "link", "file.txt")?;
        Ok(())
    })?;
    extract_here(&safe, &destination)?;
    assert_eq!(fs::read(destination.join("file.txt"))?, b"data");
    assert_eq!(
        fs::read_link(destination.join("link"))?,
        Path::new("file.txt")
    );

    let escape_dir = root.path().join("escape");
    fs::create_dir(&escape_dir)?;
    let unsafe_archive = root.path().join("unsafe.tar");
    write_tar_members(&unsafe_archive, |builder| {
        append_tar_symlink(builder, "escape", "../outside")?;
        Ok(())
    })?;
    assert!(extract_here(&unsafe_archive, &escape_dir).is_err());
    assert!(!root.path().join("outside").exists());
    Ok(())
}

/// A hard link that appears before its referent still resolves after the archive is exhausted.
#[test]
fn hardlink_forward_reference() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let destination = root.path().join("destination");
    fs::create_dir(&destination)?;
    let archive = root.path().join("links.tar");
    write_tar_members(&archive, |builder| {
        append_tar_file(builder, "file.txt", b"data")?;
        append_tar_hardlink(builder, "link", "file.txt")?;
        Ok(())
    })?;

    extract_here(&archive, &destination)?;
    assert_eq!(fs::read(destination.join("file.txt"))?, b"data");
    assert_eq!(fs::read(destination.join("link"))?, b"data");
    Ok(())
}

/// Hard-link chains (link → link → file) resolve across passes.
#[test]
fn hardlink_chain() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let destination = root.path().join("destination");
    fs::create_dir(&destination)?;
    let archive = root.path().join("links.tar");
    write_tar_members(&archive, |builder| {
        append_tar_file(builder, "file.txt", b"data")?;
        append_tar_hardlink(builder, "link1", "file.txt")?;
        append_tar_hardlink(builder, "link2", "link1")?;
        Ok(())
    })?;

    extract_here(&archive, &destination)?;
    assert_eq!(fs::read(destination.join("file.txt"))?, b"data");
    assert_eq!(fs::read(destination.join("link1"))?, b"data");
    assert_eq!(fs::read(destination.join("link2"))?, b"data");
    Ok(())
}

/// A hard link whose target never appears in the archive is refused.
#[test]
fn hardlink_missing_target() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let destination = root.path().join("destination");
    fs::create_dir(&destination)?;
    let archive = root.path().join("links.tar");
    write_tar_members(&archive, |builder| {
        append_tar_file(builder, "file.txt", b"data")?;
        append_tar_hardlink(builder, "link", "missing.txt")?;
        Ok(())
    })?;

    let error = extract_here(&archive, &destination)
        .expect_err("a hard link to a missing member should be refused");
    assert!(
        error.to_ascii_lowercase().contains("hard")
            || error.to_ascii_lowercase().contains("link")
            || error.to_ascii_lowercase().contains("missing"),
        "{error}"
    );
    Ok(())
}

/// Cancelling before any member is written leaves the destination empty.
#[test]
fn cancellation() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let destination = root.path().join("destination");
    fs::create_dir(&destination)?;
    let archive = root.path().join("content.zip");
    write_archive(
        &archive,
        ArchiveFormat::Zip,
        &[
            ("first.bin", Some(b"early".as_slice())),
            ("second.txt", Some(b"late".as_slice())),
        ],
    )?;

    let outcome = extract_archive(
        &archive,
        &destination,
        None,
        &Arc::new(AtomicUsize::new(0)),
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
            assert!(not_attempted.is_empty());
        }
        ArchiveOutcome::Completed(_) => panic!("extraction should stop when cancelled"),
    }
    assert!(destination.read_dir()?.next().is_none());
    Ok(())
}

/// A small highly compressible archive is refused as a zip bomb.
#[test]
fn bomb_ratio() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let destination = root.path().join("destination");
    fs::create_dir(&destination)?;
    let archive = root.path().join("bomb.zip");
    let zeros = vec![0_u8; 64 * 1024];
    write_archive(
        &archive,
        ArchiveFormat::Zip,
        &[("zeros.bin", Some(zeros.as_slice()))],
    )?;
    let error = extract_limited(
        &archive,
        &destination,
        ExtractLimits::for_test(10 * 1024 * 1024, 100, 16, 2),
    )
    .expect_err("a high-ratio archive should be refused");
    assert!(
        matches!(
            error,
            ArchiveError::Failed(ref message)
                if message.contains("expands beyond the safety limit")
                    || message.to_ascii_lowercase().contains("bomb")
                    || message.to_ascii_lowercase().contains("ratio")
        ),
        "{error:?}"
    );
    assert!(!destination.join("zeros.bin").exists());
    Ok(())
}

/// Extraction stops when the member-count limit is exceeded.
#[test]
fn bomb_member_count() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let destination = root.path().join("destination");
    fs::create_dir(&destination)?;
    let archive = root.path().join("many.zip");
    write_archive(
        &archive,
        ArchiveFormat::Zip,
        &[
            ("one.txt", Some(b"a".as_slice())),
            ("two.txt", Some(b"b".as_slice())),
            ("three.txt", Some(b"c".as_slice())),
        ],
    )?;
    let error = extract_limited(
        &archive,
        &destination,
        ExtractLimits::for_test(10 * 1024 * 1024, 2, 16, 200),
    )
    .expect_err("an over-limit member count should be refused");
    assert!(
        matches!(
            error,
            ArchiveError::Failed(ref message) if message.contains("more than 2 files")
                || message.to_ascii_lowercase().contains("quota")
                || message.to_ascii_lowercase().contains("count")
        ),
        "{error:?}"
    );
    Ok(())
}

/// Extraction stops when a member path is nested beyond the depth limit.
#[test]
fn bomb_path_depth() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let destination = root.path().join("destination");
    fs::create_dir(&destination)?;
    let archive = root.path().join("deep.zip");
    write_archive(
        &archive,
        ArchiveFormat::Zip,
        &[("a/b/c/d.txt", Some(b"nested".as_slice()))],
    )?;
    let error = extract_limited(
        &archive,
        &destination,
        ExtractLimits::for_test(10 * 1024 * 1024, 100, 2, 200),
    )
    .expect_err("an over-limit path depth should be refused");
    assert!(
        matches!(
            error,
            ArchiveError::Failed(ref message)
                if message.contains("nested more than") || message.to_ascii_lowercase().contains("depth")
        ),
        "{error:?}"
    );
    Ok(())
}

/// Extract-only tar filters still restore a file written by exarch_core.
#[test]
fn extract_only_round_trip() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    for format in [ArchiveType::TarXz, ArchiveType::TarZst, ArchiveType::TarBz2] {
        let archive = root.path().join(format!("archive.{format:?}"));
        write_typed_archive(
            &archive,
            format,
            &[("file.txt", Some(b"contents".as_slice()))],
        )
        .map_err(|error| format!("{format:?} write failed: {error}"))?;
        let extracted = root.path().join("extracted");
        let _ = fs::remove_dir_all(&extracted);
        fs::create_dir(&extracted)?;
        extract_here(&archive, &extracted)
            .map_err(|error| format!("{format:?} extract failed: {error}"))?;
        assert_eq!(
            fs::read(extracted.join("file.txt"))?,
            b"contents",
            "{format:?} should extract the file contents"
        );
    }
    Ok(())
}

const ZIPCRYPTO_SECRET: &str = "\
504b03040a0009000000dc88275de5e8a25c12000000060000000a001c007365637265742e747874\
555409000380359f6a80359f6a75780b000104e803000004e8030000db26c604d38904646ebbc730\
24d7950e708b504b0708e5e8a25c1200000006000000504b01021e030a0009000000dc88275de5e8\
a25c12000000060000000a0018000000000001000000a481000000007365637265742e7478745554\
05000380359f6a75780b000104e803000004e8030000504b05060000000001000100500000006600\
00000000";

/// Encrypted ZIP is refused instead of prompting: exarch_core cannot decrypt.
#[test]
fn encrypted_zip_unsupported() -> Result<(), Box<dyn Error>> {
    let _serial = lock_main_context()?;
    let root = tempfile::tempdir()?;
    let archive = root.path().join("secret.zip");
    fs::write(&archive, decode_hex(ZIPCRYPTO_SECRET)?)?;
    let destination = root.path().join("extracted");
    fs::create_dir(&destination)?;
    let events = run_archive(extract_request(&archive, &destination, None));
    assert!(
        events.iter().any(|event| matches!(
            event,
            OperationEvent::Failed { message, .. }
                if message.contains("cannot be opened") || message.to_ascii_lowercase().contains("password")
        )),
        "encrypted ZIP should fail without prompting, got {events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, OperationEvent::PasswordRequired { .. })),
        "encrypted ZIP must not prompt, got {events:?}"
    );
    assert!(destination.read_dir()?.next().is_none());
    Ok(())
}

/// WinZip AES-256 ZIP is refused instead of decrypting.
#[test]
fn encrypted_zip_aes256_unsupported() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let archive = root.path().join("secret.zip");
    fs::write(&archive, include_bytes!("winzip_aes256.zip"))?;
    let destination = root.path().join("extracted");
    fs::create_dir(&destination)?;
    let error = extract_with_password(&archive, &destination, Some("password"))
        .expect_err("AES-256 ZIP should be refused");
    assert!(
        error.contains("cannot be opened") || error.to_ascii_lowercase().contains("password"),
        "{error}"
    );
    Ok(())
}

/// Encrypted 7z cannot be decrypted; extract fails instead of prompting.
#[test]
fn encrypted_7z_unsupported() -> Result<(), Box<dyn Error>> {
    let _serial = lock_main_context()?;
    let root = tempfile::tempdir()?;
    let archive = root.path().join("secret.7z");
    fs::write(
        &archive,
        decode_hex(
            "377abcaf271c000382afea881000000000000000610000000000000058ff6c6ab90d88d0ac2d6ba3\
             5abbe535dfd141d90104060001091000070b0100022406f107010a5307d9646d649abf0ed5230301\
             01055d0000010001000c080400080a01a865327e000005011111006200610072002e007400780074\
             000000140a0100008636a879b0ce01150601002080b4810000",
        )?,
    )?;
    let destination = root.path().join("extracted");
    fs::create_dir(&destination)?;
    let events = run_archive(extract_request(&archive, &destination, None));
    assert!(
        events
            .iter()
            .any(|event| matches!(event, OperationEvent::Failed { .. })),
        "encrypted 7z should fail without prompting, got {events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, OperationEvent::PasswordRequired { .. })),
        "encrypted 7z must not prompt, got {events:?}"
    );
    Ok(())
}

/// Encrypted RAR cannot be extracted; extract fails instead of prompting.
#[test]
fn encrypted_rar_unsupported() -> Result<(), Box<dyn Error>> {
    let _serial = lock_main_context()?;
    let root = tempfile::tempdir()?;
    let archive = root.path().join("secret.rar");
    fs::write(
        &archive,
        decode_hex(
            "526172211a0701003392b5e50a0105060005010180800035ff321e1f02030b12041220556e5aee80\
             000005612e7478740a03027c2184a3827cda01546869732069732066726f6d20612e7478747a4171\
             b35002033c300412201845a30280030005622e747874300100030fc7445ee180f8b59fd62b433708\
             bc57cdfa34df06ab5d05d49307770f1d8e8053ea35671d70f12d4b6b9c1a9c0a0302726598a6827c\
             da0163ba54b2d010a5495fb4bad3c069a83b1bbcbb3827d7879378732f7c93e2d228c400defea6c0\
             3b7e6c67d4055000dcf88d7cf1661f02030b12041220353d9a9480000005632e7478740a03023e48\
             e9ae827cda01546869732069732066726f6d20632e747874ecfdf05d5202033ca00004920020c3df\
             631280030005642e747874300100030fc7445ee180f8b59fd62b433708bc57cdf3cdd6b31cb45ba7\
             7cac36588eb30e69834cd448e915ebd6ac77b7a30a03027bc529f48f7cda01db6972f4f6568ea6fd\
             1965e995b336853437f52aa0d32787b2f5c10232d8aa5e1d77565103050400",
        )?,
    )?;
    let destination = root.path().join("extracted");
    fs::create_dir(&destination)?;
    let events = run_archive(extract_request(&archive, &destination, None));
    assert!(
        events
            .iter()
            .any(|event| matches!(event, OperationEvent::Failed { .. })),
        "encrypted RAR should fail without prompting, got {events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, OperationEvent::PasswordRequired { .. })),
        "encrypted RAR must not prompt, got {events:?}"
    );
    Ok(())
}

/// A member whose path contains "password" is still a policy failure, not a prompt.
#[test]
fn password_named_member_is_not_password_required() -> Result<(), Box<dyn Error>> {
    let _serial = lock_main_context()?;
    let root = tempfile::tempdir()?;
    let archive = root.path().join("content.tar");
    write_tar_members(&archive, |builder| {
        append_tar_named(builder, "../password.txt", b"escaped")?;
        Ok(())
    })?;
    let destination = root.path().join("extracted");
    fs::create_dir(&destination)?;
    let events = run_archive(extract_request(&archive, &destination, None));
    assert!(
        events
            .iter()
            .any(|event| matches!(event, OperationEvent::Failed { .. })),
        "a parent-traversal member named password.txt should fail, got {events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, OperationEvent::PasswordRequired { .. })),
        "policy refusal must not become PasswordRequired, got {events:?}"
    );
    Ok(())
}

/// A member whose claimed size exceeds the byte budget is refused.
#[test]
fn claimed_size_exceeds_budget() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let destination = root.path().join("destination");
    fs::create_dir(&destination)?;
    let archive = root.path().join("big.zip");
    write_archive(
        &archive,
        ArchiveFormat::Zip,
        &[("blob.bin", Some(vec![0_u8; 200].as_slice()))],
    )?;
    let error = extract_limited(
        &archive,
        &destination,
        ExtractLimits::for_test(100, 100, 16, 200),
    )
    .expect_err("a member larger than the byte budget should be refused");
    assert!(
        matches!(
            error,
            ArchiveError::Failed(ref message)
                if message.contains("free space")
                    || message.to_ascii_lowercase().contains("quota")
                    || message.to_ascii_lowercase().contains("size")
        ),
        "{error:?}"
    );
    assert!(!destination.join("blob.bin").exists());
    Ok(())
}

fn write_zip_member(path: &Path, name: &str, bytes: &[u8]) -> Result<(), Box<dyn Error>> {
    let file = fs::File::create(path)?;
    let mut zip = zip::ZipWriter::new(file);
    zip.start_file(
        name,
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored),
    )?;
    std::io::Write::write_all(&mut zip, bytes)?;
    zip.finish()?;
    Ok(())
}
