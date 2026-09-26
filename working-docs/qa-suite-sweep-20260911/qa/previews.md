# QA: previews

Status: complete. Origin `wmfeht/strata-skills` (`COMMON.md`, `strata-qa-previews/SKILL.md`) was not readable from this Cloud Agent (Origin CLI unauthenticated; GitHub `wmfeht/strata-skills` is an empty public stub). This file uses an inline COMMON-style template inferred from the sweep prompt and in-repo preview docs/tests.

## Meta

| Field | Value |
| --- | --- |
| Product | `wmfeht/strata` 0.15.0 |
| Head SHA | `66fb0e6cb346b2dc127438a50bea48834aaf219f` |
| Skill | `strata-qa-previews` (text unavailable; see Gaps) |
| Agent | `bc-6da40080-7b8b-4c65-b842-fc5541265d3b` |
| Date | 2026-09-11 |
| Verdict | `fail` (two product findings; text/image/PDF/interaction paths passed) |

## Environment

| Item | Value |
| --- | --- |
| OS | Ubuntu 24.04.4 LTS, kernel 6.12.94+ |
| GTK | 4.14.5 |
| Rust | 1.98.1 (`/usr/local/cargo`) |
| Binary | `/workspace/target/debug/strata` built at `66fb0e6` |
| Host tools | `ffmpeg` + `libvpx`/`libopus`, `ffmpegthumbnailer` 2.2.2, `bwrap`, ImageMagick `convert` |
| Isolation | Private Xvfb + private D-Bus; never `DISPLAY=:1` |
| HOME / XDG | Throwaway (`dbus-run-session` for Rust; e2e harness `strata-e2e-home-*` / `TestEnvironment`) |
| Artifacts | `/opt/cursor/artifacts` |

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
- Saved mute/volume across players (Rust)
- Hardware video backend preference (read/persist path; this VM has no useful GPU)
- Thumbnails vs full preview
- Print / open-in-default-app chrome (unit coverage; no live print dialog)
- Filtered-result preview without changing the query

## Cases

| Case | Result | How |
| --- | --- | --- |
| Space opens/closes text preview (all modes) | pass | Canonical e2e `test_quick_preview.py` |
| Preview follows keyboard/pointer selection; single-click on/off | pass | e2e |
| List horizontal scroll preserved while previewing | pass | e2e |
| Extended selection updates preview without collapsing | pass | e2e |
| Folder focus closes preview and stays closed | pass | e2e + exploratory |
| Markdown source preview | pass | e2e + exploratory (`page.md` body visible) |
| Space after pointer selection | pass | e2e |
| Filtered-result Space / menu Quick preview | pass | e2e `test_quick_preview` + `test_filter_results` |
| Filtered thumbnail stays across query updates | pass | e2e |
| Simple click opens preview; drag does not | pass | e2e `test_pointer_intent` |
| PNG image preview | pass | Exploratory GUI; raster rendered |
| PDF page preview | pass | Exploratory GUI; raster rendered |
| Video → text replaces media (no leftover player) | pass | Exploratory GUI (`#765` path) |
| Header Close preview (Space) | pass | Covered by e2e Space/close control |
| Text shortcuts inside preview (`#650`) | pass | Rust `clipboard_and_delete_shortcuts_proceed_inside_preview_text` |
| Live mute/volume to every player | pass | Rust `saved_and_live_audio_preferences_reach_every_open_player` |
| Media widget teardown on close/replace | pass | Rust preview GTK tests |
| Video mp4 full preview | **fail** | GUI: **Preview unavailable — The sandboxed preview renderer failed**. Helper works unsandboxed. See finding 1. |
| Audio-only ogg preview | **fail** | Same GUI message. Helper also fails unsandboxed (`-map 0:v:0`). See findings 1 and 2. |
| Video list thumbnail via `ffmpegthumbnailer` | **fail** | Same missing `/etc/alternatives` → `libblas.so.3`. See finding 1. |
| Unsupported `mystery.bin` | pass (by design) | Pane closes; `preview_target` rejects `PreviewContent::Unsupported` |
| Hardware VA-API/Vulkan encode | skipped | No useful GPU; software backend forced |

### Automated commands

```bash
xvfb-run -a dbus-run-session -- env -u WAYLAND_DISPLAY GDK_BACKEND=x11 \
  GTK_A11Y=none NO_AT_BRIDGE=1 STRATA_REQUIRE_GTK_TESTS=1 \
  cargo test --all-targets --all-features preview
# 62 passed, 0 failed (filter also matches release-channel "preview" tests)

PATH=/tmp/docker-hostnet:$PATH STRATA_E2E_WORKERS=2 \
  ./scripts/e2e.sh \
    tests/e2e/scenarios/test_quick_preview.py \
    tests/e2e/scenarios/test_pointer_intent.py \
    tests/e2e/scenarios/test_filter_results.py \
    -k 'preview or thumbnail'
# 50 passed in 77.25s
```

Exploratory GUI used the in-repo e2e harness (private Xvfb + AT-SPI + throwaway HOME), never the desktop display.

## Findings

### Product non-nits

1. **Sandboxed media preview and video thumbnails fail on Ubuntu 24.04 because `/etc/alternatives` is not bound**
   - **Severity:** major
   - **Area:** `src/sandbox.rs` (`sandbox_command`), media helper / `ffmpegthumbnailer`
   - **Related:** distinct from `#773` (VA-API SIGSEGV on audio-with-cover). This is a software-path load failure.
   - **Repro:**
     1. On Ubuntu 24.04, build Strata at `66fb0e6`.
     2. In an isolated HOME, open a directory that contains a short H.264/AAC `clip.mp4`.
     3. Select the file and press Space (software video backend).
     4. Optionally inspect `ldd $(which ffmpeg) | grep blas` and `readlink -f /usr/lib/x86_64-linux-gnu/libblas.so.3`.
   - **Expected:** Sandboxed ffmpeg produces the VP8/Opus WebM preview (first 30s) described in `docs/preview-sandbox.md`.
   - **Actual:** Pane shows **Preview unavailable** / **The sandboxed preview renderer failed**. Unsandboxed `strata --preview-helper preview-media clip.mp4 out.webm 0 software` exits 0 and writes a WebM. The same `bwrap` recipe as `sandbox_command` fails with `ffmpeg: error while loading shared libraries: libblas.so.3` because `libblas.so.3` is an alternatives symlink under `/etc/alternatives`, which the sandbox does not bind. Adding `--ro-bind /etc/alternatives /etc/alternatives` makes both raw ffmpeg and the helper succeed. `ffmpegthumbnailer` hits the same missing library, so video list thumbnails also fail closed.
   - **Evidence:** `/opt/cursor/artifacts/preview_video_mp4.png`, `/opt/cursor/artifacts/media_sandbox_libblas.txt`

2. **Audio-only files cannot be normalized even outside the sandbox**
   - **Severity:** minor
   - **Area:** `src/sandbox_helper.rs` `media_command` (`-map 0:v:0`)
   - **Repro:**
     1. `ffmpeg -f lavfi -i sine=frequency=660:duration=1 tone.ogg`
     2. `strata --preview-helper preview-media tone.ogg /tmp/out.webm 0 software`
   - **Expected:** An audio preview (controls, no video frame) or a dedicated fail message.
   - **Actual:** Helper exits 1 with `Unable to normalize media preview` because every backend requires video stream `0:v:0`. In the GUI this is currently masked by finding 1 (`The sandboxed preview renderer failed`). After a sandbox bind fix, audio-only files would still fail.

### Nits / process

- Case list reconstructed without `strata-qa-previews/SKILL.md`; some skill-only edges may be missing.
- Cargo filter `preview` also ran release-channel tests (`services::release_channel`, update-check “preview” feed). All passed; not preview-pane coverage.
- `PreviewContent::Unsupported` has a “No visual preview / Metadata is available” UI, but `preview_target` never opens the pane for `application/octet-stream`. Selecting `mystery.bin` dismisses the drawer (same as a folder). Treated as intended.

## Gaps

- Origin `COMMON.md` and `strata-qa-previews/SKILL.md` not loaded.
- No live print dialog or “Open in default application” click (print layout unit-tested; Open is hidden when `allow_external_open` is false in some test drawers; main window button not clicked).
- No GIF preview, no multi-page PDF zoom/scroll, no two-window mute/volume live check (Rust covers the preference binding).
- Hardware VA-API/Vulkan encode not exercised (no useful GPU). `#773` not re-run.
- Native-host exploratory GUI is extra evidence; the pre-push media gap is still the sandbox bind, which e2e does not cover (no media render scenarios).

## Artifacts

| Path | What it shows |
| --- | --- |
| `/opt/cursor/artifacts/preview_text_notes.png` | Text preview of `notes.txt` |
| `/opt/cursor/artifacts/preview_markdown.png` | Markdown source preview |
| `/opt/cursor/artifacts/preview_image_png.png` | PNG raster preview |
| `/opt/cursor/artifacts/preview_pdf.png` | PDF raster preview |
| `/opt/cursor/artifacts/preview_video_mp4.png` | Video fail-closed message |
| `/opt/cursor/artifacts/preview_audio_ogg.png` | Audio fail-closed message |
| `/opt/cursor/artifacts/preview_after_video_to_text.png` | Video → text teardown |
| `/opt/cursor/artifacts/rust_preview_tests.log` | Isolated Rust `preview` filter |
| `/opt/cursor/artifacts/e2e_preview.log` | Canonical container e2e (50 passed) |
| `/opt/cursor/artifacts/exploratory_preview.log` | Exploratory AT-SPI log |
| `/opt/cursor/artifacts/media_sandbox_libblas.txt` | libblas / alternatives repro notes |
