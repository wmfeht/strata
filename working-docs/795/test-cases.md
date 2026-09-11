# Test cases: #795 Columns leftover Shift+click

Narrow verification for the Columns leftover claim fix. Existing held-through leftover E2E already passes and is a regression, not a replacement for early modifier-up.

Harness note: `Pointer.click(..., modifiers=)` holds modifiers until **after** mouse-up. Early-release cases need an explicit sequence (Shift down → button down → Shift up → button up), either a new helper or a one-off using the AT-SPI connection.

Setup unless noted: folder of short names `a.txt`…`l.txt` (or the E2E fixture with enough leftover in the name cell). Columns (`Ctrl+1`). Preview closed (`single_click_previews=false`). Click leftover at the right of the name label (still on the row), not the glyphs.

## 1. Columns leftover, Shift released before mouse-up (the bug)

**Purpose:** Press-time range must survive early Shift-up.

**Setup:** Select `a.txt` on filename text.

**Steps:**

1. Hold Shift and press in the leftover of `f.txt`.
2. Release Shift while the button is still down.
3. Release the mouse button.

**Expected:** Selection is `a.txt`…`f.txt`, not only `f.txt`.

## 2. Columns leftover, Shift held through mouse-up (control)

**Purpose:** Held-through leftover must keep working.

**Setup:** Same as case 1.

**Steps:** Shift+click leftover of `f.txt` and release the button before Shift.

**Expected:** Range `a.txt`…`f.txt`. This is the path current E2E leftover tests already cover.

## 3. Columns filename glyphs, Shift released before mouse-up (control)

**Purpose:** Glyph hits already claim; do not regress.

**Setup:** Select `a.txt` on filename text.

**Steps:** Shift-press on `f.txt` glyphs, release Shift, then mouse-up.

**Expected:** Range `a.txt`…`f.txt`.

## 4. List leftover, Shift released before mouse-up (regression vs #786)

**Purpose:** Columns change must not disturb List.

**Setup:** List view. Select `a.txt`.

**Steps:** Same early Shift-up on leftover of `f.txt`.

**Expected:** Range `a.txt`…`f.txt` (already true on v0.15.0).

## 5. Columns leftover, Ctrl released before mouse-up

**Purpose:** Same claim gate wraps Ctrl toggle.

**Setup:** Select `a.txt`.

**Steps:** Ctrl-press leftover of `f.txt`, release Ctrl, then mouse-up.

**Expected:** `a.txt` and `f.txt` selected (not only `f.txt`).

## 6. Unmodified leftover click still replaces selection

**Purpose:** Do not claim leftover when no modifier ran.

**Setup:** Select `a.txt`.

**Steps:** Plain click leftover of `f.txt`.

**Expected:** Only `f.txt` selected.

## 7. Unmodified leftover drag still starts

**Purpose:** Claiming modifier leftover must not disable whole-row drag.

**Steps:** Press leftover of a file, move past the drag threshold, drop on another folder (or cancel after drag begins).

**Expected:** Drag starts from leftover as today (`row_whitespace_point` / existing drag E2E). No drag on the stationary leftover Shift-click in case 1.

## Out of scope for this issue

- Icons gutter vs leftover (Icons already uses `install_modified_selection_click` after #786).
- Marquee from pane background.
- Search-filtered Columns activation (separate claim on that path).
