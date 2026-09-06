// SPDX-License-Identifier: GPL-3.0-or-later

use super::{
    ArchiveFormat, ConflictChoice, TransferConflict, resolve_conflict_choice, validate_basename,
};

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
    assert_eq!(ArchiveFormat::from_extension("document.pdf"), None);
    assert_eq!(ArchiveFormat::from_extension("no_extension"), None);
}

#[test]
fn archive_format_extensions_round_trip() {
    for format in [ArchiveFormat::Zip, ArchiveFormat::TarGz, ArchiveFormat::Tar] {
        let name = format!("test.{}", format.extension());
        assert_eq!(ArchiveFormat::from_extension(&name), Some(format));
    }
}

#[test]
fn conflict_choices_map_to_the_shared_transfer_policy() {
    let mut remaining = vec!["notes.txt", "photo.jpg"];
    assert_eq!(
        resolve_conflict_choice(
            ConflictChoice::KeepBoth,
            false,
            "report.txt",
            &mut remaining
        ),
        vec![("report.txt", TransferConflict::KeepBoth)]
    );
    assert_eq!(remaining, vec!["notes.txt", "photo.jpg"]);

    assert_eq!(
        resolve_conflict_choice(ConflictChoice::KeepBoth, true, "report.txt", &mut remaining),
        vec![
            ("report.txt", TransferConflict::KeepBoth),
            ("notes.txt", TransferConflict::KeepBoth),
            ("photo.jpg", TransferConflict::KeepBoth),
        ]
    );
    assert!(remaining.is_empty());
    remaining = vec!["notes.txt", "photo.jpg"];

    assert_eq!(
        resolve_conflict_choice(ConflictChoice::Replace, true, "report.txt", &mut remaining),
        vec![
            ("report.txt", TransferConflict::ReplaceExisting),
            ("notes.txt", TransferConflict::ReplaceExisting),
            ("photo.jpg", TransferConflict::ReplaceExisting),
        ]
    );
    assert!(remaining.is_empty());
}

#[test]
fn skipping_a_conflict_can_drop_only_the_current_item_or_the_rest() {
    let mut remaining = vec!["notes.txt", "photo.jpg"];
    assert!(
        resolve_conflict_choice(ConflictChoice::Skip, false, "report.txt", &mut remaining)
            .is_empty()
    );
    assert_eq!(remaining, vec!["notes.txt", "photo.jpg"]);

    assert!(
        resolve_conflict_choice(ConflictChoice::Skip, true, "report.txt", &mut remaining)
            .is_empty()
    );
    assert!(remaining.is_empty());
}
