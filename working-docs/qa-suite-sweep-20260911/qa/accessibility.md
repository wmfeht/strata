# QA — accessibility

Scope: `strata-qa-accessibility` only.
Product: `wmfeht/strata` @ `66fb0e6cb346b2dc127438a50bea48834aaf219f` (v0.15.0).
Agent: `bc-5de74638-ae27-496c-ade6-c4bf552f5e1e`.

Skills: Origin `wmfeht/strata-skills` (`COMMON.md`, `strata-qa-accessibility/SKILL.md`) was not readable from this VM (`origin` CLI unauthenticated; GitHub has no `strata-skills` repo). This report uses the COMMON exploratory-QA layout reconstructed from `strata-exploratory-qa` plus the product accessibility contract in `src/ui/accessibility.rs` and `tests/e2e/scenarios/test_accessibility.py`.

No product fixes. No GitHub issues filed. No product PR comments.

## Verdict

`pass-with-nits`

The AT-SPI contract the E2E suite and screen readers rely on holds on this SHA: named/described entries (including symlink kinds), selection/focus states, pane and file-list names, context-menu roles, dialog names, toolbar names, inline rename fields, Tab-to-listing, and keyboard-only rename. New nits are unnamed/misnamed query fields (pane filter, global search) and the F1 toggle swallowing the open reference as its accessible name. Keyboard Menu / Shift+F10 still does not open the context menu (open #583, not a regression).

## Environment

| Item | Value |
| --- | --- |
| SHA | `66fb0e6cb346b2dc127438a50bea48834aaf219f` |
| Version | 0.15.0 |
| Host | Ubuntu 24.04.4 LTS, GTK 4.14.5 |
| Display | Private Xvfb via `xvfb-run` / `e2e-native.sh` (never `DISPLAY=:1`) |
| Session | Private D-Bus + throwaway `HOME` / XDG (`strata-e2e-home-*`) |
| AT-SPI | Enabled for E2E (`python3-gi` / `gir1.2-atspi-2.0`); Rust unit tests used `GTK_A11Y=none` |
| Binary | `/workspace/target/debug/strata` rebuilt at this SHA |

## Coverage

| Case | Result | Notes |
| --- | --- | --- |
| Rust `ui::accessibility` unit tests (3) | pass | Spoken kind names; Folder ≠ File; distinct view names |
| E2E `test_accessibility.py` (18) | pass | Columns / Icons / List for naming, selection, pane, container; plus menu, dialog, toolbar, inline fields, Tab order, keyboard-only rename |
| Symlink descriptions | pass | `File link` / `Folder link` / `Broken link` |
| Unnamed interactive chrome at startup | pass | No unnamed button / toggle / checkbox / menu item / text field among interactive roles |
| Sidebar / header / pane chrome names | pass | Home, Trash, Network, Desktop, Search, Appearance, Settings, Close window, Refresh, sort, filter toggle, Copy path, F1 Shortcuts |
| Context menu roles + names | pass | `menu` / `menu item`; action name separate from accelerator description |
| Open With row names (#576) | pass | Dialog `Open With`; list `Applications`; every `list item` named (e.g. Review Text Viewer, Emacs, Mousepad, Vim) |
| Settings General controls | pass | Checkboxes and segmented toggles have names and descriptions |
| Empty directory pane | pass | Pane still `empty` / `Columns view`; placeholder label `This directory is empty` |
| Hidden files Ctrl+H | pass | `.hidden.txt` appears and is named |
| Preview panel | pass | Panel name `Preview` |
| Keyboard-only rename | pass | Covered by canonical E2E |
| Menu key / Shift+F10 | fail (known) | Neither opens a `menu` while `todo.txt` is focused+selected. Matches open #583 |
| Pane filter field name | fail (new) | Focused `text` has empty name and description |
| Global search field name | fail (new) | Focused `text` name is the tooltip (`Search locations:\n…\n/var/lib/docker\nRemote shares are not included.`) |
| Search overlay role | nit | Overlay is nested `panel`s on `frame 'Strata'`, not `dialog` |
| F1 shortcut reference | pass / nit | Overlay opens (`Keyboard shortcuts` label). Not a `dialog`. Open toggle name concatenates the full reference |

Skipped: Orca live speech, high-contrast / reduced-motion visual checks (theming scope), file-chooser portal (file-chooser scope), Trash-only a11y (trash scope). Native `e2e-native.sh` used for AT-SPI; pinned `./scripts/e2e.sh` container was not re-run for this scope.

## What works

- File rows expose `list item` / `table cell` names and kind descriptions in all three views. Symlinks use distinct spoken kinds.
- Selected vs unselected states, `focusable`, and Tab from header chrome into the listing.
- Pane group named for the directory and described as `Columns view` / `Icons view` / `List view`. Entry container described as `Files`.
- Pointer context menu uses `menu` / `menu item`. Accelerators live in `description` (`Ctrl+C`, `F2 / Ctrl+R`, `Shift+Del`), not in the name.
- Delete confirmation is a named `dialog`/`alert`. Open With is a named dialog; #576 empty row names are fixed on this SHA.
- Toolbar, sidebar places, breadcrumb segments, and pane chrome buttons are named. Settings General checkboxes are named and described.
- Empty folders stay identifiable. Preview is a named panel. Keyboard-only F2 rename completes.

## Findings

### Product non-nits

#### 1 — MINOR — pane filter query field has no accessible name

Area: pane filter (`Ctrl+F`); likely the filter `GtkEntry` next to `Filter this pane (Ctrl+F)`.
Related: none found on `lgse/strata`.
Repro:

1. Isolated HOME/XDG, Columns, fixture with several files.
2. Focus the listing and press Ctrl+F.
3. Dump AT-SPI for the focused `text` node.

Result: `text ''` with empty description. Visible placeholder is “Filter N items…”. The toggle that opens it is named; the field a screen reader lands in is not.
Expected: name such as `Filter` (and optional description for the current-folder scope).
Evidence: `/opt/cursor/artifacts/tree_filter.txt`, `/opt/cursor/artifacts/a11y_filter.png`, `/opt/cursor/artifacts/followup_notes.json`.

#### 2 — MINOR — global search field announces the locations tooltip as its name

Area: `src/ui/search.rs` (`set_tooltip_text("Search locations:\n…")` with no explicit accessible label).
Related: none found. GTK uses the tooltip as the accessible name when Label is unset.
Repro:

1. Ctrl+K on the same isolated session.
2. Read the focused `text` node’s name.

Result: name is

```
Search locations:
/tmp/strata-e2e-home-…/home
/var/lib/docker
Remote shares are not included.
```

Visible placeholder is “Search files and folders…”. Overlay ancestors are `panel` → `frame 'Strata'` (not `dialog`).
Expected: name `Search` (or similar); locations belong in description or a separate status label. Overlay should be a dialog/alert or otherwise announced as a search surface.
Evidence: `/opt/cursor/artifacts/tree_search.txt`, `/opt/cursor/artifacts/a11y_search.png`, `/opt/cursor/artifacts/exploratory_notes.json`.

### Nits / process

#### 3 — NIT — F1 Shortcuts toggle name swallows the open reference

Opening F1 shows a `Keyboard shortcuts` overlay (not a dialog). While it is open, the footer control’s AT-SPI name becomes `F1  Shortcuts Keyboard shortcuts Close File-view shortcuts…` plus the entire shortcut list. Likely GTK popover children leaking into the toggle name.
Evidence: `/opt/cursor/artifacts/tree_f1_followup.txt` (toggle / button lines), `/opt/cursor/artifacts/a11y_f1_followup.png`.

### Known open (not new)

#### 4 — Menu / Shift+F10 do not open the context menu

`todo.txt` was `focused` + `selected`. `Menu` (keysym `0xFF67`) and Shift+F10 produced no `menu` within 3s. Pointer right-click still opens a fully named menu. This is open [lgse/strata#583](https://github.com/lgse/strata/issues/583) (P1 enhancement). Not filed again.
Evidence: `/opt/cursor/artifacts/tree_after_menu_key.txt`, `/opt/cursor/artifacts/tree_after_shift_f10.txt`.

#576 (Open With empty row names) is **fixed** on this SHA.

## Gaps

- Origin `strata-qa-accessibility` checklist was not available; cases above are the product contract plus exploratory AT-SPI around search, filter, Settings, Open With, F1, Menu, empty dirs, and symlinks.
- No Orca speech log (AT-SPI names/states only).
- No high-contrast / reduced-motion visual pass (owned by theming).
- No file-chooser or Trash-specific sweep (other scopes).
- Pinned container `./scripts/e2e.sh` not re-run; native harness used the same AT-SPI assertions as CI.

## Evidence

| File | What |
| --- | --- |
| `/opt/cursor/artifacts/e2e_accessibility.log` | 18/18 `test_accessibility.py` passed (19.85s) |
| `/opt/cursor/artifacts/rust_accessibility.log` | 3/3 Rust a11y unit tests passed |
| `/opt/cursor/artifacts/exploratory_notes.json` | Symlinks, menu, Open With, search, settings, empty, hidden |
| `/opt/cursor/artifacts/followup_notes.json` | F1, filter, preview, search overlay |
| `/opt/cursor/artifacts/a11y_startup_columns.png` | Columns listing with symlink kinds |
| `/opt/cursor/artifacts/a11y_context_menu.png` | Pointer context menu |
| `/opt/cursor/artifacts/a11y_open_with.png` | Named Open With rows |
| `/opt/cursor/artifacts/a11y_search.png` | Global search overlay |
| `/opt/cursor/artifacts/a11y_filter.png` | Unnamed pane filter field |
| `/opt/cursor/artifacts/a11y_settings.png` | Settings General |
| `/opt/cursor/artifacts/a11y_empty.png` | Empty directory pane |
| `/opt/cursor/artifacts/a11y_preview.png` | Named Preview panel |
| `/opt/cursor/artifacts/a11y_f1_followup.png` | Shortcut reference overlay |
| `/opt/cursor/artifacts/tree_*.txt` | AT-SPI dumps for each surface |
