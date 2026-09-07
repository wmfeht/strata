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
    ArchiveError, ArchiveFormat, ArchiveOutcome, ExtractLimits, TarFilter, WriteArchive,
    always_cancelled, extract_archive, extract_here, extract_limited, validated_archive_path,
    write_archive, write_tar_filter,
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

/// TAR hard links share the source inode instead of copying bytes.
#[test]
fn hardlink_same_inode() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let destination = root.path().join("destination");
    fs::create_dir(&destination)?;
    let archive = root.path().join("links.tar");
    let mut writer = WriteArchive::create(fs::File::create(&archive)?, ArchiveFormat::Tar, None)?;
    writer.write_file_header(Path::new("file.txt"), 4, 0o644, None)?;
    writer.write_all(b"data")?;
    writer.finish_entry()?;
    writer.write_hardlink(Path::new("link"), OsStr::new("file.txt"))?;
    writer.finish()?;

    extract_here(&archive, &destination)?;
    let file = fs::metadata(destination.join("file.txt"))?;
    let link = fs::metadata(destination.join("link"))?;
    assert_eq!(
        file.ino(),
        link.ino(),
        "extracted hard link should share the source inode"
    );
    assert_eq!(
        file.nlink(),
        2,
        "extracted hard link should increment nlink"
    );
    assert_eq!(fs::read(destination.join("link"))?, b"data");
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
