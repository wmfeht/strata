// SPDX-License-Identifier: GPL-3.0-or-later

use super::{archive_has_collision, normalized_archive_name};
use crate::model::Location;
use crate::services::ArchiveFormat;

/// The selected format's suffix is stripped so the dialog does not double the extension.
#[test]
fn names_strip_matching_extension() {
    assert_eq!(
        normalized_archive_name("backup.zip", ArchiveFormat::Zip),
        "backup"
    );
    assert_eq!(
        normalized_archive_name("backup.ZIP", ArchiveFormat::Zip),
        "backup"
    );
    assert_eq!(
        normalized_archive_name("backupzip", ArchiveFormat::Zip),
        "backupzip"
    );
    assert_eq!(
        normalized_archive_name("backup.tar.gz", ArchiveFormat::TarGz),
        "backup"
    );
}

/// Non-ASCII names must not be sliced at a non-char boundary, and a matching
/// suffix after CJK still strips.
#[test]
fn names_preserve_non_ascii_without_suffix() {
    assert_eq!(
        normalized_archive_name("日本語", ArchiveFormat::Zip),
        "日本語"
    );
    assert_eq!(
        normalized_archive_name("📦archive", ArchiveFormat::Zip),
        "📦archive"
    );
    assert_eq!(
        normalized_archive_name("日本語.zip", ArchiveFormat::Zip),
        "日本語"
    );
    assert_eq!(normalized_archive_name("📦.ZIP", ArchiveFormat::Zip), "📦");
}

/// Collision checks look at the final filename, including the format extension.
#[test]
fn collisions_use_final_name() -> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let destination = Location::local(root.path());
    assert!(!archive_has_collision(&destination, "archive.zip"));
    std::fs::write(root.path().join("archive.zip"), b"existing")?;
    assert!(archive_has_collision(&destination, "archive.zip"));
    Ok(())
}
