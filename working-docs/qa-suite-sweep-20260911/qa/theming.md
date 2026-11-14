# QA — theming

Scope: `strata-qa-theming` only.
Product: `wmfeht/strata` @ `66fb0e6cb346b2dc127438a50bea48834aaf219f` (v0.15.0).
Agent: `bc-f4adb068-6df9-4a34-99d0-6f4a02a5e11d`.

Skills: Origin `wmfeht/strata-skills` (`COMMON.md`, `strata-qa-theming/SKILL.md`) was not readable from this VM (`origin` CLI unauthenticated; GitHub has no `strata-skills` repo). This report uses the COMMON exploratory-QA layout reconstructed from `strata-exploratory-qa` plus the product theming contract in `docs/themes.md`, `docs/preferences.md`, `src/ui/theme.rs`, and `src/ui/settings/theme.rs`.

No product fixes. No GitHub issues filed. No product PR comments.

## Verdict

`pass-with-nits`

Bundled catalog, live theme switching, custom TOML themes (including `rgb()` light palettes from #655), first-run Tokyo Night, Omarchy Quattro follow + live recolor, text-size persistence, missing-id fallback, and two-window theme save all hold on this SHA. New nits are documentation (Azure Glow vs Tokyo Night), GTK CSS parser warnings from `!important` scrollbar rules, the Add-a-theme editor revealing below the fold without scrolling, and Light/Dark filtering only the bundled catalog (Your themes stay listed). Closed #655 (rgb / `#fff` parse) did not recur.

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
| Rust `cargo test --all-targets --all-features -- theme` | pass | **65 passed**, 0 failed (catalog, tokens, Quattro map, #655 rgb/`#fff`, preferences, two-page theme bindings) |
| Bundled catalog 95 / unique / alphabetical | pass | `bundled_catalog_is_valid_unique_and_alphabetical` |
| Fresh install, no `settings.toml` | pass | Tokyo Night chrome; Follow Omarchy row hidden; file not written until a change |
| Settings → Theme & appearance | pass | Typography, Search themes, All/Light/Dark, catalog, Your themes |
| Catalog search `nord` | pass | Nord + Nord Light remain; selecting Nord persists `theme = "nord"` and recolors chrome |
| Light filter (bundled) | pass | Retry: only `* Light` bundled cards stay visible |
| Dark filter (bundled) | pass | Toggled; complementary to Light |
| Text size Large | pass | Persists `text_size = "large"` |
| Custom `~/.config/strata/themes/*.toml` | pass | Ocean Blue applies to chrome; Paper Light listed; `tokyo-night.toml` replaces bundled name with User Tokyo |
| `rgb()` light custom (#655) | pass | Whole UI goes light; card named `Rgb Light Editor`; Light filter keeps it; Dark filter still shows it under Your themes (not catalog) |
| Add a theme editor | pass / nit | Opens via AT-SPI `activate` (`Add theme` / Cancel). Pointer click on the clipped card missed once. Editor does not scroll into view |
| Omarchy Quattro present (fresh) | pass | Follow Omarchy row + switch on; in-memory `mode = omarchy`; `settings.toml` still absent until a save |
| Omarchy live recolor | pass | Rewrite `theme.name` + `theme/colors.toml` → chrome goes crimson without restart |
| Missing saved theme id | pass | UI falls back to Azure Glow; file still stores `does-not-exist` until the next save |
| Two windows, select Dracula | pass | Same-process second window; `theme = "dracula"` after activate (pointer into off-screen catalog missed once) |
| Theme pages follow external changes | pass | Rust `theme_hint_and_channel_controls_follow_external_changes` (follow switch, text size, selected card, new custom on two pages) |

Skipped: Orca / high-contrast desktop; live Omarchy/Hyprland (seeded Quattro files only); syntax-highlight preview of an open source file after rgb() save; editing an existing custom theme; pinned `./scripts/e2e.sh` container (native private Xvfb used). Appearance **view-mode** menu is browser-modes / settings-general, not this scope.

## What works

- Fresh HOME with no settings file uses Tokyo Night and does not show Follow Omarchy.
- With `$HOME/.local/state/omarchy/current/theme.name` + `theme/colors.toml` (Quattro keys), first launch follows Omarchy and the switch is on. Changing those files while following recolors the running window.
- Settings → Theme & appearance search, All/Light/Dark, and theme-card selection persist and apply CSS immediately.
- Custom TOML under `~/.config/strata/themes/` is discovered at startup. Same-id files replace the bundled entry. `rgb()` tokens theme the chrome (closed #655).
- Text size writes `settings.toml`. Missing theme ids remap in memory to Azure Glow.
- Two Settings windows share `ThemeManager`; selecting Dracula in one persists for the process.
- The 95-theme catalog stays valid, unique, and alphabetical. SourceView schemes canonicalize `rgb()` / `#fff`.

## Findings

### Product non-nits

None that break documented theming behavior.

### Nits / process

#### 1 — docs/themes.md still calls Azure Glow the default

Area: `docs/themes.md` vs `docs/preferences.md` / `Preferences::default`.
Related: none filed for this mismatch.
Repro: fresh isolated HOME, no `settings.toml`.
Result: Tokyo Night. Azure Glow is the **missing-id** fallback (`theme.rs` load), not the first-run default.
Expected: docs agree (Tokyo Night unless Omarchy is followed).
Evidence: `/opt/cursor/artifacts/theming_fresh_tokyo_night.png`, `/opt/cursor/artifacts/theming_fresh_settings_theme.png`.

#### 2 — GTK CSS parser warnings on every launch

Area: `src/style.css` delete-confirmation scrollbar rules (`padding` / `min-width` / `margin` / `background` / `border` with `!important`).
Related: seen on v0.15.0 exploratory QA; not filed.
Result: `Theme parser error: <data>:512-528` including `Expected a valid color` for `border: none !important`. Chrome still paints.
Expected: stylesheet loads without GTK warnings on GTK 4.14.
Evidence: `/workspace/target/e2e-artifacts/test_add_theme_editor_ui/strata.log`.

#### 3 — Add-a-theme editor does not scroll into view

Area: Settings → Theme & appearance → Your themes → Add a theme.
Repro: open Theme & appearance on a typical window; activate Add a theme.
Result: Revealer opens (`Add theme` / Cancel in AT-SPI) below the catalog. The visible viewport stays on bundled cards; the form is off-screen until the user scrolls.
Expected: revealing the editor scrolls it on screen.
Evidence: `/opt/cursor/artifacts/theming_add_theme_editor_retry.png` (editor open, still showing catalog).

#### 4 — Light/Dark filter ignores Your themes

Area: catalog filter vs custom grid (`bind_catalog_filter` only tracks packaged cards).
Result: a `rgb()` light custom stays visible under Dark (Your themes). Bundled Light filter is correct.
Expected: documented as catalog-only, or custom cards honor the same filter. Not a #655 regression (luminance parse works; rust `theme_appearance_uses_background_luminance` passed).
Evidence: `/opt/cursor/artifacts/theming_retry_results.json` (`rgb-in-dark-catalog` still lists `Rgb Light Editor`).

#### 5 — Invalid theme id is not rewritten until the next save

Area: `ThemeManager::load` remaps unknown `theme` to `azure-glow` in memory only.
Result: UI shows Azure Glow; `settings.toml` still has `theme = "does-not-exist"`.
Expected: acceptable (matches “save on change”); surprising if a user inspects the file.
Evidence: `/opt/cursor/artifacts/theming_qa_results.json` (`missing-theme-fallback`).

#### 6 — Origin theming skill unavailable

Process: could not clone `origin.cursor.com/wmfeht/strata-skills`. Report follows sibling sweep layout.

## Gaps

- No live Omarchy install; Quattro state was seeded under throwaway `$HOME`.
- Did not exercise GtkSourceView syntax colours in the Preview pane after creating an editor theme (covered by rust `source_style_scheme_xml_canonicalizes_rgb_tokens_for_gtksourceview`).
- Did not run the pinned `./scripts/e2e.sh` container for this scope.
- First pointer-driven Add-a-theme / two-window catalog clicks failed because cards were clipped; AT-SPI `activate` + `scroll_into_view` succeeded. Those are harness geometry issues, not product fails.
- Draft/shared sweep PR not commented. No issues filed.
