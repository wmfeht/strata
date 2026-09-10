# Plan: drag of an unselected file should become the selection

Issue: https://github.com/lgse/strata/issues/770
Base: `lgse/strata` `main` at `fcbb257` (`fix(search): stabilize results, indexing, and keyboard navigation (#758)`)

## Goal

When the user starts a drag on file B while a different file A is selected, B becomes both the drag payload and the selected/highlighted item. After the drag completes or is cancelled, A is no longer highlighted. Icons and List must match Columns and ordinary file-manager behavior.

## What the issue actually is

The reporter described a highlight mismatch while dragging an unselected file. A verification pass on `wmfeht/strata` `main` at `4d75826` (issue comment) reproduced it **only in Icons and List**:

| View | Drag ghost | Highlight during drag | After release |
| --- | --- | --- | --- |
| Icons | B | A stays selected | A still selected |
| List | B | A stays selected | A still selected |
| Columns | B | Highlight moves to B | B stays selected |

The drag **payload is already B** in every view. The bug is selection chrome and the GTK/app selection model in Icons/List, not the `text/uri-list` / `GdkFileList` content.

Reproduced on X11/GTK 4.14.5 as well as the reporter's Arch/Hyprland/Wayland, so this is not compositor-specific.

## Research

### Product architecture

`docs/architecture.md` keeps Columns, Icons, and List on the same `BrowserEvent` stream. Pointer/DnD policy is view-specific:

- Columns and List: whole `.file-row` / `.list-row` is a drag origin (including unused label allocation and padding).
- Icons: content-only drag (`hits_item_content`); gutters remain marquee origins.
- Drag controllers sit on the application-owned row so they can coexist with GTK's native list-item selection gesture.

### Columns already does the right thing

Columns owns unmodified selection in `src/ui/browser/columns/rows.rs`:

- Capture-phase `GestureClick` on press exclusive-selects the pressed row unless `should_preserve_drag_selection` says to keep a multi-selection (`clicked_selected && selected_count > 1`).
- `DragSource::connect_prepare` then uses `browser.selected_entries()` if the dragged location is already selected, otherwise `vec![entry]`.
- Because press already selected B, payload and highlight agree.

`should_preserve_drag_selection` is unit-tested in `src/ui/browser/columns/tests.rs`.

### Icons/List split payload from selection

Icons (`browser_modes.rs` factory setup) and List (`browser_modes/list_factory.rs`) share `install_list_drag_drop` and `install_modified_selection_click`.

`install_modified_selection_click` handles **only** Ctrl/Shift. Unmodified press sets the selection anchor and returns so GTK's native list gesture can own the click. It does **not** exclusive-select on press.

`install_list_drag_drop` `connect_prepare` already computes the correct payload:

```text
if selected contains dragged location → drag the current selection
else → drag only the pressed entry
```

`DragSource` is Capture-phase and grouped with the content click. Crossing the GTK DnD threshold claims the sequence, so the native GridView/ColumnView click never completes. Result: payload is B, GTK `MultiSelection` (and therefore highlight + `Browser::selected_entries`) stays on A through drag-end.

### In-repo precedents

- `#631` (`tests/e2e/scenarios/test_drag_and_drop.py`): row padding/whitespace must start a drag, not steal it for selection. Whole-row List/Columns drag origins must stay. Do not tighten `hits_item_content` again.
- Multi-selection drag already covered: `test_dragging_a_multi_selection_moves_every_entry` selects a group then drags a **selected** member. That path must keep the group.
- Preview drag (`preview_drag_entries`) is a proxy for the previewed file only; out of scope.
- `#718` is a Columns filtered-result press-vs-release activation hole; do not fold it into this fix.

### External pattern

Nautilus, Finder, and Explorer exclusive-select an unselected item that becomes a drag source, and they keep a multi-selection when the press is on an already-selected member. Columns already follows that. The code change should copy Columns' **outcome**, not necessarily its press-time timing.

## Approach (smallest change)

Do **not** rewrite Icons/List to own all unmodified selection on press (that fights native GTK selection and `#631`). Select B when a drag of an unselected item actually starts.

1. **Pass the pane/section `gtk::MultiSelection` into `install_list_drag_drop`** (Icons and List factories already have it; drag setup currently does not).
2. **In `connect_prepare`**, after resolving the dragged entry:
   - If the entry is already in `browser.selected_entries()`, keep that set as the payload (existing multi-select / already-selected behavior).
   - Else use `vec![entry]` as the payload **and** `selection.select_item(view_position, true)` so highlight follows B.
3. **Let existing `connect_selection_changed` sync the app model.** For grouped List, `multiple_selection` is false on a plain drag, so other groups are cleared the same way an exclusive click is. Prefer GTK `select_item` over `Browser::select` / `set_selection`: that matches Columns press handling and avoids emitting `FocusChanged` / fighting `SelectionSynced` apply loops.
4. **Extract a tiny helper** next to `install_list_drag_drop` (for example `drag_source_entries(selected, item) -> (entries, exclusive_select)`) and unit-test it. Do not add a new module. Columns can keep its press-time `should_preserve_drag_selection`; sharing is optional and not required for the fix.

Use the **view** position (`ListItem::position()`), not the source index, when calling `select_item`. Filtered/grouped panes already map source↔view in this function.

`connect_prepare` runs after the DnD threshold, so A may remain highlighted for the press-and-hold before the drag starts. That is acceptable; the reported failure is during and after the drag. Matching Columns' pre-threshold press-select is a non-goal.

### Call sites

- `src/ui/browser_modes.rs` — `install_list_drag_drop`, Icons factory `connect_setup`
- `src/ui/browser_modes/list_factory.rs` — `install_interactions`

## Constraints

- GTK 4.14 baseline; keep `DragSource` Capture + grouping with the content click.
- No theme/icon/preference work. No Settings page changes (`docs/preferences.md` does not apply).
- Tests: helper unit tests in `src/ui/browser_modes/tests.rs` (or an adjacent `tests.rs` already declared from `browser_modes.rs`). E2E in `tests/e2e/scenarios/`. Do not put tests inline in production functions.
- Keep native paths / `FileEntry::location` identity for “is this the selected item?”.

## Risks

- `select_item` inside `prepare` might re-enter `selection_changed` / bind. The existing `syncing` flag and `SelectionSynced` skip in shared browser events should absorb this; if a drag aborts in testing, select after content is prepared or in `drag_begin` instead.
- Grouped List: exclusive-select must clear other type groups. Relies on `!multiple_selection` in `connect_selection`.
- Filtered views: wrong position (source vs view) would select the wrong row.
- Do not exclusive-select when the drag source is already selected (would collapse a multi-selection).
- Ctrl/Shift press already updates selection before prepare; leave that path alone.
- Icons content-only vs List whole-row: the exclusive-select belongs in prepare after the existing hit test, so padding in Icons still starts a marquee rather than a drag.

## Non-goals

- Product code in this plan PR (staging docs only).
- Changing Columns pointer policy (already correct).
- Selecting on every unmodified Icons/List press.
- Preview-pane drag, sidebar/pinned-place drag, marquee, rubber-band.
- Changing drag payload, drop/transfer, or `#631` hit-testing.
- Visual baseline updates.
- `#718` filtered Columns activation-on-press.
- Wayland-only workarounds; the mismatch is in shared GTK selection wiring.
