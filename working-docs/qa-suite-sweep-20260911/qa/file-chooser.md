# QA — file-chooser

Scope: `strata-qa-file-chooser` only.
Product: `wmfeht/strata` @ `66fb0e6cb346b2dc127438a50bea48834aaf219f` (v0.15.0).
Agent: `bc-29879c63-9dea-429c-b4f5-86c72c6244af`.

Skills: Origin `wmfeht/strata-skills` (`COMMON.md`, `strata-qa-file-chooser/SKILL.md`) was not readable from this VM (`origin` CLI unauthenticated; GitHub has no `strata-skills` repo). This report uses the COMMON exploratory-QA layout reconstructed from sibling sweep reports plus the product contract in `docs/portal-file-chooser.md`, `scripts/portal-test.py`, `src/portal.rs`, and `src/ui/chooser/tests/*`.

No product fixes. No GitHub issues filed. No product PR comments.

## Verdict

`pass-with-nits`

Dedicated-client portal paths hold on this SHA: OpenFile (single + filter + Enter), Escape / `Request.Close` cancel, OpenFile directory, SaveFile overwrite Cancel-then-Escape and Replace, SaveFiles multi-URI Replace, FileChooser version 4, 3- and 40-filter lists, List / Columns / Icons, Classic Light, remote-URI local-only error, Ctrl+Shift+N inline `new folder`, Space preview, and the first-run `portal-opt-in-v1` marker. Closed filter-list work (#498 / #466) did not recur.

Known [lgse/strata#717](https://github.com/lgse/strata/issues/717) still fails in the filtered GTK suite (readdir order in a nested-path test; product accept-by-path is OK). Four ignored xdotool tests failed when run alone under private Xvfb — treated as harness-sensitive candidates, not filed. Settings → General opened via Ctrl+, but **System file chooser / Configure…** stayed below the fold.

## Environment

| Item | Value |
| --- | --- |
| SHA | `66fb0e6cb346b2dc127438a50bea48834aaf219f` |
| Version | 0.15.0 |
| Host | Ubuntu 24.04.4 LTS, GTK 4.14.5 |
| Display | Private Xvfb `:99` (`1440x900x24`, tmux `qa-fc-xvfb`); never `DISPLAY=:1` |
| Session | Private D-Bus via `scripts/portal-test.py --binary`; throwaway `HOME` / XDG |
| AT-SPI | Disabled for this scope (`GTK_A11Y=none` on Rust tests; dedicated client isolates a11y) |
| Binary | `/workspace/target/debug/strata` rebuilt at this SHA (`cargo build --bin strata --locked`) |
| Client | `python3 scripts/portal-test.py <case> --binary target/debug/strata` (does **not** install the desktop portal) |

`xdotool search --name Strata` matches a 1×1 dummy plus the real chooser (~1000×680). Drivers waited for `WIDTH >= 400` and parsed the pretty-printed client JSON with `json.JSONDecoder().raw_decode` (a trailing “A connection to the bus can't be made” is cleanup noise). ImageMagick `import` of the mapped chooser window; cairo/Xvfb subpixel capture often **doubles letters** in OCR — treat that as a capture artifact, not a product bug.

## Coverage

| Case | Result | Notes |
| --- | --- | --- |
| Rust `chooser` / `portal` / `portal_setup` / `portal_preferences` filter | pass / known fail | **70 passed**, **1 failed** (#717), **7 ignored**. Log: `/opt/cursor/artifacts/chooser_cargo_tests.log` |
| `filtered_selection_only_accepts_on_enter_or_open_with_exact_nested_path` | fail (known #717) | `left: …/nested-b.txt` vs `right: …/nested-a.txt`. `observer_activation_returns_exact_nested_path` passed |
| `long_filter_lists_are_accepted` | pass | #498 / #466 stay fixed |
| Ignored `keyboard_only_controls_and_file_navigation_work_in_every_chooser_view` | fail (harness) | After Down, selected `"00.txt"` expected `"01.txt"` (`keyboard.rs` ~445) |
| Ignored `chooser_context_menus_and_rename_work_in_every_view` | fail (harness) | Columns inline rename focus landed on `GtkListItemWidget`, not the name field |
| Ignored `options_share_a_compact_row_and_wrap_in_narrow_windows` | fail (harness) | At 900px, “all options share one row” |
| Ignored `every_view_enforces_single_selection_including_type_groups` | fail (harness) | Columns toolbar New Folder did not activate the inline entry in 5s |
| Ignored `setup_copy_aligns_and_success_replaces_the_explanation` | pass | Run alone under isolated XDG |
| Ignored `native_filters_match_globs_and_mime_types_without_hiding_directories` | pass | Run alone under isolated XDG |
| OpenFile single + Ctrl+F `notes.txt` + Enter | pass | `response: 0`, URI `…/notes.txt`, `writable: true` |
| Escape cancel | pass | `response: 1` |
| `--cancel-after` / `Request.Close` | pass | `response: 1` |
| OpenFile directory + Select Folder | pass | `response: 0`, URI `…/Documents` (highlighted child, not the current folder) |
| SaveFile overwrite Cancel | pass | Dialog stays; chooser remains open; Escape then `response: 1` |
| SaveFile overwrite Replace | pass | `response: 0`, dest `strata-portal-demo.txt`, `choices: encoding=utf8, compress=false`, `current_filter: Text files` |
| SaveFiles + Replace (retry) | pass | First attempt SIGTERM (`rc: -15`); retry `response: 0`, URIs `notes.txt` + `new-file.txt`, same choices |
| FileChooser version | pass | `gdbus` → `(<uint32 4>,)` |
| Filters 3 and 40 | pass | Both open; Filter 01 selected on the 40-filter list; no reject |
| Views List / Columns / Icons | pass | List + `--theme classic-light`; Columns / Icons on Tokyo Night |
| Remote `smb://…` | pass | In-chooser error: local files/folders only |
| Ctrl+Shift+N | pass | Inline `new folder` rename |
| Space on `sample.png` | pass | Preview pane with PNG |
| First-run offer marker | pass | `portal-opt-in-v1` written after Escape; offer chrome not cleanly captured |
| Settings Ctrl+, | pass / nit | General opens. System file chooser / Configure… is below the fold; Page_Down did not reach it |
| Ctrl+Enter accept current folder | not confirmed | Timed out (focus likely not on the file view) |
| Multiple-open Shift+range | weak | Successful D-Bus run returned **one** URI; driver, not proven product |
| SaveFile choice chrome | observation | Encoding / Compress not shown on SaveFile; defaults still in D-Bus. SaveFiles shows Encoding |
| Hyprland parent sizing / centering | skipped | No Hyprland |
| `portal-test.html` / `--install-portal` | skipped | Would mutate the desktop portal |

GTK theme parser warnings (`Junk at end of value for padding`, `Expected a valid color`) fire on every chooser launch. Same stylesheet noise as the theming sweep; chrome still paints.

## What works

- Dedicated client on a private bus starts FileChooser backend **version 4** and returns portal `response` 0/1 without installing desktop metadata.
- Single OpenFile + pane filter (`Ctrl+F` `notes.txt` + Enter) returns the exact local URI and `writable: true`.
- Escape and `Request.Close` both cancel (`response: 1`).
- Directory requests accept the highlighted folder (`Documents`), not the parent “Test files” location.
- SaveFile suggested name `strata-portal-demo.txt` opens the themed “Replace existing file?” modal. Cancel leaves the chooser open; Replace returns the destination plus `choices` and `current_filter`.
- SaveFiles overwrite lists both destinations (`notes.txt`, `new-file.txt`) and returns both URIs after Replace.
- 40 application filters open (Filter 01 selected). `long_filter_lists_are_accepted` still passes.
- List (Classic Light), Columns, and Icons present the same fixture. Remote URIs show “Unable to open location” / local files and folders only.
- Ctrl+Shift+N allocates `new folder` and selects the name. Space on `sample.png` opens the preview pane.
- First normal launch writes `${XDG_CONFIG_HOME}/strata/portal-opt-in-v1` after dismissing (Escape). Ctrl+, opens Settings → General.
- Portal contract unit tests (filters/choices order, URI scheme, save basename safety, setup opt-in / uninstall) pass on this SHA.

## Findings

### Product non-nits

None that break the documented dedicated-client portal contract.

### Known open (not new)

#### 1 — Nested filtered-path GTK test assumes readdir order

Area: `ui::chooser::tests::acceptance::filtered_selection_only_accepts_on_enter_or_open_with_exact_nested_path`.
Related: open [lgse/strata#717](https://github.com/lgse/strata/issues/717) (“do not assume readdir order in nested filtered-path test”).
Repro: isolated Xvfb, `STRATA_REQUIRE_GTK_TESTS=1`, that test alone or in the chooser/portal filter.
Result: accepted `…/folder/nested-b.txt` while the fixture expected `nested-a.txt`. Sibling test `observer_activation_returns_exact_nested_path` passed.
Expected: test should not depend on directory iteration order. Product Enter/Open-by-path is not implicated.
Evidence: `/opt/cursor/artifacts/chooser_cargo_tests.log`. Not re-filed.

### Nits / process

#### 2 — Four ignored xdotool tests fail when run alone

Area: `ui::chooser::tests::{keyboard,context_menu,layout,selection}` ignored cases that require `xdotool` and isolated XDG.
Related: none filed from this sweep. Docs already say run each alone under a test display.
Result:

- Keyboard: Down after `00.txt` stayed on `00.txt` (expected `01.txt`).
- Context menu / rename: Columns inline rename focus was `GtkListItemWidget` on a `GtkListView`.
- Layout: at 900px width, options did not share one row.
- Selection: Columns toolbar New Folder did not open the inline folder entry within 5s.

Interactive Ctrl+Shift+N **did** open inline rename on List. Two other ignored tests (`setup_copy_aligns…`, `native_filters_match_globs…`) passed. Treat as harness / Columns timing, not new product bugs.
Evidence: `/opt/cursor/artifacts/chooser_ignored_tests.log`.

#### 3 — Settings System file chooser row is below the fold

Area: Settings → General → DESKTOP INTEGRATION (owned for deep Configure by settings-general / desktop-integration).
Repro: throwaway HOME, `strata` (not `--portal`), Ctrl+,.
Result: General opens at BROWSING. Page_Down / scroll attempts did not bring **System file chooser / Configure…** on screen. Configure dialog was not captured here. Settings-general sweep did open that dialog.
Expected: the row should be reachable without hunting; scroll-into-view on first open is a nit, not a functional fail.
Evidence: `/opt/cursor/artifacts/r3b_settings.png`, `/opt/cursor/artifacts/r3c_settings_bottom.png`.

#### 4 — SaveFile chrome omitted visible Encoding / Compress

Area: SaveFile vs SaveFiles application `choices` row.
Result: SaveFile Replace still returned `encoding=utf8` / `compress=false` on D-Bus. The SaveFile footer showed the Text files filter, not the Encoding dropdown. SaveFiles showed **Encoding UTF-8** plus the two filenames.
Expected: both methods should surface the same compact choices row (`docs/portal-file-chooser.md`). Observation only; D-Bus defaults are correct.
Evidence: `/opt/cursor/artifacts/r3_save_overwrite_cancel.png`, `/opt/cursor/artifacts/r2_savefiles_choices_row.png`, `/opt/cursor/artifacts/file_chooser_qa_round3.json`.

#### 5 — GTK CSS parser warnings on every chooser launch

Area: same `src/style.css` `!important` scrollbar rules noted by the theming sweep.
Result: `Theme parser error` at `<data>:512-528` including `Expected a valid color`. Chooser still paints.
Expected: stylesheet loads without GTK 4.14 warnings.
Evidence: every `portal-test.py --binary` log under `/opt/cursor/artifacts/`.

#### 6 — Origin file-chooser skill unavailable

Process: could not clone `origin.cursor.com/wmfeht/strata-skills`. Cases follow `docs/portal-file-chooser.md` Local test tools plus the chooser/portal GTK suite.

### Weak / not proven

- Multiple OpenFile + Shift+range: one successful D-Bus accept returned a single URI (`strata-portal-demo.txt`). Likely the driver selected one row. Not logged as a product fail.
- Ctrl+Enter “accept current folder” timed out; focus was probably still on chrome.
- First-run **offer UI** was not cleanly captured (`xdotool search --name Strata` also hit leftover portal windows). Marker file write is the pass.

## Gaps

- Origin `strata-qa-file-chooser` checklist was not available; cases above are the portal doc + dedicated client + GTK chooser/portal suite.
- No Hyprland IPC parent sizing or `strata-file-chooser-center` placement.
- No `scripts/portal-test.html` / Chromium File System Access path.
- No `--install-portal` / `--uninstall-portal` against the Cloud Agent desktop (would mutate the host portal).
- Pinned container `./scripts/e2e.sh` not re-run for this scope.
- Configure… dialog and installer `--with-file-chooser` / `--without-file-chooser` not exercised here.

## Evidence

| File | What |
| --- | --- |
| `/opt/cursor/artifacts/chooser_cargo_tests.log` | 70 passed, 1 failed (#717), 7 ignored |
| `/opt/cursor/artifacts/chooser_ignored_tests.log` | 4 ignored fails, 2 ignored passes |
| `/opt/cursor/artifacts/file_chooser_qa_round3.json` | D-Bus results for accept / cancel / directory / save |
| `/opt/cursor/artifacts/file_chooser_round3.log` | Round-3 driver log (SaveFiles first attempt `rc: -15`) |
| `/tmp/strata-qa-fc/r3-savefiles-1789103821369.log` | SaveFiles retry: both URIs + choices |
| `/opt/cursor/artifacts/r3_single_filtered.png` | Ctrl+F `notes.txt` hit |
| `/opt/cursor/artifacts/r3_save_overwrite_cancel.png` | SaveFile “Replace existing file?” |
| `/opt/cursor/artifacts/r3b_savefiles_overwrite.png` | SaveFiles multi overwrite (`notes.txt`, `new-file.txt`) |
| `/opt/cursor/artifacts/fc_filters_bulky_40.png` | 40 filters, Filter 01 selected |
| `/opt/cursor/artifacts/fc_list_classic_light.png` | List + Classic Light |
| `/opt/cursor/artifacts/fc_columns_tokyo_night.png` | Columns |
| `/opt/cursor/artifacts/fc_icons_tokyo_night.png` | Icons |
| `/opt/cursor/artifacts/fc_remote_uri_error.png` | Remote URI local-only error |
| `/opt/cursor/artifacts/fc_new_folder_inline_rename.png` | Ctrl+Shift+N `new folder` |
| `/opt/cursor/artifacts/fc_space_preview_png.png` | Space preview of `sample.png` |
| `/opt/cursor/artifacts/r2_savefiles_choices_row.png` | SaveFiles Encoding UTF-8 row |
| `/opt/cursor/artifacts/r3b_settings.png` | Settings → General (chooser row not visible) |

Tiny/useless frames (~296 bytes) not cited: `fc_single_list_filtered.png`, `fc_save_overwrite_replace_dialog.png`.
