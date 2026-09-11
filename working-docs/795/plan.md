# Plan: Columns leftover Shift+click collapses if Shift is released before mouse-up

Issue: https://github.com/lgse/strata/issues/795
Base: `lgse/strata` `main` at `f4e2a9b` (v0.15.0)

## Goal

Keep the press-time Shift range in Columns when the click is on name-cell leftover and Shift is released before mouse-up. Match List leftover after #786 and match Columns leftover when Shift is held through release.

## Reported behavior

Measured on v0.15.0 (`f4e2a9b`) with letters `a.txt`…`l.txt`, preview closed:

| Sequence | Result |
| --- | --- |
| Columns leftover of `f.txt`; Shift released before mouse-up | collapses to `f.txt` |
| Columns leftover; Shift held through mouse-up | range `a.txt`–`f.txt` kept |
| Columns filename text; Shift released before mouse-up | range kept |
| List leftover; Shift released before mouse-up | range kept (#786) |

Press-time `select_range` already runs. The unmodified release path then replaces the range with the clicked row because the Columns gesture is not claimed on leftover.

## Research

### In-repo precedent (#786 / #771)

List and Icons use `install_modified_selection_click` in `src/ui/browser_modes.rs`. Before #786 that helper claimed only when Control/Shift **and** `hits_item_content` were true. Leftover is not item content (`src/ui/pointer.rs`: expanded labels fill the row; only rendered glyphs count).

#786 (`edb7453`) changed the List/Icons press path to:

- run `select_range` / toggle as before
- grab focus only when `hits_item_content`
- **always** `Claimed` after a modifier selection (unmodified presses still `return` before that)

Release still claims if Control/Shift is still down. That covers held-through leftover. Early modifier-up needs the press-time claim.

Columns does **not** use that helper. Miller rows own their own `GestureClick` in `src/ui/browser/columns/rows.rs`. The press handler still does:

```text
if (control || shift) && hits_item_content { grab_focus; Claimed }
```

The Columns release handler already claims when the modifier is still down, which is why held-through leftover works.

#597 remains open and is List-focused. After #786, List leftover early-release keeps the range on v0.15.0. This issue is the remaining Columns path, not a reopen of #597.

### Why leftover fails

1. `hits_item_content` is false on unused name-label allocation (`src/ui/pointer.rs`, covered by `src/ui/pointer/tests.rs`).
2. Columns therefore leaves the sequence unclaimed on leftover Shift/Ctrl press.
3. If Shift is gone by mouse-up, the release handler also does not claim.
4. GTK’s native list-item selection then treats the release as an unmodified click and `select_item`s the row.

Filename glyphs skip step 2, so early Shift-up still keeps the range.

### Related tests that do not catch this

Existing E2E leftover Shift-clicks hold modifiers through mouse-up:

- `tests/e2e/harness/interaction.py` `Pointer.click` presses modifiers, `_tap`s (down then up), **then** releases modifiers in `finally`.
- `test_shift_click_ranges_from_the_entry_a_fresh_listing_selected[row-space-columns]` and `test_shift_click_revisits_a_file_after_opening_a_folder[row-space]` therefore exercise the held-through path, which already passes.

There is no harness helper that releases Shift after button-down and before button-up.

### Architecture constraints

- Pointer intent is shared (`docs/architecture.md`): Columns/List treat the whole `.file-row` / `.list-row` as a drag origin; `hits_item_content` stays the glyph/icon hit test.
- Columns click, pending activation, search, and drag stay on the row factory; do not fold Columns into `install_modified_selection_click` for this fix.
- GTK 4.14 baseline. No theme, icon, or preference changes.

## Approach

Smallest change: mirror #786 **only** in the Columns press handler.

In `src/ui/browser/columns/rows.rs` `selection_click.connect_pressed`:

1. Keep the existing Shift `select_range` / Ctrl toggle / unmodified `select_item` logic.
2. When `control || shift`, claim the sequence **regardless of hit location**.
3. Gate **only** `grab_focus` on `hits_item_content` (and parent widget), matching List/Icons after #786.

Do not change:

- `hits_item_content`
- List/Icons `install_modified_selection_click`
- the Columns release claim that already handles held-through modifiers
- unmodified leftover clicks (they must still replace the selection)

Optional later (non-goal): extract a shared claim helper. Columns still needs its own press path for search, pending activation, and drag grouping.

## Touch points

| Path | Role |
| --- | --- |
| `src/ui/browser/columns/rows.rs` | Product fix: press-time `Claimed` after modifier selection |
| `tests/e2e/harness/interaction.py` | Likely: a click that releases modifiers before mouse-up |
| `tests/e2e/scenarios/test_selection.py` | Regression for Columns leftover + early Shift-up; keep existing held-through leftover cases |
| `src/ui/pointer/tests.rs` | No change expected; leftover vs glyph hit tests already exist |

## Risks

- Claiming leftover modifier press may compete with the grouped `DragSource` (`selection_click` is grouped with the row drag). List already claims leftover modifier presses after #786; Columns should match that. Verify unmodified leftover drag and that a stationary leftover Shift-click does not start a drag.
- Ctrl leftover in Columns uses the same claim gate. Fixing Shift without claiming Ctrl would leave the same collapse for Ctrl. Include Ctrl leftover early-release as a sibling case, not a separate product change.
- Do not claim unmodified leftover presses, or a plain leftover click would stop replacing the selection.

## Non-goals

- Product implementation in this plan commit (code stage owns the patch).
- Re-fixing List/Icons or closing #597 from this issue.
- Changing `hits_item_content`, marquee predicates, or treating leftover as item content.
- Refactoring Columns onto `install_modified_selection_click`.
- Theme, icons, preferences, or activation/preview behavior.
