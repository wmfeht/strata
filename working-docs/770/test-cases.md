# Test cases: unselected drag source selection (#770)

Narrow verification for the Icons/List highlight mismatch. Payload is already B; cases focus on selection chrome and not regressing group drags or `#631` origins.

## Automated (for the code skill)

### T1 — Helper: unselected item is the sole drag source

- **Purpose:** Exclusive-select decision when B is not in the current selection.
- **Setup:** `selected = [A]`, dragged item `B`.
- **Steps:** Call the extracted `drag_source_entries` (or equivalent) helper.
- **Expected:** Returns payload `[B]` and `exclusive_select = true`.

### T2 — Helper: already-selected item keeps a multi-selection

- **Purpose:** Dragging a selected member must not collapse the group (existing FM behavior).
- **Setup:** `selected = [A, B]`, dragged item `B`.
- **Steps:** Call the helper.
- **Expected:** Returns payload `[A, B]` (same set) and `exclusive_select = false`.

### T3 — Helper: dragging the only selected item is a no-op

- **Purpose:** Do not churn selection when A is selected and the drag starts on A.
- **Setup:** `selected = [A]`, dragged item `A`.
- **Steps:** Call the helper.
- **Expected:** Payload `[A]`, `exclusive_select = false`.

### T4 — E2E: Icons/List exclusive-select B after dragging an unselected file

- **Purpose:** User-visible fix for the issue in the two broken presentations.
- **Setup:** Default fixture (`todo.txt`, `readme.md`, …). Parametrize `browser_mode` Icons and List. Optional: include Columns as a control.
- **Steps:**
  1. `select_entry("readme.md")` (A).
  2. Drag `todo.txt` (B) to empty pane space (`drag_to_point` / existing background no-op pattern). Do not drop on a folder.
  3. Wait until selection is stable.
- **Expected:** `selected_names()` is `["todo.txt"]`. `readme.md` is not selected. File listing unchanged (cancel/no-op drop).

### T5 — E2E regression: multi-selection drag still moves the group

- **Purpose:** Exclusive-select must not fire when the drag source is already selected.
- **Setup:** Existing `test_dragging_a_multi_selection_moves_every_entry` (or keep that test unchanged and rely on it).
- **Steps:** Select `readme.md`, Shift+Down so both files are selected, drag `todo.txt` onto `archive`.
- **Expected:** Both files arrive in `archive`. Unchanged from today.

## Manual (QA / exploratory)

### M1 — Icons: unselected drag, during and after

- **Purpose:** Confirm highlight, not only AT-SPI selected state.
- **Setup:** Icons (Ctrl+2). Isolated config. Two files A/B plus empty space.
- **Steps:** Click A. Press and drag B (icon/caption) into empty pane. Confirm ghost label is B. Release with no drop target.
- **Expected:** During drag, A is not the selected card; B is the source (dimmed/dragging). After release, only B is highlighted.

### M2 — List: unselected drag from the name cell and from row padding

- **Purpose:** List whole-row drag origin (`#631`) plus this selection fix.
- **Setup:** List (Ctrl+3). Same files.
- **Steps:** Click A. Drag B from the filename, then again from unused row padding / label allocation, onto empty space.
- **Expected:** Both starts drag B. Selection moves to B during/after. No activation/open.

### M3 — Columns: still exclusive-selects on the press that starts the drag

- **Purpose:** No regression on the already-correct view.
- **Setup:** Columns (Ctrl+1).
- **Steps:** Same as M1.
- **Expected:** Highlight already on B when the drag starts; stays on B after cancel.

### M4 — Multi-select preserve

- **Purpose:** Press-drag on a selected member of a group.
- **Setup:** Any view. Shift-select A and B.
- **Steps:** Drag B (already selected) to empty space; cancel.
- **Expected:** Both remain selected. Ghost indicates two items when a count badge exists.

### M5 — Ctrl-click then drag (modifier path)

- **Purpose:** Modifier selection is owned by `install_modified_selection_click`, not this fix.
- **Setup:** Icons or List. A selected.
- **Steps:** Ctrl-click B (both selected), then drag B to empty space; cancel.
- **Expected:** Group preserved. Do not exclusive-select down to B.

### M6 — Drop still moves only the drag payload

- **Purpose:** Selection chrome follows B; transfer must still move B only (not leftover A).
- **Setup:** Icons or List. A selected. Folder `archive` present.
- **Steps:** Drag unselected B onto `archive` and drop.
- **Expected:** Only B moves. A stays in the source directory. Destination folder is not also moved.

### M7 — Cancel via Escape / release outside the window

- **Purpose:** After a cancelled drag, B remains the selection (not A).
- **Setup:** Icons or List.
- **Steps:** Select A, start drag on B, release outside the window (see `test_releasing_outside_the_window_cancels_the_drag`).
- **Expected:** Nothing moved. Selection is B.

### M8 — Single-click preview / double-click activate not on drag

- **Purpose:** Exclusive-select in `prepare` must not activate or open preview.
- **Setup:** Default click counts (files: double-click). Optionally enable single-click previews.
- **Steps:** Select A, drag B to empty space.
- **Expected:** No preview drawer for B, no launch. B selected only.

## Out of scope for this set

- Preview header/image drag.
- Marquee from Icons gutters.
- Filtered-column activation-on-press (`#718`).
- Visual PNG baselines.
- Exhaustive density × grouping × filter permutations; grouped List exclusive-clear of other type groups is a useful extra if grouping is on, not a required matrix.
