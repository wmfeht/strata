# QA — browser-modes

Scope: `strata-qa-browser-modes` only.
Product: `wmfeht/strata` @ `66fb0e6cb346b2dc127438a50bea48834aaf219f` (v0.15.0).
Agent: `bc-f2824a51-eb76-4497-bfcb-d4aab5f2ea34`.

Skills: Origin `wmfeht/strata-skills` (`COMMON.md`, `strata-qa-browser-modes/SKILL.md`) was not readable from this VM (`origin` CLI unauthenticated; GitHub has no `strata-skills` repo). This report uses the COMMON exploratory-QA layout reconstructed from `strata-exploratory-qa` plus the product contract in `tests/e2e/scenarios/test_view_switching.py`, `test_popover_scrolling.py`, `src/ui/window.rs` (`build_appearance_menu`), `src/ui/browser_modes.rs`, and `docs/preferences.md`.

No product fixes. No GitHub issues filed. No product PR comments.

## Verdict

`pass-with-nits`

Columns / Icons / List switching (appearance menu and Ctrl+1/2/3), selection/directory/sort survival, preference persistence, restart in List+Airy, density Compact/Airy, List-only type grouping, Hidden files from the appearance row, sort-by-size, thumbnail-size panel, and outside-wheel dismiss of sort/appearance/thumbnail panels all hold on this SHA. Canonical container E2E for this surface passed. The only new nit is the appearance **Hidden files** control’s AT-SPI name concatenating the accelerator (`Hidden files Ctrl + H`). Numpad Ctrl+KP_2 was not reliably injectable via XTEST on this Xvfb (number-row Ctrl+2 works).

## Environment

| Item | Value |
| --- | --- |
| SHA | `66fb0e6cb346b2dc127438a50bea48834aaf219f` |
| Version | 0.15.0 |
| Host | Ubuntu 24.04.4 LTS, GTK 4.14.5 |
| Display | Private Xvfb via `xvfb-run` / pinned `./scripts/e2e.sh` / `e2e-native.sh` `HeadlessDisplay` (never `DISPLAY=:1`) |
| Session | Private D-Bus + throwaway `HOME` / XDG (`strata-e2e-home-*`) |
| AT-SPI | Enabled for E2E (`python3-gi` / `gir1.2-atspi-2.0`); Rust unit tests used `GTK_A11Y=none` |
| Binary | `/workspace/target/debug/strata` rebuilt at this SHA |
| Docker | Host-network wrapper for `./scripts/e2e.sh` (no default bridge on this VM) |

## Coverage

| Case | Result | Notes |
| --- | --- | --- |
| Rust `cargo test … browser_mode` | pass | **42 passed**, 1 ignored (`skeletons::capture_comparison`, needs `STRATA_SKELETON_VISUALS`) |
| `live_preferences_reach_existing_and_future_browsers…` | pass | Mode, density, grouping, click activation update two in-process browsers; chooser keeps peek policy |
| `saved_browser_preferences_apply_without_settings…` | pass | Saved List+Airy+grouping apply before Settings; survive view rebuilds |
| `type_grouping_is_list_only` | pass | Icons clustering remains gated off |
| E2E `test_view_switching.py` + `test_popover_scrolling.py` | pass | **33 passed** in pinned container (`./scripts/e2e.sh`) |
| Appearance menu switches Icons / List | pass | Lands on the same directory; `documents` still listed |
| Ctrl+1 / Ctrl+2 / Ctrl+3 | pass | Selection (`todo.txt`) survives each switch; round-trip returns to Columns |
| Switch preserves directory, selection, sort | pass | Columns `pictures` descending + `diagram.txt` → List keeps all three |
| Chosen view written to `settings.toml` | pass | `browser_mode = "list"` after appearance switch |
| Appearance checkmarks after shortcut | pass | Columns / Icons / List; Group by file type sensitive only in List |
| Outside wheel dismiss (sort / appearance / thumbnail) | pass | Listing and sidebar close the panel; inside-panel wheel does not; Columns wheel only moves the pointed column |
| E2E visual baselines + sort (`-k` subset) | pass | **29 passed**: columns / icons / icons-airy / list views plus sort-field / reverse-direction |
| Exploratory: default Columns + Compact checked | pass | AT-SPI check images + screenshot |
| Exploratory: Ctrl+2 Icons, selection kept | pass | `todo.txt` remains selected |
| Exploratory: Airy density persists | pass | `browser_density = "airy"` while still Icons |
| Exploratory: Icons thumbnail-size panel | pass | Header **Thumbnail size** opens Small/Medium/Large |
| Exploratory: Ctrl+3 List, selection kept | pass | List checked in appearance menu |
| Exploratory: Group by file type (List) | pass | `group_by_type = true`; headings Folder / Markdown document / Todo.txt file |
| Exploratory: Hidden files from appearance | pass | `.hidden.txt` appears; AT-SPI name is `Hidden files Ctrl + H` (nit) |
| Exploratory: Ctrl+1 round-trip | pass | `browser_mode = "columns"` after return |
| Exploratory: sort by Size | pass | Files reorder to `todo.txt`, `.hidden.txt`, `readme.md` after folders |
| Exploratory: restart List + Airy | pass | Same XDG; window comes back List, `browser_density = "airy"`, grouping + hidden still on |
| Numpad Ctrl+KP_2 | gap | XTEST `0xFFB2` did not change the view; number-row Ctrl+2 works. Not treated as a product fail |

Skipped: pointer leftover Shift/Ctrl (pointer-selection), deep keyboard grid motion (keyboard), pane filter / global search (search-filter), click-activation Settings page (settings-general), Trash / archives / previews. Native screenshots use C.UTF-8 and show host-font glyph noise; container visual baselines are the rendering gate.

## What works

- Default presentation is Columns. Ctrl+1/2/3 and the Appearance menu switch Icons and List. The menu checkmark follows the keyboard, and the header icon updates.
- Switching keeps the current directory, the current selection, and the pane sort (including a Columns-column descending sort carried into List).
- `browser_mode` and `browser_density` persist immediately and survive process restart. Saved List+Airy+grouping apply without opening Settings.
- Density Compact / Airy are exclusive checkmarks. Airy writes `"airy"` and is still List/Icons after restart.
- **Group by file type** is insensitive in Columns and Icons, sensitive in List, and draws type headings (Folder, Markdown document, Todo.txt file).
- Hidden files from the appearance row (and Ctrl+H) reveal `.hidden.txt`.
- Sort popover offers Name / Size / Modified / Type / Folders first. Size sort reorders files while folders stay first.
- Icons expose a thumbnail-size popover. Outside wheel ticks dismiss sort, appearance, and thumbnail panels and only scroll the listing under the pointer (#590 contract).
- Two in-process browsers share live mode/density/grouping/click-activation; the chooser still refuses folder peeking.

## Findings

### Product non-nits

None that break documented Columns / Icons / List, density, grouping, sort, or appearance-panel behavior.

### Nits / process

#### 1 — NIT — appearance **Hidden files** AT-SPI name includes the accelerator

Area: Appearance popover (`build_appearance_menu` / `appearance_option_with_shortcut` in `src/ui/window.rs`).
Related: accessibility sweep nits on concatenated accessible names (F1 toggle). Not re-filed.
Repro:

1. Isolated HOME/XDG. Open Appearance.
2. Read the button that toggles hidden files.

Result: accessible name is `Hidden files Ctrl + H` (label + shortcut child). Click still reveals `.hidden.txt`. Other appearance options (`Columns`, `Airy`, `Group by file type`) stay cleanly named.
Expected: name `Hidden files`; accelerator in `description` (same pattern as context-menu items).
Evidence: `/opt/cursor/artifacts/browser_modes_explore.log` (button dump), `/opt/cursor/artifacts/bm_02_appearance_menu_columns.png`, `/opt/cursor/artifacts/bm_10_list_hidden_files.png`.

### Known / adjacent (not new)

- Closed leftover Shift+click on Columns (#795 / this SHA). Not re-tested here (pointer-selection scope). E2E selection cases were not re-run.
- Icons type-grouping is intentionally gated (`BrowserMode::supports_type_grouping` is List-only).

## Gaps

- Origin `strata-qa-browser-modes` checklist was not available; cases above follow the E2E view-switching / popover contract plus appearance-menu / density / grouping / sort / restart exploration.
- Numpad Ctrl+KP_1/2/3: product maps `gdk::Key::KP_*` in `browser_mode_for_digit`; this Xvfb XTEST session did not deliver KP_2. Number-row shortcuts are covered.
- No same-process second *window* in the GUI (no New Window shortcut in the footer). Two-browser live bindings are covered by the GTK preference test.
- Compact vs Airy spacing is asserted by the container `icons-airy-view` baseline; native host screenshots are not a pixel gate (C.UTF-8 glyph noise).
- Click-activation segmented controls live on Settings → General (other scope). Defaults (Columns: 1-click folders / 2-click files; Icons/List: 2-click both) are unit-tested.

## Evidence

| File | What |
| --- | --- |
| `/opt/cursor/artifacts/e2e-view-switching-popovers.log` | 33/33 container E2E (`test_view_switching.py` + `test_popover_scrolling.py`) |
| `/opt/cursor/artifacts/e2e-baselines-sort.log` | 29/29 container E2E (mode baselines + sort) |
| `/opt/cursor/artifacts/rust-browser-mode-tests.log` | 42 passed / 1 ignored `browser_mode` Rust filter |
| `/opt/cursor/artifacts/rust-live-prefs.log` | Two-browser live mode/density/grouping |
| `/opt/cursor/artifacts/browser_modes_explore.log` | AT-SPI exploratory log (prefs, names, restart) |
| `/opt/cursor/artifacts/bm_01_columns_default.png` | Default Columns |
| `/opt/cursor/artifacts/bm_02_appearance_menu_columns.png` | Appearance: Columns + Compact checked; grouping insensitive |
| `/opt/cursor/artifacts/bm_03_icons_after_ctrl2.png` | Icons after Ctrl+2; `todo.txt` selected |
| `/opt/cursor/artifacts/bm_05_icons_airy.png` | Icons after Airy |
| `/opt/cursor/artifacts/bm_06_icons_thumbnail_panel.png` | Thumbnail size popover |
| `/opt/cursor/artifacts/bm_07_list_after_ctrl3.png` | List after Ctrl+3 |
| `/opt/cursor/artifacts/bm_08_appearance_menu_list.png` | List checked; grouping sensitive |
| `/opt/cursor/artifacts/bm_09_list_grouped.png` | Type headings in List |
| `/opt/cursor/artifacts/bm_10_list_hidden_files.png` | `.hidden.txt` after appearance Hidden files |
| `/opt/cursor/artifacts/bm_12_sort_panel.png` | Sort-by popover |
| `/opt/cursor/artifacts/bm_16_list_airy_after_restart.png` | List + Airy + grouping + hidden after restart |
