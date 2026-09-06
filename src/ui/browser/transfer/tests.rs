// SPDX-License-Identifier: GPL-3.0-or-later

use crate::services::{DropCommit, TransferKind};

#[test]
fn drop_confirmation_presents_copy_move_and_cancel() {
    let source = include_str!("../transfer.rs");
    assert!(source.contains("\"Copy or move?\""));
    assert!(source.contains("\"Copy\""));
    assert!(source.contains("with_label(\"Move\")"));
    assert!(source.contains("layout.cancel"));
    assert!(source.contains("confirm_cross_volume_drop"));
    assert!(source.contains("start_transfer"));
}

#[test]
fn commit_file_drop_routes_copy_move_ask_and_forbidden() {
    let source = include_str!("../transfer.rs");
    let commit_fn = {
        let start = source
            .find("fn commit_file_drop")
            .expect("commit_file_drop");
        &source[start..]
    };
    assert!(commit_fn.contains("DropCommit::Copy => self.start_transfer"));
    assert!(commit_fn.contains("DropCommit::Move => self.start_transfer"));
    assert!(commit_fn.contains("confirm_cross_volume_drop"));
    assert!(commit_fn.contains("DropCommit::Forbidden"));
    assert_eq!(DropCommit::Copy.transfer_kind(), TransferKind::Copy);
    assert_eq!(DropCommit::Move.transfer_kind(), TransferKind::Move);
    assert_eq!(
        DropCommit::Ask {
            default: TransferKind::Copy,
        }
        .transfer_kind(),
        TransferKind::Copy
    );
}
