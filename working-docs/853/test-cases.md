# Test cases: #853 keep multi-selection after F5

## Automated

1. **Navigation snapshot restore**  
   Setup: listing `alpha`, `bravo`, `charlie`; `set_selection(0, &[0, 2], Some(2))`.  
   Steps: `reload_column` then `install_snapshot` with the same three entries.  
   Expected: `selected_positions` is `[0, 2]`; focused is `2`.

2. **Vanished member**  
   Setup: multi-select `alpha` and `charlie`.  
   Steps: reload; snapshot listing is only `alpha` and `bravo`.  
   Expected: `alpha` stays selected; `charlie` is gone.

3. **Browser reload**  
   Setup: `FakeFileSource` / scripted listing; select two names.  
   Steps: `reload_active()`; wait until not loading.  
   Expected: `selected_positions` still those two names.

4. **GTK Columns / Icons / List**  
   Setup: temp dir with `readme.md` and `todo.txt`; `BrowserView`; select both.  
   Steps: `reload_active()`; wait until loaded.  
   Expected: `selected_positions` is both files; Columns GTK `MultiSelection` has both rows selected.

## Manual

5. **Happy path**  
   Setup: folder with at least three files, Hidden files off.  
   Steps: Ctrl-click two files; press F5.  
   Expected: both stay highlighted; footer still describes a multi-selection.

6. **Icons and List**  
   Setup: same folder.  
   Steps: Ctrl+2, Ctrl-select two files, F5; then Ctrl+3, Ctrl-select two files, F5.  
   Expected: multi-selection survives in both modes.

7. **Refresh button**  
   Setup: Columns, two files selected.  
   Steps: click the pane Refresh (F5) button.  
   Expected: same as keyboard F5.

8. **Regression: empty directory / single selection**  
   Setup: one file selected, then F5; separately, empty folder F5.  
   Expected: single selection restored; empty folder stays empty with no crash.

9. **One of two files deleted on disk before F5**  
   Setup: select `a` and `b`; delete `b` outside Strata.  
   Steps: F5.  
   Expected: `a` remains selected; missing file is gone.
