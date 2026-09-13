# Plan: keep multi-selection after F5 refresh

Issue: https://github.com/lgse/strata/issues/853

## Goal

F5 (and the pane Refresh button) must keep a pointer multi-selection in Columns, Icons, and List. Names that were selected before the reload stay selected after the directory is ready again. A keyboard cursor may remain as today; the selected set must not collapse to empty.

## Research

- `NavigationState::reload_column` snapshots only the focused row into `selection_target`, then clears `entries` and `selected`. It does not snapshot `selected_locations`.
- `install_snapshot` / `apply_batch` restore that single `selection_target`. `select_first_on_load` is not set on refresh, so it is not the wipe.
- `Browser::refresh_column` emits `ColumnReloaded` and drops `last_batch_selection`.
- UI: Columns and Icons/List detach the GTK `MultiSelection` model on `ColumnReloaded`. `complete_publication` emits `SelectionSetChanged` **before** `LoadFinished`. The GTK model is still detached, so `set_selections` / `set_column_selections` cannot stick.
- `LoadFinished` reconnects the model and sets `syncing` false **without** re-applying `selected_positions`. An empty GTK selection then syncs back through `connect_selection` / `set_selection` and clears app state. That matches QA: `selected_names` goes empty; a cursor may remain.
- Incremental splices already restore GTK selection after the model mutation (`EntriesSpliced`). Reload does not.
- Distinct from closed Shift-click / drag bugs (#771, #770, #521).

## Approach (smallest change)

1. **Navigation:** on `reload_column`, copy `selected_locations` (plus the focused location) into `pending_selection`. After the new listing is installed, restore that set to still-present entries and keep `selection_target` for the focused row.
2. **Columns UI:** after reconnecting the selection model on `LoadFinished`, apply `browser.selected_positions` via `set_column_selections` while syncing is still true.
3. **Icons/List:** `Pane::finish_loading` reconnects, then `set_selections` from `browser.selected_locations` / `selected_positions`, then clears the syncing flag.
4. Do not change event order of `SelectionSetChanged` vs `SortingFinished` (publication tests depend on it). Do not keep `last_batch_selection` across reload.

## Touch points

- `src/app/navigation.rs` — snapshot / restore
- `src/ui/browser/events.rs` — Columns `LoadFinished`
- `src/ui/browser_modes/events.rs` and `reconnect_pane_model` — Icons/List
- Tests: `src/app/navigation/tests.rs`, `src/app/browser/tests.rs`, `src/ui/browser/tests/focus.rs`

## Constraints

- GTK 4 `MultiSelection` must be updated with `syncing` true so `selection-changed` does not overwrite restored locations.
- Theme / preferences / icons: no change.
- Adjacent unit tests, not inline in production modules.

## Risks

- Reconnect still fires a synchronous empty `selection-changed`. Restore must happen before `syncing` goes false.
- Hidden files / vanished entries: restore only locations still in the listing.
- Initial `LoadFinished` (not a reload) must not regress: applying `selected_positions` again is a no-op when already selected.

## Non-goals

- Preserving the pane filter across view-mode switches (#851).
- ShowItems hidden-file reveal (#858).
- Changing F5 to a partial/incremental refresh.
- Auto-refresh interval behavior beyond the same reload path.
