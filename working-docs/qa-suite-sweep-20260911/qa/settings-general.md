# QA — settings-general

Scope: `strata-qa-settings-general` only.
Product: `wmfeht/strata` @ `66fb0e6cb346b2dc127438a50bea48834aaf219f` (v0.15.0).
Agent: `bc-bb1e5b4b-207b-42e0-992e-1b5bd3d7ffda`.

Skills: Origin `wmfeht/strata-skills` (`COMMON.md`, `strata-qa-settings-general/SKILL.md`) was not readable from this VM (`origin` CLI unauthenticated; GitHub has no `strata-skills` repo). This report uses the COMMON exploratory-QA layout reconstructed from `strata-exploratory-qa` plus the product General page in `src/ui/settings/general.rs` and `docs/preferences.md`.

No product fixes. No GitHub issues filed. No product PR comments.

## Verdict

`pass-with-nits`

Settings → General browsing switches, first-run defaults, persistence, restart, two-window sync, and live apply all hold on this SHA. Cross-device drop, auto-refresh, hardware video backend, click-activation, type-to-search, filter-subfolder scope, single-click previews, and folder peeking behave as documented. New nits are AT-SPI: segmented ToggleButtons never expose `checked`, and the video-backend `MenuButton` has no accessible activate action (pointer still works). Closed #515 (peeking ignored until Settings opens) did not recur.

## Environment

| Item | Value |
| --- | --- |
| SHA | `66fb0e6cb346b2dc127438a50bea48834aaf219f` |
| Version | 0.15.0 |
| Host | Ubuntu 24.04.4 LTS, GTK 4.14.5 |
| Display | Private Xvfb via `xvfb-run` / e2e `HeadlessDisplay` (never `DISPLAY=:1`) |
| Session | Private D-Bus + throwaway `HOME` / XDG (`strata-e2e-home-*`) |
| AT-SPI | Enabled for E2E (`python3-gi` / `gir1.2-atspi-2.0`); Rust unit tests used `GTK_A11Y=none` |
| Binary | `/workspace/target/debug/strata` rebuilt at this SHA |

## Coverage

| Case | Result | Notes |
| --- | --- | --- |
| Rust `ui::settings::` (37) | pass | Includes `every_general_control_stays_in_sync_without_initializing_browser_behavior`, dialog sizing, video labels |
| Rust `ui::theme::tests::preferences` (15) | pass | First-run save, unreadable/malformed TOML preserve, recursive-filter default |
| E2E `test_preferences.py` | pass | Four browsing switches sync across two windows and survive restart |
| Header Settings opens General | pass | Default page; headings BROWSING / FILE TRANSFERS / REFRESH / VIDEO PREVIEWS / MOTION / CLICK ACTIVATION / DESKTOP INTEGRATION |
| Opening Settings does not rewrite `settings.toml` | pass | Byte-identical file after open |
| Ctrl+, opens General | pass | Sent as Control + comma keysym `0x002c` (harness has no `comma` name) |
| Browsing + Reduce motion persist + restart | pass | Folder peeking, single-click previews, open search results, type-to-search, include subfolders, reduce motion |
| Cross-device DnD Always Copy / Move / Ask | pass | Persists `always-copy` / `always-move` / `always-ask`. Visual selection works |
| Auto-refresh Off / 1 / 5 / 10 min | pass | Persists `0` / `60` / `300` / `600` |
| Hardware video accel + backend | pass | Enabling makes backend sensitive; VA-API persists as `"vaapi"` |
| Click activation 12 named controls | pass | `Columns/Icons/List` × Files/Folders × 1/2 clicks. Persist `explorer_*` / `grid_*` / `list_*` |
| Live click-activation without restart | pass | Canonical `test_changing_the_preference_takes_effect_without_restarting` |
| System file chooser Configure… | pass (shallow) | Dialog opens; deep portal owned by file-chooser / desktop-integration |
| Type-to-search off | pass | Typing `photo` does not open a filter; listing unchanged |
| Type-to-search on | pass | Types into filter; `todo` matches `todo.txt` |
| Live type-to-search toggle | pass | Off then on in the same session, no restart |
| Filter exclude subfolders | pass | `photo` matches nothing in the root when the switch is off |
| Filter include subfolders | pass | `photo` matches nested `photo.txt` |
| Type-to-search nested (canonical) | pass | Columns / Icons / List `test_type_to_search_finds_matches_anywhere_in_the_tree` |
| Single-click previews | pass | Selecting `readme.md` opens Preview with `# Fixture` |
| Folder peeking on hover | pass | Icons + `folder_peeking=true`; peek over `archive` (“This directory is empty”) |
| First-run defaults (no `settings.toml`) | pass | Peeking on, single-click previews on, open-search-results off, type-to-search on, include-subfolders on, reduce-motion off; Always Ask / Off selected visually |
| Two-window Open search results | pass | Same-process second window; toggle syncs both ways |
| Segmented AT-SPI `checked` | fail (new nit) | No Copy/Move/Ask/refresh/click-activation toggle reports `checked` |
| Video backend AT-SPI activate | fail (new nit) | `MenuButton` `activate()` is false; pointer click works |

Skipped: Appearance / Keybindings / Updates / About pages; theme, hidden files, density, sort (not General); deep xdg-desktop-portal install (other scopes); actual cross-device drop on two mounts; auto-refresh wall-clock wait; hardware decode on real VA-API/Vulkan (Xvfb software). Pinned `./scripts/e2e.sh` container was not re-run for this scope.

## What works

- General is the default Settings page from the header button and from Ctrl+,.
- Constructing Settings does not initialize browser behavior or dirty `settings.toml` (#515 contract).
- All six General switches persist, restart, and (where tested) apply live. `Open search results directly` syncs across two windows (gap in the stock two-window E2E).
- First-run without a settings file matches `Preferences::default()` for those switches.
- Cross-device strategy and auto-refresh interval persist through the segmented controls; GTK `is_active` is covered by `every_general_control_stays_in_sync…`.
- Click-activation accessible names are `"{View} {Files|Folders} {1 click|2 clicks}"` and write the documented serialized keys.
- Type-to-search and include-subfolders change pane-filter behavior without opening Settings again after a live toggle.
- Single-click previews and folder peeking honor the saved General values at startup (no Settings visit required).
- Unreadable / malformed `settings.toml` still uses in-memory defaults and does not overwrite the broken file (GTK preference suite).

## Findings

### Product non-nits

None that break documented General behavior.

### Nits / process

#### 1 — NIT — General segmented ToggleButtons never expose AT-SPI `checked`

Area: Settings → General segmented controls (`src/ui/controls.rs` `segmented_control` + `settings::bindings::bind_choice`).
Related: none found on `lgse/strata` for this gap. Accessibility sweep noted the controls are *named*; it did not assert selected state.
Repro:

1. Isolated HOME/XDG. Open Settings → General (e2e defaults include `cross_volume_drop_strategy = "always-ask"`).
2. Dump AT-SPI for `Always Copy` / `Always Move` / `Always Ask`, `Off` / `1 min` / `5 min` / `10 min`, and the twelve click-activation toggles.

Result: every button reports `focusable`, `sensitive`, `showing`, `visible` only. No `checked` / `pressed` / `selected`. GTK `ToggleButton::is_active` is true for the saved choice (unit test + visual highlight: Always Ask, Off, 2-click columns files, etc.). Persistence after `activate()` / pointer click is correct.
Expected: the active option should expose `checked` (or equivalent) so a screen reader can announce the current strategy / interval / click count.
Evidence: `/workspace/target/e2e-artifacts/test_sg04_cross_volume_and_refresh_and_video/accessibility-tree.txt`, `/opt/cursor/artifacts/settings_general_open.png`, `/opt/cursor/artifacts/settings_general_click_activation.png`.

#### 2 — NIT — video preview backend menu has no AT-SPI activate action

Area: General → VIDEO PREVIEWS backend `MenuButton` (`video_preview_backend_control`).
Related: none found.
Repro: locate `button` / `toggle button` named `Video preview hardware backend` and call the default accessible action.
Result: `activate()` returns false. Pointer click opens the popover; choosing VA-API persists `"vaapi"` and updates the label.
Expected: the button should expose `press`/`click` so keyboard / AT-SPI clients can open the menu without coordinates.
Evidence: `/opt/cursor/artifacts/settings_general_video_backend.png`.

### Known closed (not regressed)

#### 3 — Folder peeking applied before Settings opens

Closed [lgse/strata#515](https://github.com/lgse/strata/issues/515) / [#98](https://github.com/lgse/strata/issues/98). On this SHA, `folder_peeking=true` peeks on hover without opening Settings; `false` is the e2e default and does not peek. Opening Settings does not rewrite preferences.

#### 4 — Unreadable `settings.toml` has no in-UI warning

Intentional per #728 / docs/preferences.md. GTK suite confirms the file is preserved and live changes still apply in memory. Not re-filed.

## Gaps

- Origin `strata-qa-settings-general` checklist was not available; cases above follow the General page + preference lifecycle doc.
- No two-mount cross-device drop (Copy/Move/Ask dialog). Persistence of the strategy only.
- No 1/5/10 minute wall-clock auto-refresh.
- No real VA-API/Vulkan decode (software Xvfb). Backend preference persistence only.
- Deep portal “Use Strata” / restore previous chooser owned by desktop-integration / file-chooser.
- Appearance, Keybindings, Updates, About out of scope.
- Pinned container `./scripts/e2e.sh` not re-run; native harness used the same AT-SPI stack as CI.

## Evidence

| File | What |
| --- | --- |
| `/opt/cursor/artifacts/settings_general_qa.log` | GTK + E2E case rollup |
| `/opt/cursor/artifacts/gtk_settings_tests.log` | 37 `ui::settings::` tests passed |
| `/opt/cursor/artifacts/gtk_pref_tests.log` | 15 preference lifecycle tests passed |
| `/opt/cursor/artifacts/settings_general_open.png` | General page; Ask selected visually |
| `/opt/cursor/artifacts/settings_general_click_activation.png` | Refresh, video, motion, click-activation |
| `/opt/cursor/artifacts/settings_general_video_backend.png` | VA-API selected after persist |
| `/opt/cursor/artifacts/settings_general_first_run_defaults.png` | Missing `settings.toml` defaults |
| `/opt/cursor/artifacts/settings_general_chooser_configure.png` | System file chooser dialog |
| `/opt/cursor/artifacts/settings_general_folder_peek.png` | Icons peek on `archive` |
| `/opt/cursor/artifacts/settings_general_single_click_preview.png` | Preview of `readme.md` |
