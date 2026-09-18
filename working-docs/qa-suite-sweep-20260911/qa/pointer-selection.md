# Pointer selection QA

| Field | Value |
| --- | --- |
| Scope | `strata-qa-pointer-selection` only |
| Product | `wmfeht/strata` @ `66fb0e6cb346b2dc127438a50bea48834aaf219f` |
| Head subject | `fix(selection): claim Columns leftover Shift+click (#797)` |
| Agent | `bc-a6668e6a-fb14-4e5b-bcba-d2e36ca93a3c` |
| Date | 2026-09-11 |
| Report branch | `cursor/qa-pointer-selection-3a3c` (cloud naming). Requested path kept. |

## Verdict

**pass** — no new product defects in this scope.

Known-open `#594` still reproduces in **Icons** only. Leftover Shift/Ctrl early-release (`#597` / `#786` / `#795`) holds in List, Icons, and Columns.

## Skills

Origin `wmfeht/strata-skills` was not readable (`origin` CLI: not authenticated; GitHub `wmfeht/strata-skills` 404). Cases follow product e2e plus historical pointer-selection issues. This file uses an inline COMMON-style template.

## Environment

| Item | Value |
| --- | --- |
| Isolation | Never `DISPLAY=:1`. Canonical E2E in pinned container (private Xvfb + D-Bus + throwaway HOME). Rust via `scripts/test-headless.py`. Native leftover probes via `scripts/e2e-native.sh` (debug only; not a substitute for the container run). |
| GTK (container) | 4.14.5 |
| rustc (container) | 1.98.1 (48a229cea 2026-09-01) |
| Host binary | `/workspace/target/debug/strata` rebuilt at `66fb0e6` |
| Artifacts | `/opt/cursor/artifacts/pointer-selection/` |

## Coverage

| Case | Result | Evidence |
| --- | --- | --- |
| Click replace, Shift range, Ctrl toggle — List / Icons / Columns | pass | container `test_selection.py` |
| Shift range from listing load / after navigate / after keyboard | pass | container `test_selection.py` |
| Modifier click on filename focuses target | pass | container `test_selection.py` |
| Right-click selects; right-click inside multi-selection keeps it | pass | container `test_selection.py` |
| Leftover unmodified click replaces | pass | container `test_selection.py` |
| Leftover Shift early-release keeps range (row modes) | pass | container + native probe (all 3 modes) |
| Leftover Ctrl early-release keeps toggle (Columns + List + Icons) | pass | container (Columns) + native probe (all 3 modes) |
| Background / empty-column / beside-Columns clear | pass | container `test_background_selection.py` |
| Marquee from inert space; Ctrl/Shift marquee; no preview | pass | container `test_pointer_intent.py` |
| Marquee edge/wheel auto-scroll keeps earlier hits | pass | container `test_marquee_scrolling.py` |
| Sidebar marquee reaches leading pane | pass | container `test_pointer_intent.py` |
| Content drag vs click preview; Ctrl-drag copy keeps selection | pass | container `test_pointer_intent.py` (selection/intent only) |
| Columns background / context-menu column target | pass | container `test_column_background.py` |
| Unselected press moves selection; selected press keeps multi for drag | pass | Rust `pressing_unselected_item_moves_selection_on_press_and_preserves_multi_selection` |
| Hit-testing: expanded labels leave inert leftover; icon gutters | pass | Rust `ui::pointer` (4 tests) |
| `#770` unselected drag claims selection | pass | covered by Rust press-on-unselected + shipped `#770` fix on this history |
| `#521` Shift-click after navigation re-anchors | pass | container `test_a_click_after_navigating_re_anchors_the_range` and related |
| `#594` selected-row leftover drag drops sibling | **known open — Icons only** | native probe: Icons `after=['a.txt','b.txt','c.txt','d.txt']`; List/Columns keep `['a.txt','h.txt']` |

Not owned (other sweep agents): keyboard-only range/Escape, file-op DnD conflicts, activation click-count, search/filter, chooser selection.

## Findings

None new.

### Known open (still true on this SHA)

**`#594` Icons leftover/card gutter drag from a selected item starts a replace-marquee and drops the rest of the selection.**

- Setup: letters `a.txt`…`h.txt`, Icons, `a.txt`+`h.txt` selected.
- Unmodified drag from the gutter left of the `a.txt` thumbnail through `d.txt`.
- AT-SPI after release: `['a.txt', 'b.txt', 'c.txt', 'd.txt']` (`h.txt` dropped). Files unchanged (not a move).
- Same gesture from List name leftover, List mid-row (Name/Mode), and Columns far-right leftover **kept** `['a.txt', 'h.txt']`.
- Do not file again. Still open: https://github.com/lgse/strata/issues/594

**`#597` leftover Ctrl/Shift released before mouse-up** did **not** reproduce. All three modes kept the press-time toggle/range. `#786` (List/Icons) and `#795`/`#797` (Columns) hold on `66fb0e6`. `#597` can be closed from this evidence; this agent did not comment on the product issue.

## Gaps

- Origin skill files were not loaded; a coordinator should confirm no extra cases were skipped.
- Native leftover probes used the host GTK 4.14.5 harness (`e2e-native.sh`). Canonical pass/fail for the scripted suite is the container run (103 passed).
- Software-renderer screenshots garble some chrome labels; selection state is from AT-SPI, not pixels.
- `#594` List/Columns non-repro is for leftover-of-row starts. Alt-marquee from the gap above a row (intentional marquee, `test_pointer_intent.py` / `test_marquee_scrolling.py`) still replace-selects; that is the documented inert-marquee path, not a selected-row leftover drag.

## Commands and results

```text
PATH=/tmp/docker-hostnet:$PATH ./scripts/e2e.sh \
  tests/e2e/scenarios/test_selection.py \
  tests/e2e/scenarios/test_pointer_intent.py \
  tests/e2e/scenarios/test_background_selection.py \
  tests/e2e/scenarios/test_marquee_scrolling.py \
  tests/e2e/scenarios/test_column_background.py
# 103 passed in 181.40s

./scripts/test-headless.py ui::pointer
# 4 passed

./scripts/test-headless.py pressing_unselected_item_moves_selection
# 1 passed

# Native leftover probes (throwaway file, not committed): 9 + 2 mid-row probes passed
# Icons #594 still drops h.txt; List/Columns keep a+h
```

## Artifacts

- `/opt/cursor/artifacts/pointer-selection/canonical_e2e.log`
- `/opt/cursor/artifacts/pointer-selection/list_leftover_ctrl_early_release_keeps_a_and_c.png`
- `/opt/cursor/artifacts/pointer-selection/columns_leftover_shift_early_release_keeps_range.png`
- `/opt/cursor/artifacts/pointer-selection/list_selected_inert_drag_keeps_a_and_h.png`
- `/opt/cursor/artifacts/pointer-selection/icons_selected_inert_drag_drops_h_txt.png`
- `/opt/cursor/artifacts/pointer-selection-probes/` (full probe set + AT-SPI notes)
