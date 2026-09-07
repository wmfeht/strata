// SPDX-License-Identifier: GPL-3.0-or-later

use super::{ArchiveFormat, is_extractable_archive, validate_basename};

#[test]
fn basenames_reject_empty_reserved_nested_absolute_and_nul_names() {
    for name in [
        "",
        ".",
        "..",
        "../escaped",
        "nested/child",
        "/tmp/absolute",
        "nul\0name",
    ] {
        assert!(
            validate_basename(name).is_err(),
            "{name:?} should be rejected"
        );
    }
}

#[test]
fn basenames_accept_single_native_and_unicode_components() {
    for name in ["report.txt", "folder name", ".config", "résumé"] {
        assert!(
            validate_basename(name).is_ok(),
            "{name:?} should be accepted"
        );
    }
}

#[test]
fn archive_formats_are_detected_by_extension() {
    assert_eq!(
        ArchiveFormat::from_extension("photos.zip"),
        Some(ArchiveFormat::Zip)
    );
    assert_eq!(
        ArchiveFormat::from_extension("backup.tar.gz"),
        Some(ArchiveFormat::TarGz)
    );
    assert_eq!(
        ArchiveFormat::from_extension("archive.TGZ"),
        Some(ArchiveFormat::TarGz)
    );
    assert_eq!(
        ArchiveFormat::from_extension("data.tar"),
        Some(ArchiveFormat::Tar)
    );
    assert_eq!(ArchiveFormat::from_extension("backup.tar.xz"), None);
    assert_eq!(ArchiveFormat::from_extension("notes.gz"), None);
    assert_eq!(ArchiveFormat::from_extension("payload.rar"), None);
    assert_eq!(ArchiveFormat::from_extension("document.pdf"), None);
    assert_eq!(ArchiveFormat::from_extension("no_extension"), None);
}

#[test]
fn archive_format_extensions_round_trip() {
    for format in [
        ArchiveFormat::Zip,
        ArchiveFormat::SevenZ,
        ArchiveFormat::TarGz,
        ArchiveFormat::Tar,
    ] {
        let name = format!("test.{}", format.extension());
        assert_eq!(ArchiveFormat::from_extension(&name), Some(format));
        assert_eq!(
            format.archive_filename("test"),
            name,
            "{format:?} should publish the stem plus its extension"
        );
    }
}

#[test]
fn extractable_archives_include_read_only_suffixes() {
    for name in [
        "photos.zip",
        "backup.tar.gz",
        "archive.TGZ",
        "data.tar",
        "backup.tar.xz",
        "backup.tzst",
        "backup.tbz2",
        "notes.gz",
        "payload.rar",
    ] {
        assert!(
            is_extractable_archive(name),
            "{name} should be treated as extractable"
        );
    }
    assert!(!is_extractable_archive("document.pdf"));
    assert!(!is_extractable_archive("no_extension"));
}
