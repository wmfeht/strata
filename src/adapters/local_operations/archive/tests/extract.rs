// SPDX-License-Identifier: GPL-3.0-or-later

use std::{
    error::Error,
    ffi::OsStr,
    fs,
    io::Write as _,
    os::unix::fs::MetadataExt as _,
    path::Path,
    sync::{Arc, atomic::AtomicUsize},
};

use super::{
    ArchiveError, ArchiveFormat, ArchiveOutcome, ExtractLimits, OperationEvent, TarFilter,
    WriteArchive, always_cancelled, cancel_after_next_copy_chunk, decode_hex, extract_archive,
    extract_here, extract_limited, extract_request, extract_with_password, lock_main_context,
    never_cancelled, run_archive, validated_archive_path, write_archive, write_tar_filter,
};

const EXTRACT_ONLY_FILTERS: [TarFilter; 3] = [TarFilter::Xz, TarFilter::Zstd, TarFilter::Bzip2];

/// Nested members extract together; colliding top-level names get a unique suffix.
#[test]
fn nested_with_conflicts() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let destination = root.path().join("destination");
    fs::create_dir(&destination)?;
    fs::write(destination.join("report.txt"), b"original")?;
    fs::create_dir(destination.join("existing"))?;
    fs::write(destination.join("existing/old.txt"), b"old")?;
    let archive = root.path().join("content.zip");
    write_archive(
        &archive,
        ArchiveFormat::Zip,
        &[
            ("folder/nested/item.txt", Some(b"nested".as_slice())),
            ("report.txt", Some(b"replacement".as_slice())),
            ("existing/new.txt", Some(b"new".as_slice())),
        ],
    )?;

    assert_eq!(
        extract_here(&archive, &destination)?.as_deref(),
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

/// TAR archives that include a `.` / `./` root still extract their real members.
#[test]
fn tar_root_directory() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let destination = root.path().join("destination");
    fs::create_dir(&destination)?;
    let archive = root.path().join("content.tar");
    write_archive(
        &archive,
        ArchiveFormat::Tar,
        &[
            ("./", None),
            ("./folder/item.txt", Some(b"contents".as_slice())),
        ],
    )?;

    assert_eq!(
        extract_here(&archive, &destination)?,
        Some("folder".to_owned())
    );
    assert_eq!(fs::read(destination.join("folder/item.txt"))?, b"contents");
    assert_eq!(fs::read_dir(&destination)?.count(), 1);
    Ok(())
}

/// Every creatable format refuses a member that walks above the destination.
#[test]
fn parent_traversal() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let destination = root.path().join("destination");
    fs::create_dir(&destination)?;

    for format in [
        ArchiveFormat::Zip,
        ArchiveFormat::Tar,
        ArchiveFormat::TarGz,
        ArchiveFormat::SevenZ,
    ] {
        let archive = root.path().join("malicious");
        write_archive(
            &archive,
            format,
            &[("../marker", Some(b"escaped".as_slice()))],
        )?;
        assert!(
            extract_here(&archive, &destination).is_err(),
            "{format:?} should refuse a parent-traversal member"
        );
    }
    assert!(!root.path().join("marker").exists());
    Ok(())
}

/// Relative symlinks are restored; escaping targets and destination symlinks are not followed.
#[test]
fn symlink_policy() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;

    let destination = root.path().join("safe");
    fs::create_dir(&destination)?;
    let safe = root.path().join("safe.tar");
    let mut writer = WriteArchive::create(fs::File::create(&safe)?, ArchiveFormat::Tar, None)?;
    writer.write_file_header(Path::new("file.txt"), 4, 0o644, None)?;
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

    let escape_dir = root.path().join("escape");
    fs::create_dir(&escape_dir)?;
    let unsafe_archive = root.path().join("unsafe.tar");
    let mut writer =
        WriteArchive::create(fs::File::create(&unsafe_archive)?, ArchiveFormat::Tar, None)?;
    writer.write_symlink(Path::new("escape"), OsStr::new("../outside"))?;
    writer.finish()?;
    assert!(extract_here(&unsafe_archive, &escape_dir).is_err());
    assert!(!escape_dir.join("escape").exists());
    assert!(!root.path().join("outside").exists());

    let redirected = root.path().join("redirected");
    let external = root.path().join("external");
    fs::create_dir(&redirected)?;
    fs::create_dir(&external)?;
    std::os::unix::fs::symlink(root.path().join("missing"), redirected.join("dangling"))?;
    std::os::unix::fs::symlink(&external, redirected.join("redirect"))?;
    let final_archive = root.path().join("final.zip");
    let intermediate_archive = root.path().join("intermediate.zip");
    write_archive(
        &final_archive,
        ArchiveFormat::Zip,
        &[("dangling", Some(b"escaped".as_slice()))],
    )?;
    write_archive(
        &intermediate_archive,
        ArchiveFormat::Zip,
        &[("redirect/marker", Some(b"escaped".as_slice()))],
    )?;
    assert!(extract_here(&final_archive, &redirected).is_err());
    assert!(extract_here(&intermediate_archive, &redirected).is_err());
    assert!(!root.path().join("missing").exists());
    assert!(!external.join("marker").exists());
    Ok(())
}

/// A hard link that appears before its referent still resolves after the archive is exhausted.
#[test]
fn hardlink_forward_reference() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let destination = root.path().join("destination");
    fs::create_dir(&destination)?;
    let archive = root.path().join("links.tar");
    let mut writer = WriteArchive::create(fs::File::create(&archive)?, ArchiveFormat::Tar, None)?;
    writer.write_hardlink(Path::new("link"), OsStr::new("file.txt"))?;
    writer.write_file_header(Path::new("file.txt"), 4, 0o644, None)?;
    writer.write_all(b"data")?;
    writer.finish_entry()?;
    writer.finish()?;

    extract_here(&archive, &destination)?;
    let file = fs::metadata(destination.join("file.txt"))?;
    let link = fs::metadata(destination.join("link"))?;
    assert_eq!(
        file.ino(),
        link.ino(),
        "forward-reference hard link should share the source inode"
    );
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
    let mut writer = WriteArchive::create(fs::File::create(&archive)?, ArchiveFormat::Tar, None)?;
    writer.write_hardlink(Path::new("link2"), OsStr::new("link1"))?;
    writer.write_hardlink(Path::new("link1"), OsStr::new("file.txt"))?;
    writer.write_file_header(Path::new("file.txt"), 4, 0o644, None)?;
    writer.write_all(b"data")?;
    writer.finish_entry()?;
    writer.finish()?;

    extract_here(&archive, &destination)?;
    let file = fs::metadata(destination.join("file.txt"))?;
    let link1 = fs::metadata(destination.join("link1"))?;
    let link2 = fs::metadata(destination.join("link2"))?;
    assert_eq!(
        file.ino(),
        link1.ino(),
        "chain link1 should share the source inode"
    );
    assert_eq!(
        file.ino(),
        link2.ino(),
        "chain link2 should share the source inode"
    );
    assert_eq!(
        file.nlink(),
        3,
        "chain should produce three directory entries"
    );
    Ok(())
}

/// A hard link whose target never appears in the archive is refused.
#[test]
fn hardlink_missing_target() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let destination = root.path().join("destination");
    fs::create_dir(&destination)?;
    let archive = root.path().join("links.tar");
    let mut writer = WriteArchive::create(fs::File::create(&archive)?, ArchiveFormat::Tar, None)?;
    writer.write_file_header(Path::new("file.txt"), 4, 0o644, None)?;
    writer.write_all(b"data")?;
    writer.finish_entry()?;
    writer.write_hardlink(Path::new("link"), OsStr::new("missing.txt"))?;
    writer.finish()?;

    let error = extract_here(&archive, &destination)
        .expect_err("a hard link to a missing member should be refused");
    assert!(
        error.contains("Hard link target was not extracted"),
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
            ArchiveError::Failed(ref message) if message.contains("nested more than")
        ),
        "{error:?}"
    );
    Ok(())
}

/// Extract-only tar filters still restore a file written by libarchive.
///
/// RAR cannot be encoded. Single-stream `.gz` / `.xz` / `.zst` / `.bz2` need
/// libarchive's raw format, which the reader does not enable. Creatable
/// formats are covered by [`super::compress::formats_round_trip`].
#[test]
fn extract_only_round_trip() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    for filter in EXTRACT_ONLY_FILTERS {
        let archive = root.path().join(format!("archive.{}", filter.extension()));
        write_tar_filter(
            &archive,
            filter,
            &[("file.txt", Some(b"contents".as_slice()))],
        )
        .map_err(|error| format!("{filter:?} write failed: {error}"))?;
        let extracted = root.path().join("extracted");
        let _ = fs::remove_dir_all(&extracted);
        fs::create_dir(&extracted)?;
        extract_here(&archive, &extracted)
            .map_err(|error| format!("{filter:?} extract failed: {error}"))?;
        assert_eq!(
            fs::read(extracted.join("file.txt"))?,
            b"contents",
            "{filter:?} should extract the file contents"
        );
    }
    Ok(())
}

/// Member names must be confined relative paths; empty and `..` names are refused.
#[test]
fn member_paths() -> Result<(), Box<dyn Error>> {
    for path in ["", ".", "./", "../marker", "/tmp/marker", "C:marker"] {
        assert!(
            validated_archive_path(path).is_err(),
            "{path:?} should be rejected"
        );
    }
    assert_eq!(
        validated_archive_path("folder/./nested//item.txt")?,
        Path::new("folder/nested/item.txt")
    );
    Ok(())
}

const ZIPCRYPTO_SECRET: &str = "\
504b03040a0009000000dc88275de5e8a25c12000000060000000a001c007365637265742e747874\
555409000380359f6a80359f6a75780b000104e803000004e8030000db26c604d38904646ebbc730\
24d7950e708b504b0708e5e8a25c1200000006000000504b01021e030a0009000000dc88275de5e8\
a25c12000000060000000a0018000000000001000000a481000000007365637265742e7478745554\
05000380359f6a75780b000104e803000004e8030000504b05060000000001000100500000006600\
00000000";

/// Traditional ZipCrypto without a password asks the UI to prompt rather than failing.
#[test]
fn encrypted_zip_without_password() -> Result<(), Box<dyn Error>> {
    let _serial = lock_main_context()?;
    let root = tempfile::tempdir()?;
    let archive = root.path().join("secret.zip");
    fs::write(&archive, decode_hex(ZIPCRYPTO_SECRET)?)?;
    let destination = root.path().join("extracted");
    fs::create_dir(&destination)?;
    let events = run_archive(extract_request(&archive, &destination, None));
    assert!(
        events
            .iter()
            .any(|event| matches!(event, OperationEvent::PasswordRequired { .. })),
        "events should include PasswordRequired, got {events:?}"
    );
    assert!(
        destination.read_dir()?.next().is_none(),
        "password prompt must not leave extracted members behind"
    );
    Ok(())
}

/// Nested encrypted ZIP members must not leave directories behind before the prompt.
///
/// A later retry with the password would otherwise extract into `folder (2)/`.
#[test]
fn encrypted_zip_nested_without_password() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let archive = root.path().join("mixed.zip");
    fs::write(&archive, include_bytes!("mixed_zipcrypto.zip"))?;
    let destination = root.path().join("extracted");
    fs::create_dir(&destination)?;
    let error = extract_archive(
        &archive,
        &destination,
        None,
        &Arc::new(AtomicUsize::new(0)),
        &never_cancelled(),
    )
    .expect_err("encrypted nested ZIP should prompt rather than extract");
    assert!(
        matches!(error, ArchiveError::NeedsPassword(_)),
        "nested encrypted ZIP should be NeedsPassword, got {error:?}"
    );
    assert!(
        destination.read_dir()?.next().is_none(),
        "password prompt must not leave extracted members behind"
    );
    extract_with_password(&archive, &destination, Some("pass"))?;
    assert_eq!(fs::read(destination.join("readme.txt"))?, b"plain");
    assert_eq!(
        fs::read(destination.join("folder/nested/secret.txt"))?,
        b"secret"
    );
    assert!(!destination.join("folder (2)").exists());
    assert!(!destination.join("readme (2).txt").exists());
    Ok(())
}

/// WinZip AES-256 ZIP created by earlier Strata versions still extracts.
#[test]
fn encrypted_zip_aes256_with_password() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let archive = root.path().join("secret.zip");
    fs::write(&archive, include_bytes!("winzip_aes256.zip"))?;
    let destination = root.path().join("extracted");
    fs::create_dir(&destination)?;
    extract_with_password(&archive, &destination, Some("password"))?;
    let readme = fs::read(destination.join("README"))?;
    assert_eq!(
        readme.len(),
        6818,
        "AES-256 ZIP should restore the README payload"
    );
    Ok(())
}

/// WinZip AES-256 ZIP without a password prompts rather than failing.
#[test]
fn encrypted_zip_aes256_without_password() -> Result<(), Box<dyn Error>> {
    let _serial = lock_main_context()?;
    let root = tempfile::tempdir()?;
    let archive = root.path().join("secret.zip");
    fs::write(&archive, include_bytes!("winzip_aes256.zip"))?;
    let destination = root.path().join("extracted");
    fs::create_dir(&destination)?;
    let events = run_archive(extract_request(&archive, &destination, None));
    assert!(
        events
            .iter()
            .any(|event| matches!(event, OperationEvent::PasswordRequired { .. })),
        "AES-256 ZIP should prompt, got {events:?}"
    );
    assert!(
        destination.read_dir()?.next().is_none(),
        "AES-256 password prompt must not leave extracted members behind"
    );
    Ok(())
}

/// The same ZipCrypto fixture extracts after the correct password is supplied.
#[test]
fn encrypted_zip_with_password() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let archive = root.path().join("secret.zip");
    fs::write(&archive, decode_hex(ZIPCRYPTO_SECRET)?)?;
    let destination = root.path().join("extracted");
    fs::create_dir(&destination)?;
    extract_with_password(&archive, &destination, Some("pass"))?;
    assert_eq!(fs::read(destination.join("secret.txt"))?, b"secret");
    Ok(())
}

/// A wrong ZipCrypto password is a terminal failure, not another prompt.
#[test]
fn encrypted_zip_wrong_password() -> Result<(), Box<dyn Error>> {
    let _serial = lock_main_context()?;
    let root = tempfile::tempdir()?;
    let archive = root.path().join("secret.zip");
    fs::write(&archive, decode_hex(ZIPCRYPTO_SECRET)?)?;
    let destination = root.path().join("extracted");
    fs::create_dir(&destination)?;
    let events = run_archive(extract_request(
        &archive,
        &destination,
        Some("nope".to_owned()),
    ));
    assert!(
        events.iter().any(|event| matches!(
            event,
            OperationEvent::Failed { message, .. } if message.contains("Incorrect password")
        )),
        "wrong password should fail, got {events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, OperationEvent::PasswordRequired { .. })),
        "wrong password must not re-prompt, got {events:?}"
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
        events.iter().any(|event| matches!(
            event,
            OperationEvent::Failed { message, .. }
                if message.contains("cannot be opened")
        )),
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

/// Encrypted RAR5 cannot be decrypted; extract fails instead of prompting.
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
    let archive = root.path().join("content.zip");
    write_archive(
        &archive,
        ArchiveFormat::Zip,
        &[("../password.txt", Some(b"escaped".as_slice()))],
    )?;
    let destination = root.path().join("extracted");
    fs::create_dir(&destination)?;
    let events = run_archive(extract_request(&archive, &destination, None));
    assert!(
        events.iter().any(|event| matches!(
            event,
            OperationEvent::Failed { message, .. } if message.contains("unsafe archive path")
        )),
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

/// A member whose claimed size exceeds free space is refused before any write.
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
            ArchiveError::Failed(ref message) if message.contains("free space")
        ),
        "{error:?}"
    );
    assert!(!destination.join("blob.bin").exists());
    Ok(())
}

/// Cancelling after a member has been created removes that incomplete file.
#[test]
fn cancelled_extract_drops_incomplete_file() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let destination = root.path().join("destination");
    fs::create_dir(&destination)?;
    let archive = root.path().join("large.zip");
    let payload = vec![0x5a_u8; 2 * 1024 * 1024];
    write_archive(
        &archive,
        ArchiveFormat::Zip,
        &[("large.bin", Some(payload.as_slice()))],
    )?;

    let _cancel = cancel_after_next_copy_chunk();
    let outcome = extract_archive(
        &archive,
        &destination,
        None,
        &Arc::new(AtomicUsize::new(0)),
        &never_cancelled(),
    )?;
    assert!(
        matches!(outcome, ArchiveOutcome::Cancelled { .. }),
        "a mid-write cancel should not complete the extract, got {outcome:?}"
    );
    assert!(
        !destination.join("large.bin").exists(),
        "cancelled extract should not leave a partial member"
    );
    Ok(())
}
