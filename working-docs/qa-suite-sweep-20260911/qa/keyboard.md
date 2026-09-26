# QA — keyboard

Scope: `strata-qa-keyboard` only.
Product: `wmfeht/strata` @ `66fb0e6cb346b2dc127438a50bea48834aaf219f` (v0.15.0).
Agent: `bc-67187367-d10a-498a-a23e-7646f4a76a57`.

Skills: Origin `wmfeht/strata-skills` (`COMMON.md`, `strata-qa-keyboard/SKILL.md`) was not readable from this VM (`origin` CLI unauthenticated; GitHub `wmfeht/strata-skills` is an empty repo). This report uses the COMMON exploratory-QA layout reconstructed from sibling sweep reports plus the product keyboard contract in `src/ui/window/keyboard/`, `src/ui/shortcut_footer.rs`, `tests/e2e/scenarios/test_keyboard_navigation.py`, `test_escape_selection.py`, and `test_inline_renaming.py`.

No product fixes. No GitHub issues filed. No product PR comments.

## Verdict

`pass`

File-view keyboard movement, activation, Shift-arrow range selection, Escape, rename, view-switch shortcuts, clipboard, filter/search chords, F1 reference, and header/sidebar focus hold on this SHA. Official native E2E for those surfaces passed (317 cases, 0 failed). Rust keyboard dispatch tests passed (21 passed, 1 ignored chooser case). Agent-written exploratory failures were test-author mistakes (filter Down while the query is focused; Escape-after-Ctrl+A assumed the first name stays focused), not product breaks.

## Environment

| Item | Value |
| --- | --- |
| SHA | `66fb0e6cb346b2dc127438a50bea48834aaf219f` |
| Version | 0.15.0 |
| Host | Ubuntu 24.04.4 LTS, GTK 4.14.5 |
| Display | Private Xvfb via `xvfb-run` / `e2e-native.sh` `HeadlessDisplay` (never `DISPLAY=:1`) |
| Session | Private D-Bus + throwaway `HOME` / XDG (`strata-e2e-home-*` / `strata-e2e-fixture-*`) |
| AT-SPI | Enabled for E2E (`python3-gi` / `gir1.2-atspi-2.0`); Rust unit tests used `GTK_A11Y=none` |
| Binary | `/workspace/target/debug/strata` rebuilt at this SHA |
| Native screenshots | C.UTF-8 host-font glyph noise; AT-SPI names used for assertions |

## Coverage

| Case | Result | Notes |
| --- | --- | --- |
| Rust `cargo test … keyboard` (private Xvfb) | pass | **21 passed**, 1 ignored (`ui::chooser::tests::keyboard::…`, needs xdotool; chooser is another scope) |
| `ctrl_a_during_rename_selects_only_unicode_entry_text…` | pass | Ctrl+A stays in the rename field in every view |
| `shift_after_escape_starts_on_the_focused_entry` | pass | First Shift after Escape starts on the cursor |
| `filter_clipboard_proceeds_and_escape_dismisses_one_surface…` | pass | Escape pops one overlay at a time |
| `clipboard_and_delete_shortcuts_proceed_inside_preview_text` | pass | Preview text does not steal file-view clipboard/delete |
| `parked_pointer_cannot_override_keyboard_navigation` | pass | Keyboard target wins over a parked pointer |
| E2E `test_keyboard_navigation.py` | pass | **20/20**: arrows + hjkl, Enter, Alt+Up/history, Shift+arrows, Ctrl+A, view switch, keyboard copy/paste |
| E2E `test_escape_selection.py` | pass | **46/46**: Escape clears without navigating; Enter after Escape opens; Shift after Escape starts on the focused entry |
| E2E `test_inline_renaming.py` | pass | **138/138**: F2/new-item editors, Escape keeps the name, commit/invalid names |
| E2E `test_view_switching.py` | pass | **11/11**: Ctrl+1/2/3 and appearance menu |
| E2E `test_search_filter_sort.py` | pass | **29/29**: `/` and Ctrl+F filter, Ctrl+K search, sort |
| E2E `test_quick_preview.py` | pass | **31/31**: Space preview, including Space on a filtered result without changing the query |
| E2E `test_clipboard.py` | pass | **29/29**: keyboard and menu copy/cut/paste |
| E2E `test_locations.py` | pass | **13/13**: Ctrl+L path, breadcrumb, sidebar place |
| Exploratory: arrows / Enter / Alt+Up | pass | All three views; Columns shot after returning from `documents` |
| Exploratory: hjkl move + Shift+arrows extend | pass | `j`/`l` move; `Shift+↑/↓` extend. Shift+hjkl is not an advertised alias |
| Exploratory: Ctrl+A + Ctrl+1/2/3 | pass | Pane selection survives the view switch |
| Exploratory: keyboard copy/paste | pass | Ctrl+C on `todo.txt`, Enter into `archive`, Ctrl+V |
| Exploratory: F2 / Ctrl+R rename | pass | Ctrl+A stays in the field; Escape keeps the original name |
| Exploratory: Ctrl+Shift+N | pass | Escape keeps `new folder` |
| Exploratory: F1 blocks file shortcuts | pass | Ctrl+C while the reference is open does not copy; F1 still works with hints hidden |
| Exploratory: `/` empty filter | pass | Opens an empty pane filter |
| Exploratory: Ctrl+L + Ctrl+H | pass | Location field and hidden-files toggle |
| Exploratory: Ctrl+B / Ctrl+Shift+B | pass | Sidebar hide/show and focus swap |
| Exploratory: header Up/Down round-trip | pass | Up from the first item reaches the header; Down returns |
| Exploratory: Right on a file in Columns | pass | Does not open a child column |
| Exploratory: Shift from empty | pass | First Shift after Escape selects the cursor; the next Shift extends |
| Exploratory: keyboard folder open | pass | First child only is selected |
| Exploratory: Ctrl+D + Ctrl+Z | pass | Duplicate then undo |
| Exploratory: Alt+Enter | pass | Properties for `readme.md` |
| Exploratory: `y` / `p` | pass | Copy path / pin (type-to-search off) |
| Exploratory: Ctrl+K | pass | Overlay opens; Escape dismisses |
| Exploratory: Home / End / Ctrl+Up / Ctrl+Down | pass | Jump and pane-edge motion |
| Agent filter+Space+Down (query focused) | n/a | Official Space-on-filtered-result case passed. Down while the query is focused does not move the listing (documented type-in-query behavior) |
| Agent Escape after Ctrl+A | n/a | Official Escape cases (no Ctrl+A first) passed. After Ctrl+A, focus is on the last item (`todo.txt`), not `readme.md` |

Skipped: chooser keyboard (other scope; ignored GTK test needs xdotool), pointer leftover Shift (pointer-selection / this SHA #797), Trash Delete (GVfs Trash missing on this VM), Ctrl+T terminal, Menu / Shift+F10 (open #583, accessibility), second window, pinned `./scripts/e2e.sh` container (native harness used the same AT-SPI assertions).

## What works

- Arrows and hjkl move focus and selection in Columns, Icons, and List. Enter opens a folder; Alt+Up / history return; Backspace goes to the parent.
- `Shift+↑ / ↓` extends a range. After Escape, the first Shift selects only the focused entry; the next Shift extends. Ctrl+A selects the pane and survives Ctrl+1/2/3.
- Right on a file in Columns does not open a child. Keyboard-opening a folder selects the first child only.
- F2 and Ctrl+R open inline rename. Ctrl+A selects the field text, not the pane. Escape keeps the original name. Ctrl+Shift+N starts `new folder`; Escape keeps that default.
- Ctrl+C / Ctrl+V copy and paste without the pointer. Ctrl+D duplicates; Ctrl+Z undoes.
- `/` and Ctrl+F open the pane filter. Official Space preview of a filtered result leaves the query unchanged. Ctrl+K opens global search; Escape dismisses one surface at a time.
- Ctrl+L edits the location. Ctrl+H toggles hidden files. Ctrl+B hides the sidebar; Ctrl+Shift+B swaps sidebar/listing focus. Up from the first item reaches the header; Down returns.
- F1 opens the shortcut reference and swallows file-view chords (Ctrl+C does not copy). It still opens when footer hints are hidden.
- Alt+Enter opens Properties. `y` copies the path and `p` pins a folder when type-to-search is off. Home / End / Ctrl+Up / Ctrl+Down jump as documented.
- Parked pointer cannot steal keyboard navigation. Preview text does not eat clipboard/delete. Modal and inline editors own their keys before window shortcuts.

## Findings

### Product non-nits

None that break the documented file-view keyboard contract.

### Nits / process

None that change advertised behavior. `selection_navigation` in `src/ui/window/keyboard/items.rs` handles only `Shift+Up` / `Shift+Down`. The F1 reference lists `Shift+↑ / ↓` for extend and `h / j / k / l` as movement aliases, so Shift+hjkl is not a missing advertised shortcut. An early exploratory case that expected Shift+j/k to extend failed in all three views; the rewritten case that uses Shift+arrows passed.

### Known / adjacent (not new)

- Menu / Shift+F10 still do not open the context menu (open [lgse/strata#583](https://github.com/lgse/strata/issues/583) / accessibility sweep). Not re-filed.
- Closed leftover Shift+click on Columns (#797 on this SHA) is pointer-selection scope.
- Ctrl+K “Partial search — some folders could not be read” on a clean throwaway HOME is recorded by the search-filter sweep, not this scope.
- Native screenshots show C.UTF-8 glyph noise; AT-SPI names were used for pass/fail.

## Gaps

- Origin `strata-qa-keyboard` checklist was not available; cases follow the E2E keyboard / Escape / rename / filter / preview contract plus exploratory chords from the F1 reference.
- Chooser keyboard not exercised (ignored Rust test; other scope).
- Delete-to-Trash and Shift+Delete were not run (no GVfs Trash on this VM; trash sweep owns that surface).
- Ctrl+T (terminal) and a second GUI window were not opened.
- Pinned container `./scripts/e2e.sh` was not re-run; native `e2e-native.sh` used a private Xvfb + D-Bus session.

## Evidence

| File | What |
| --- | --- |
| `/opt/cursor/artifacts/rust_keyboard_tests.log` | 21 passed / 1 ignored `keyboard` Rust filter (private Xvfb) |
| `/opt/cursor/artifacts/e2e_keyboard_qa.log` | Official keyboard / Escape / rename / view-switch: **215 passed**, 0 official failed (native) |
| `/opt/cursor/artifacts/e2e_keyboard_qa_round2.log` | Official filter / locations / preview / clipboard: **102 passed**, 0 official failed; 6 exploratory authoring misses |
| `/opt/cursor/artifacts/qa_keyboard_columns_after_alt_up.png` | Columns after Enter into `documents` and Alt+Up; `archive` focused |
| `/opt/cursor/artifacts/qa_keyboard_f1_reference.png` | F1 shortcut reference over Columns (`todo.txt` selected) |
| `/opt/cursor/artifacts/qa_keyboard_f1_hints_hidden.png` | F1 still opens after footer hints are hidden |
| `/opt/cursor/artifacts/qa_keyboard_filter_todo.png` | `/` / Ctrl+F filter query `todo` showing `todo.txt` |
| `/opt/cursor/artifacts/qa_keyboard_new_folder_editor.png` | Ctrl+Shift+N inline `new folder` editor in List |
| `/opt/cursor/artifacts/qa_keyboard_copy_paste_archive.png` | Keyboard paste of `todo.txt` into `archive` |
| `/opt/cursor/artifacts/qa_keyboard_ctrl_k_search.png` | Ctrl+K overlay on a home-folder marker |
| `/opt/cursor/artifacts/qa_keyboard_alt_enter_properties.png` | Alt+Enter Properties for `readme.md` |
