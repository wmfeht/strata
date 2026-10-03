# QA: search-filter

| Field | Value |
| --- | --- |
| Scope | `strata-qa-search-filter` only |
| Product | `wmfeht/strata` 0.15.0 |
| Head SHA | `66fb0e6cb346b2dc127438a50bea48834aaf219f` |
| Agent | `bc-6e8bc5c4-3771-4549-8ecb-077c0ede5b7f` |
| Date | 2026-09-11 |
| Verdict | **pass-with-findings** |
| Skills | Origin `wmfeht/strata-skills` (`COMMON.md`, `strata-qa-search-filter/SKILL.md`) was not readable from this Cloud Agent (`origin` CLI unauthenticated; GitHub has no `wmfeht/strata-skills`). Coverage follows product docs, e2e, and source. |
| Environment | Private Xvfb + D-Bus; throwaway `HOME`/`XDG` via `./scripts/e2e-native.sh`. Never `DISPLAY=:1`. |

## Coverage

### Automated (private Xvfb)

- Rebuild `target/debug/strata` at the pinned SHA.
- `cargo test --all-targets --all-features search`: **85 passed**, 0 failed (GTK cases under `xvfb-run`, `GTK_A11Y=none`, `STRATA_REQUIRE_GTK_TESTS=1`).
- `cargo test --all-targets --all-features filter`: **26 passed**, **1 failed**, 2 ignored. The failure is the known chooser order test `#717` (out of scope; see below).
- Native e2e (`STRATA_BINARY=target/debug/strata`, `STRATA_E2E_WORKERS=2`, `./scripts/e2e-native.sh`):
  - `tests/e2e/scenarios/test_search_filter_sort.py`
  - `tests/e2e/scenarios/test_filter_results.py`
  - `test_quick_preview.py::test_space_previews_a_filtered_result_without_changing_the_query`
  - `test_inline_renaming.py::test_new_item_clears_a_filter_that_would_hide_its_editor`
  - filter-related `test_escape_selection.py` cases
  - Official cases in that invocation **passed**. Failures in the same run were only a throwaway exploratory file (fixed and re-run separately; **11/11 passed**).

### Exploratory GUI (same e2e harness, throwaway HOME)

| Case | Result |
| --- | --- |
| Type-to-search finds nested `photo.txt`; Escape restores listing | Pass (e2e) |
| Ctrl+F pane filter; directory-only vs recursive | Pass (e2e + live Settings toggle) |
| Filtered click/Enter activation; recursive file double-click launches once | Pass (e2e) |
| Filter dismiss keeps hidden files hidden; Ctrl+H then filter finds `.hidden.txt` | Pass |
| `/` opens an empty filter when type-to-search is on | Pass |
| Type-to-search off: printable keys do not open a filter | Pass |
| Space preview of a filtered result keeps the query | Pass (e2e) |
| Filtered context menu / copy / rename target the real nested path | Pass (e2e) |
| Query updates keep selection, focus, preview, thumbnails | Pass (e2e) |
| New folder/file clears a filter that would hide the editor | Pass (e2e) |
| Ctrl+K searches `$HOME`, not the browsed directory | Pass (e2e) |
| Ctrl+K arrows keep the query focused; Enter opens the selection | Pass (e2e) |
| First Ctrl+K Down stays on the first result; second Down moves | Observed (intentional `#758` quirk) |
| NFC query `résumé` vs NFD filename `re\u{0301}sume\u{0301}.txt` | **Fail** (filter and Ctrl+K) |
| Shift+Down from the filter query | No range extend; List/Icons move like Down; Columns stay |

Logs: `/opt/cursor/artifacts/rust-search-tests.log`, `rust-filter-tests.log`, `e2e-search-filter.log`, `e2e-search-filter-explore.log`.

## Findings

### 1. Minor — NFC query does not match an NFD filename

**Area:** Global search and pane filter (`src/services/search.rs`). Indexing lowercases the relative path and uses `find` / subsequence match. There is no Unicode normalization.

**Repro:**

1. Create `résumé-nfc.txt` (NFC) and `résumé.txt` (NFD: `e` + combining acute).
2. Ctrl+F or Ctrl+K and enter NFC `résumé` (AT-SPI `set_text_contents` in this environment; `é` is not on the Xvfb keymap).
3. Only the NFC file appears.

**Expected:** Both names match, as they are the same user-facing word.

**Evidence:** Exploratory e2e passed the assertion that the NFD name is absent. Screenshots: `filter-nfc-query.png`, `search-nfc-query.png`. Same gap noted in the v0.14.1-rc.1 search review; still present at `66fb0e6`. `#739` fixed multibyte *adjacency scoring*, not NFC/NFD.

**Likely impact:** Copies from macOS/APFS or any NFD-preserving tool. Typical Linux IME input is NFC, so same-machine creates often work.

### 2. Nit — Shift+Down in a filter query never range-extends, and modes disagree

**Area:** List/Icons `src/ui/inline_search.rs` (Up/Down ignore Shift and call `select_row`). Columns `src/ui/browser/columns.rs` (search navigation bails on `SHIFT_MASK`).

**Repro:** Recursive filter `alpha-` over `alpha-one.txt`, `alpha-two.txt`, `alpha-three.txt`. Focus stays in the query. Down, then Shift+Down.

| Mode | After Shift+Down | Multi-select |
| --- | --- | --- |
| Columns | Still `alpha-one.txt` | No |
| List | Moves to `alpha-two.txt` | No |
| Icons | Moves to `alpha-two.txt` | No |

**Expected (from keyboard-first range selection):** Shift+arrow would extend a result range, or at least behave the same in every mode.

**Evidence:** `filter-shift-down-{Columns,List,Icons}.txt` and matching PNGs. Query field remained focused.

### 3. Nit — Ctrl+K shows “Partial search — some folders could not be read” on a clean throwaway HOME

**Area:** Global search coverage (`SearchCoverage::unreadable`). Roots are `$HOME` plus non-internal mounts from GIO / `/proc/self/mountinfo` (`src/ui/window/devices.rs`).

**Repro:** Isolated e2e HOME with only a few files. Ctrl+K, type `nav-` or `résumé`. Results are correct, but the palette footer still says some folders could not be read.

**Expected:** A complete HOME index should not look like a failed search. Host mounts the process cannot read should not alarm the HOME query.

**Evidence:** `search-first-down-quirk.png`, `search-nfc-query.png`.

### 4. Note — first Ctrl+K Down stays on the first result

Documented `#758` behavior (`navigation_started`). Covered by `arrow_keys_keep_entry_focus_and_use_a_logical_navigation_start` and the e2e global-search arrow test. Second Down moves. Not a regression.

## Known / out of scope

- **`#717`** — `filtered_selection_only_accepts_on_enter_or_open_with_exact_nested_path` still fails on this ext4 host (`nested-b` before `nested-a`). Test portability, not product filter behavior. File-chooser scope.
- **`#794`** — enhancement: Enter to leave the query and use Vim `j/k` on results. Not implemented; Enter still opens. Do not treat as a new defect.
- **`#659` / `#739`** — multibyte adjacency; closed/merged. Rust tests still pass.
- Chooser MIME filters, theme catalog filters, and sort-only chrome belong to other scopes.

## Gaps

- Origin skill playbook was not available, so numbered skill cases may be incomplete.
- No container `./scripts/e2e.sh` run (native private Xvfb used; same harness).
- No second live window for filter-scope (covered by `test_preferences_sync_across_windows_and_restart`).
- No remote/GVfs recursive search (`#87` still open).
- No content search (not implemented; `docs/todo.md`).
- NFC query could not be typed on the Xvfb keymap; AT-SPI set the entry text instead.
- Shift+arrow with the **result list** focused (not the query) was not separately driven after the Columns leftover-selection work in `#795` / `#797`.
