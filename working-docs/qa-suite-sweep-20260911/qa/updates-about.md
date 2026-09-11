# QA — updates-about

Scope: `strata-qa-updates-about` only.
Product: `wmfeht/strata` @ `66fb0e6cb346b2dc127438a50bea48834aaf219f` (v0.15.0).
Agent: `bc-d7225697-5599-41ba-8a39-89a05851eba2`.

Skills: Origin `wmfeht/strata-skills` (`COMMON.md`, `strata-qa-updates-about/SKILL.md`) was not readable from this VM (`origin` CLI unauthenticated; GitHub has no `strata-skills` repo). This report uses the COMMON exploratory-QA layout reconstructed from `strata-exploratory-qa` plus the product Updates / About surfaces in `src/ui/settings.rs`, `src/ui/settings/about.rs`, and `src/services/update_check.rs`.

No product fixes. No GitHub issues filed. No product PR comments.

## Verdict

`pass-with-nits`

Settings → About shows the expected identity for this InPlace / source build (name, description, Version `0.15.0`, Commit `66fb0e6cb346`, Author `LGSE Ltd.`, GitHub repository link). The Build row is correctly hidden on `BuildKind::Stable`. Settings → Updates loads, Check now reports **Up to date — version 0.15.0**, current v0.15.0 notes render with **View on GitHub**, and Stable / Preview / Nightly persist in `settings.toml`. Auto-check off keeps the channel controls inactive. One product nit: the GitHub HTML comment that prefixes the v0.15.0 release body is shown as visible text.

## Environment

| Item | Value |
| --- | --- |
| SHA | `66fb0e6cb346b2dc127438a50bea48834aaf219f` |
| Version | 0.15.0 (`BuildKind::Stable`, `UpdateMethod::InPlace` / source) |
| Host | Ubuntu 24.04.4 LTS, GTK 4.14.5 |
| Display | Private Xvfb via e2e `HeadlessDisplay` (starts at `:90`; never `DISPLAY=:1`) |
| Session | Private D-Bus + throwaway `HOME` / XDG (`strata-e2e-home-*`) |
| AT-SPI | Enabled (`python3-gi` / `gir1.2-atspi-2.0`) |
| Network | Live GitHub `api.github.com/repos/lgse/strata/releases` |
| Binary | `/workspace/target/debug/strata` rebuilt at this SHA |

Channel toggles expose AT-SPI state `pressed`, not `checked`. Settings reopen restores the last page (Updates), so a wait for General after reopen is a harness false fail, not a product defect.

## Coverage

| Case | Result | Notes |
| --- | --- | --- |
| Launch on private Xvfb | pass | Browser chrome; Settings opens from header |
| About identity | pass | `Strata`; `A fast, keyboard-first file manager for Linux` |
| About Version | pass | `0.15.0` |
| About Commit | pass | `66fb0e6cb346` (`STRATA_BUILD_COMMIT`) |
| About Build row | pass | Absent on stable (hidden unless non-stable `BuildKind`) |
| About Author | pass | `LGSE Ltd.` |
| About repository link | pass | `GitHub repository` / description `Open the Strata repository` |
| Updates page loads | pass | `UPDATE PREFERENCES`, `RELEASE NOTES`, Check now |
| Auto-check default | pass | Off on a fresh throwaway HOME |
| Check now (Stable) | pass | `Up to date — version 0.15.0` |
| Current release notes | pass / nit | `Current release · v0.15.0`, markdown bullets, **View on GitHub**; HTML comment visible (finding 1) |
| Auto-check on + Stable | pass | Stable `pressed`; checkbox on |
| Re-click selected Stable | pass | Selection stays; prefs remain `stable` |
| Switch to Preview | pass | `settings.toml` `channel = "preview"`; recheck still up to date |
| Switch to Nightly | pass | `channel = "nightly"`; still up to date |
| Auto-check off blocks channel change | pass | Preview activate no-op; Stable stays `pressed`; prefs `stable` |
| Escape dismisses Settings | pass | Overlay gone; browser listing visible |

Skipped: two-window live preference sync, click-outside / **Close settings** click, sidebar update notice, install dialog (no newer installable release on any channel), package-managed About / Updates copy (AUR / Omarchy / pacman), offline / API-error path, raw-HTML interpretation (closed [lgse/strata#48](https://github.com/lgse/strata/issues/48) — HTML is escaped, not executed).

## What works

- About is a dedicated Settings page with centered identity, BUILD INFORMATION, and PROJECT. Version and Commit are selectable. Commit is the short SHA. Stable builds omit the Build row.
- Updates is lazy-built and usable on first open. Auto-check has a named checkbox and description (“Check GitHub for a newer release when Strata starts.”). Channel copy explains Preview vs Nightly.
- Manual Check now against live GitHub reports this SHA as current on Stable, Preview, and Nightly.
- Current notes load without a Check now click: heading, changelog bullets, Full Changelog URL, and **View on GitHub**.
- Channel choice persists across activate and is written to `settings.toml`. Turning auto-check off disables channel switching.
- Escape closes Settings.

## Findings

### Product non-nits

None.

### Nits / process

#### 1 — NIT — GitHub HTML comment is visible in Current release notes

Area: `src/services/update_check.rs` `parse_markdown` (`Event::Html` / `Event::InlineHtml` → `append_escaped`). Related: closed [lgse/strata#48](https://github.com/lgse/strata/issues/48) (do not interpret raw HTML). Escaping is correct; comments are not stripped.
Repro:

1. Isolated HOME/XDG, Settings → Updates (or Check now).
2. Scroll Current release notes for v0.15.0.

Result: first notes label is

```
<!-- Release notes generated using configuration in .github/release.yml at v0.15.0 -->
```

Expected: HTML comments omitted; changelog starts at **What's Changed**.
Evidence: `/opt/cursor/artifacts/updates_about_06_current_notes.tree.txt` (line 259), `/opt/cursor/artifacts/updates_about_20_autocheck_on_start.tree.txt` (line 259), `/opt/cursor/artifacts/updates_about_08_check_result.png`, `/opt/cursor/artifacts/updates_about_20_autocheck_on_start.png`.

### Known open (not new)

None in this scope. #48 remains closed and is not a regression of HTML execution.

## Gaps

- Origin `strata-qa-updates-about` checklist was not available; cases above are the product Settings contract plus exploratory AT-SPI around About identity, channel prefs, Check now, and current notes.
- No second window, so live sync of auto-check / channel across windows was not exercised.
- **Close settings** is present and named; only Escape was used to dismiss.
- Click-outside dismiss was not exercised.
- No newer GitHub release on Stable / Preview / Nightly, so sidebar update notice and install / download dialog were not shown.
- Package-managed install sources (AUR, Omarchy, pacman) were not available on this VM; About / Updates copy for those paths is untested.
- Offline and GitHub API failure messaging were not forced.

## Evidence

| File | What |
| --- | --- |
| `/opt/cursor/artifacts/updates_about_results_round2.json` | Channel, persistence, Escape, HTML-comment cases |
| `/opt/cursor/artifacts/updates_about_03_about.png` | About identity, version, commit, author, repo link |
| `/opt/cursor/artifacts/updates_about_03_about.tree.txt` | AT-SPI for About |
| `/opt/cursor/artifacts/updates_about_08_check_result.png` | Check now → up to date; notes include HTML comment |
| `/opt/cursor/artifacts/updates_about_20_autocheck_on_start.png` | Auto-check on, Stable pressed, visible HTML comment |
| `/opt/cursor/artifacts/updates_about_22_preview.png` | Preview channel selected and persisted |
| `/opt/cursor/artifacts/updates_about_26_escape_closed.png` | Settings dismissed |
| `/opt/cursor/artifacts/updates_about_*.tree.txt` | AT-SPI dumps for each captured surface |
