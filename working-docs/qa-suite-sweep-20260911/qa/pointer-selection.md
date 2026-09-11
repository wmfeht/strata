# Pointer selection QA

Status: **in progress**

| Field | Value |
| --- | --- |
| Scope | `strata-qa-pointer-selection` only |
| Product | `wmfeht/strata` @ `66fb0e6cb346b2dc127438a50bea48834aaf219f` (`fix(selection): claim Columns leftover Shift+click (#797)`) |
| Agent | `bc-a6668e6a-fb14-4e5b-bcba-d2e36ca93a3c` |
| Date | 2026-09-11 |
| Skills | Origin `wmfeht/strata-skills` was **not readable** (`origin` CLI unauthenticated; GitHub 404). Cases reconstructed from product e2e + historical selection issues. Report uses an inline COMMON-style template. |

## Verdict

Pending isolated Rust + canonical E2E + leftover/marquee probes.

## Environment (planned / in use)

- Never `DISPLAY=:1` (Cloud Agent XFCE/TigerVNC).
- Private Xvfb + private D-Bus (`scripts/test-headless.py`, `scripts/e2e.sh`).
- Throwaway HOME/XDG via the e2e harness.
- Artifacts: `/opt/cursor/artifacts`.

## Coverage plan

Pointer-driven selection only (not keyboard-only, not file-op DnD outcomes except where they prove selection/intent).

1. Click replace, Shift range, Ctrl toggle — List / Icons / Columns.
2. Right-click select vs keep multi-selection.
3. Background / empty-column clear.
4. Leftover (inert) name-cell clicks; Shift/Ctrl released before mouse-up (#597 / #786 / #795).
5. Marquee from inert space, modifier marquees, edge/wheel scroll (#594-adjacent).
6. Unselected-file drag claims selection (#770).
7. Range anchor after navigation / first-child load (#521).

## Findings

None yet.

## Gaps

- Origin `COMMON.md` and `strata-qa-pointer-selection/SKILL.md` not loaded.
- Tests not finished.
