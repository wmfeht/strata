# QA: previews

Status: in progress. Origin `wmfeht/strata-skills` (`COMMON.md`, `strata-qa-previews/SKILL.md`) was not readable from this Cloud Agent (Origin CLI unauthenticated; GitHub `wmfeht/strata-skills` is an empty public stub). This file uses an inline COMMON-style template inferred from the sweep prompt and in-repo preview docs/tests.

## Meta

| Field | Value |
| --- | --- |
| Product | `wmfeht/strata` |
| Head SHA | `66fb0e6cb346b2dc127438a50bea48834aaf219f` |
| Skill | `strata-qa-previews` (text unavailable; see Gaps) |
| Agent | `bc-6da40080-7b8b-4c65-b842-fc5541265d3b` |
| Date | 2026-09-11 |
| Verdict | pending |

## Environment

Pending after isolated runs.

## Isolation

- Never `DISPLAY=:1` (Cloud Agent XFCE/TigerVNC).
- Private Xvfb + private D-Bus.
- Throwaway `HOME` / `XDG_*`.
- Artifacts under `/opt/cursor/artifacts`.

## Scope (owned)

Quick preview pane and sandboxed renderers only (not FileChooser-only coverage, not desktop integration, not theming):

- Space / single-click open and close
- Preview follows selection; folder focus dismisses and stays closed
- Text, markdown, image, PDF, media (video/audio/GIF)
- Fail-closed **Preview unavailable**
- Media teardown when switching or closing (`#765`)
- Saved mute/volume across players
- Hardware video backend preference (read path; this VM has no useful GPU)
- Thumbnails vs full preview
- Print / open-in-default-app chrome
- Filtered-result preview without changing the query

## Cases

Pending.

## Findings

Pending.

## Gaps

- Skill files not loaded; case list is reconstructed from `docs/preview-sandbox.md`, `tests/e2e/scenarios/test_quick_preview.py`, `src/ui/preview.rs`, and known issues `#609` / `#650` / `#765` / `#773`.

## Artifacts

Pending.
