# QA: desktop-integration

Status: in progress. Origin `wmfeht/strata-skills` (`COMMON.md`, `strata-qa-desktop-integration/SKILL.md`) was not readable from this Cloud Agent (Origin CLI unauthenticated; GitLab MCP discovery failed). This file uses an inline COMMON-style template inferred from the sweep prompt and in-repo desktop-integration docs.

## Meta

| Field | Value |
| --- | --- |
| Product | `wmfeht/strata` |
| Head SHA | `66fb0e6cb346b2dc127438a50bea48834aaf219f` |
| Skill | `strata-qa-desktop-integration` (text unavailable; see Gaps) |
| Agent | `bc-1a382bd7-c3a5-4bce-a8a3-9b0a985d0500` |
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

In-repo desktop integration only (not portal FileChooser, trash, theming, or in-app navigation):

- Desktop entry, icon, application ID, `StartupWMClass`
- `inode/directory` MIME default
- `org.freedesktop.FileManager1` (`ShowFolders`, `ShowItems`, `ShowItemProperties`)
- D-Bus activation (`--gapplication-service`)
- Open With / external launch
- Startup directory/file arguments ("open this location")
- Installer FileManager1 service install/refuse-other-provider

## Cases

Pending.

## Findings

Pending.

## Gaps

- Skill files not loaded; case list is reconstructed from README, `docs/packaging.md`, `src/adapters/file_manager1.rs`, Open With, and installer tests.

## Artifacts

Pending.
