# QA — file-operations

Scope: `strata-qa-file-operations` only.
Product: `wmfeht/strata` @ `66fb0e6cb346b2dc127438a50bea48834aaf219f` (v0.15.0).
Agent: `bc-f2c7dcea-3042-47a4-8ea0-6d74671f7a60`.

Skills: Origin `wmfeht/strata-skills` (`COMMON.md`, `strata-qa-file-operations/SKILL.md`) was not readable from this VM (`origin` CLI unauthenticated; `https://origin.cursor.com/wmfeht/strata-skills.git` exists but rejects the GitHub token; GitHub has no `strata-skills` repo). This report uses the COMMON exploratory-QA layout reconstructed from sibling sweep reports plus the product contract in `docs/keyboard-navigation.md` (paste destinations, creating files/folders, footer), `tests/e2e/scenarios/test_clipboard.py`, `test_copy_conflicts.py`, `test_inline_renaming.py`, `test_rename_visibility.py`, `test_entry_management.py`, and `test_drag_and_drop.py`.

No product fixes. No GitHub issues filed. No product PR comments.

## Verdict

`pass-with-nits`

Copy / cut / paste (keyboard and context menu), same-folder numbered duplicates, paste-into-selected-folder vs current-directory targeting, conflict Skip / Replace / Keep Both / Apply to all, New File / New Folder (including Escape keeping the allocated default), F2 / Ctrl+R / context-menu rename, invalid-name rejection, same-volume drag-move, and Ctrl+Z undo of copy and move all hold on this SHA under isolated HOME/XDG. No product non-nit was reproduced. Nits are documentation drift on the paste-availability footer wording and README’s narrower undo description.

## Environment

| Item | Value |
| --- | --- |
| SHA | `66fb0e6cb346b2dc127438a50bea48834aaf219f` |
| Version | 0.15.0 |
| Host | Ubuntu 24.04.4 LTS, GTK 4.14.5 |
| Display | Private Xvfb via `e2e-native.sh` `HeadlessDisplay` (never `DISPLAY=:1`) |
| Session | Private D-Bus + throwaway `HOME` / XDG (`strata-e2e-home-*`) |
| AT-SPI | Enabled (`python3-gi` / `gir1.2-atspi-2.0`) |
| Binary | `/workspace/target/debug/strata` rebuilt at this SHA (`cargo build --bin strata`, 24.35s) |
| Runner | `STRATA_BINARY=… ./scripts/e2e-native.sh` (unsets `DISPLAY` / `WAYLAND_DISPLAY`) |

## Coverage

| Case | Result | Notes |
| --- | --- | --- |
| Native E2E file-ops surface | pass | **285 passed** / 6 deselected (`not trash and not permanent_delete`) in 641.42s, 2 workers |
| `test_clipboard.py` | pass | 36: same-folder numbered copy, Ctrl+D, copy leaves source, paste into selected folder, parent-dir paste, load-cursor paste, Columns pointer-column paste, cut-then-paste, context-menu copy/cut/paste, conflict Skip/Replace, Paste sensitivity, directory copy |
| `test_copy_conflicts.py` | pass | 13: Keep Both (pointer + Return) selects `todo (2).txt` and Ctrl+Z undoes; Cancel/Escape/Close on copy and move; mixed-paste Apply to all |
| `test_inline_renaming.py` | pass | 138: long-name caret, immediate create, collision numbering, click-away commit, invalid/Escape retain original, filter clear, stem vs folder selection — Columns / Icons / List |
| `test_rename_visibility.py` | pass | 42: committed rename stays visible (including Airy); already-visible rename/create preserve scroll; click-away respects navigation |
| `test_entry_management.py` (create/rename/undo) | pass | 34: Ctrl+Shift+N, invalid new-file correction, inside-field click, name-conflict dialog, empty-dir create, F2/Ctrl+R, location-bar isolation, context-menu rename, undo completed move |
| `test_drag_and_drop.py` | pass | 22: drop-on-folder move, self-drop no-op, folder-into-self no-op, cancel outside window, background no-op, folder contents, multi-select, leftover/padding/airy hits, peek cancel |
| Exploratory: footer after Ctrl+C | pass | Label is **Files on clipboard**, not “Ctrl+V · Paste available” |
| Exploratory: Ctrl+A during New Folder | pass | Replaces editor text only; other entries stay unselected; commits `only-this` |
| Exploratory: F1 then Ctrl+V | pass | No duplicate while the reference is open; paste works after Escape |
| Exploratory: cut → paste → footer | pass | Hint appears on cut; disappears after the move completes |
| Exploratory: unicode file + hidden rename | pass | On-disk `café notes.txt` renamed to `.hidden-cafe`; contents preserved; Ctrl+H reveals it. XTEST cannot type `é` (layout) |
| Exploratory: New File ` my notes.txt` | pass | Leading space kept exactly |

Deselected on purpose (trash scope): `test_delete_moves_the_entry_to_trash`, `test_permanent_delete_*` (3). Undo of a completed **move** was kept here.

Skipped: two-window copy/paste; foreign-app `text/uri-list` clipboard; cross-volume drop (`test_cross_volume_drop.py`, `always-ask`); pinned-container `./scripts/e2e.sh` (native harness already ran the same scenarios); Trash / archives / pointer-selection leftover claims / keyboard grid motion.

## What works

- Ctrl+C / Ctrl+X do not change the tree until Ctrl+V. Copy leaves the source; cut removes it only after a successful paste.
- Same-folder paste or Ctrl+D allocates `name (1).ext` / `folder (1)` without overwriting.
- Paste targets a single selected directory, otherwise the current directory. A Columns load-cursor folder is not an implicit paste target until it is explicitly selected. Opening a child keeps paste under the pointer column.
- Context-menu Copy / Cut / Paste match the keyboard paths. Paste is shown but insensitive while the file clipboard is empty.
- Name conflicts open **File already exists**. Skip leaves the destination; Replace overwrites; Keep Both picks the next free number and selects it; Apply this choice to all remaining conflicts covers a mixed paste. Move conflicts omit Keep Both. Cancel / Escape / Close leave both files untouched.
- Ctrl+Z undoes a completed copy (Keep Both) and a completed cut-paste.
- Ctrl+Shift+N / New Folder and background New File create `new folder` / `new file` (or the next free number) immediately, select the whole default name, and keep that default on Escape or invalid input. `/` in a new name is rejected and can be corrected with F2. Click-inside the editor continues editing; click-away or Enter commits a valid name, including surrounding spaces.
- F2 and Ctrl+R rename; a second Ctrl+R while editing is ignored. Those shortcuts do not steal the location bar. Context-menu Rename works. Conflicting names show **Unable to rename item** and keep the original listing.
- Same-volume drag onto a folder moves the file or the whole multi-selection. Self-drop, folder-into-self, pane-background drop, and release outside the window change nothing.
- After copy or cut, the footer shows **Files on clipboard**. A completed cut clears it. F1’s shortcut reference blocks Ctrl+V until it is closed.

## Findings

### Product non-nits

None. The documented copy / cut / paste / duplicate / conflict / create / rename / same-volume drag / undo paths matched the isolated runs.

### Nits / process

#### 1 — NIT — paste-availability footer wording drifted from the docs

Area: `src/ui/shortcut_footer.rs` vs `docs/keyboard-navigation.md`.
Related: footer / keybinding hints (not a paste functional break).
Repro:

1. Isolated HOME. Select `todo.txt`, press Ctrl+C.

Result: footer chip reads **Files on clipboard**. AT-SPI has no “Paste available” label.
Expected per `docs/keyboard-navigation.md` § Shortcut footer: a highlighted **Ctrl+V · Paste available** hint. `docs/screenshots/291/paste-available.png` still shows that older string.
Evidence: `/opt/cursor/artifacts/copy_footer_files_on_clipboard.png`.

#### 2 — NIT — README undo sentence is narrower than the product

Area: `README.md` Features / shortcuts vs footer + E2E.
Related: docs only.
Repro: Keep Both a colliding copy, then Ctrl+Z (canonical `test_keep_both_selects_the_numbered_copy_and_undo_preserves_originals`).
Result: the numbered copy is removed; sources stay. Footer reference: **Ctrl+Z · Undo the last file operation**.
Expected per README: “Ctrl+Z to undo the latest move or move to Trash” — that sentence omits copy undo, which the product and E2E already do.

## Gaps

- Origin `strata-qa-file-operations` checklist was not available; cases follow the E2E file-ops modules and `docs/keyboard-navigation.md` create/paste/footer sections.
- No second window, so inter-window paste ownership was not driven.
- No paste of a file list copied from another GTK app.
- No cross-volume drag (owned in part by drop-strategy preference; not run).
- Unicode characters cannot be typed through this Xvfb XTEST map; unicode was exercised by creating `café notes.txt` on disk and renaming it.
- Pinned-container `./scripts/e2e.sh` was not repeated; native isolation used the same pytest scenarios.

## Evidence

| File | What |
| --- | --- |
| `/opt/cursor/artifacts/e2e_file_operations.log` | Native E2E: 285 passed in 641.42s |
| `/opt/cursor/artifacts/e2e_file_operations_collect.log` | 285/291 collected (6 trash/permanent-delete deselected) |
| `/opt/cursor/artifacts/e2e_file_operations_exploratory.log` | Footer / Ctrl+A / F1 / cut-footer (4 passed; unicode typing failed on XTEST) |
| `/opt/cursor/artifacts/e2e_file_operations_exploratory2.log` | Unicode-on-disk rename + spaced New File (2 passed) |
| `/opt/cursor/artifacts/copy_footer_files_on_clipboard.png` | After Ctrl+C: **Files on clipboard** |
| `/opt/cursor/artifacts/ctrl_a_during_new_folder.png` | New Folder editor after Ctrl+A + type `only-this` |
| `/opt/cursor/artifacts/f1_blocks_file_shortcuts.png` | Shortcut reference open (Ctrl+V blocked) |
| `/opt/cursor/artifacts/cut_paste_footer_cleared.png` | After cut-paste into `archive`: hint gone, `todo.txt` moved |
| `/opt/cursor/artifacts/unicode_hidden_rename.png` | `.hidden-cafe` visible after Ctrl+H |
| `/opt/cursor/artifacts/spaced_new_file_name.png` | New File committed as ` my notes.txt` |
