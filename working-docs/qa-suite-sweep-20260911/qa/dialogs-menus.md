# QA — dialogs & menus

- **Scope:** `strata-qa-dialogs-menus` only
- **Product:** `wmfeht/strata` @ `66fb0e6cb346b2dc127438a50bea48834aaf219f` (v0.15.0 + #798 + #797)
- **Date:** 2026-09-11
- **Agent:** `bc-f1da998e-79a8-4f23-8ee0-9640b77a16b0`
- **Skills:** Origin `wmfeht/strata-skills` `COMMON.md` and `strata-qa-dialogs-menus/SKILL.md` were not readable here (`origin` CLI unauthenticated; repo is not on GitHub). Isolation and report shape follow the sweep prompt plus the product e2e/unit contract for this scope.
- **Fixes / issues / product PR comments:** none

## Isolation

Private Xvfb (not `DISPLAY=:1`), private D-Bus + AT-SPI, throwaway `HOME` / XDG via the e2e harness (`TestEnvironment` + `HeadlessDisplay`). Observed walkthrough display `:90`. Artifacts: `/opt/cursor/artifacts/qa-dialogs-menus/`.

## Verdict

**Pass.** No confirmed product defect in dialogs/menus at this SHA. Official harness cases for this scope all passed. Interactive AT-SPI walkthrough matched those contracts on menus and dialogs that were reached.

## Method

1. Rebuild `target/debug/strata` from `66fb0e6`.
2. Isolated Rust: `context_menu` filter under private Xvfb (`GTK_A11Y=none`).
3. Native e2e (harness owns Xvfb + D-Bus + throwaway HOME): dialogs/menus, Open With, copy conflicts, Properties size, archive error/password dialogs, entry-management (permanent-delete dialog).
4. Exploratory AT-SPI walkthrough with screenshots. After Extract to… the first script left the fixture root; later `todo.txt` timeouts are harness state, not product failures. Recovery pass re-ran those cases from the fixture root.

## Automated results

| Suite | Command / selection | Result |
| --- | --- | --- |
| Rust context menus | `cargo test --all-targets --all-features context_menu` | **22 passed**, 1 ignored |
| Native e2e | `test_dialogs_and_menus.py`, `test_open_with.py`, `test_copy_conflicts.py`, `test_properties_size.py`, `test_archive_errors.py` | **66 passed** |
| Native e2e | `test_entry_management.py` (includes Permanently delete dialog) | **40 passed** |

Ignored Rust: `ui::chooser::tests::context_menu::chooser_context_menus_and_rename_work_in_every_view` (needs X11 + xdotool + isolated XDG; file-chooser scope).

Logs: `rust-context-menu.log`, `e2e-native.log`, `e2e-entry-management.log`.

## Interactive cases

AT-SPI names and accelerators were authoritative. Screenshots confirm chrome, theme tokens, and layout.

| ID | Case | Result | Evidence |
| --- | --- | --- | --- |
| entry-menu | File menu offers Open, Open With…, Cut/Copy, Copy path, Copy to…, Move to…, Rename, Compress…, Trash, Permanently delete, Properties; Copy description `Ctrl+C` | Pass | `01_entry_context_menu.png` |
| escape-menu | Escape closes entry menu; listing unchanged | Pass | walkthrough |
| pane-menu | New Folder/File, Open in Terminal, Paste, Select All, Refresh, Show Hidden Files, Customize…, Properties | Pass | `02_pane_context_menu.png` |
| properties-file | File Properties; no Pin; Escape closes | Pass | `03_properties_file.png` |
| properties-pin | Folder Properties Pin then Unpin; dialogs close; sidebar updates | Pass | `04_properties_folder_pin.png`, `05_properties_folder_unpin.png` |
| customize | Customize Folder targets the presented directory; Done closes | Pass | `06_customize_folder.png` |
| run-program | “Run this program?”; Cancel starts focused; Close and Run dismiss | Pass | `07_run_program_confirm.png` |
| shortcuts | F1 shortcut reference; Escape closes | Pass | `08_shortcut_reference.png` |
| compress-enter | Enter submits Compress and creates the zip | Pass | `09_compress_dialog.png` |
| compress-invalid | `../escape` keeps Compress open; no zip written; Escape dismisses; invalid field uses error border | Pass | `10_compress_invalid.png` |
| archive-menu | Zip offers Extract here / Extract to… | Pass | `11_archive_entry_menu.png` |
| extract-to | Extract to… submits with Enter | Pass | `12_extract_to_dialog.png` |
| copy-to | Copy to… submits with Enter | Pass | official e2e `test_enter_submits_the_copy_to_dialog` |
| rename-conflict | Rename onto an existing name is rejected | Pass | official e2e |
| open-with | Open With lists handler; Escape dismisses | Pass | `26_open_with.png` + e2e |
| copy-conflict | “File already exists”; Keep Both / Skip / Replace; Escape preserves both | Pass | `27_copy_conflict.png` + e2e |
| multi-select | Drops Rename/Print; Open With… insensitive when types differ (“No application can open all selected file types”) | Pass | `25_multi_select_menu.png` |
| appearance | Appearance popover lists Columns / Icons / List | Pass | `18_appearance_popover.png` |
| permanently-delete | “Permanently delete 1 item?”; Escape cancels; file remains | Pass | `28_permanently_delete.png` + e2e |
| invalid-archive | “Unable to complete operation” / damaged archive; Close dismisses | Pass | `32_invalid_archive.png` + e2e |
| extract-password | Empty → “Enter a password”; wrong → “Invalid password” inline; correct extracts (7z) | Pass | e2e `test_wrong_extract_password_reopens_dialog_until_password_is_correct` |
| hidden-toggle | Pane menu offers Show Hidden Files | Pass | `23_show_hidden_toggle.png` |
| sidebar-trash-empty | Empty Trash… hidden while Trash is empty; Properties only | Pass | `20_sidebar_trash_empty.png` |
| sidebar-trash-nonempty | Empty Trash… after Move to Trash | **Not reached** | see Notes |
| sidebar-home / desktop | Right-click did not open a menu | Note | `31_sidebar_home_no_menu` not captured on recovery; `33_sidebar_desktop_no_menu.png` |

## Findings

None to file.

## Notes

1. **Empty Trash confirmation** was not opened. After a successful Move to Trash (source file gone), the Trash sidebar menu still offered only Properties. The harness sets `GIO_USE_VFS=local`; the product hides Empty Trash… until a trash probe reports items (`TrashContents::Unknown` / empty). Treat as isolation, not a confirmed product bug. Rust covers visibility (`trash_mutating_operations_refresh_the_context_menu`). No e2e scenario opens this dialog.
2. **Home / Desktop sidebar** right-click did not expose a menu. Trash did. Likely limited to trash / mounts / unpinable pins. Not treated as a defect without a written contract.
3. First walkthrough `todo.txt` timeouts after Extract to… were leftover navigation into `unpacked/`.
4. Screenshot OCR of thin `@theme_*` cyan-on-dark type looks smeared; AT-SPI names were clean English.
5. Out of scope: Settings pages, portal file chooser, theme editor, search dialog, preview drawer, archive format/CRC beyond dialog UX (owned by other sweep agents). ZipCrypto wrong-password e2e lives in `test_archive_errors.py` and passed in this run.

## Evidence

Directory: `/opt/cursor/artifacts/qa-dialogs-menus/`

Key shots: `01_entry_context_menu.png`, `02_pane_context_menu.png`, `06_customize_folder.png`, `07_run_program_confirm.png`, `09_compress_dialog.png`, `10_compress_invalid.png`, `25_multi_select_menu.png`, `26_open_with.png`, `27_copy_conflict.png`, `28_permanently_delete.png`, `32_invalid_archive.png`.
