# QA suite sweep summary — 2026-09-11

- **Product:** `wmfeht/strata` (same as `lgse/strata` main) @ `66fb0e6cb346b2dc127438a50bea48834aaf219f`
- **Skills:** Origin `wmfeht/strata-skills` @ ~`f79b5db` (most agents could not fetch Origin; reconstructed COMMON layout)
- **Model:** Grok 4.6 high, one agent per scope
- **Coordinator:** Strata QA Test Coordinator

## Scope → verdict

| Scope | Verdict | Agent |
| --- | --- | --- |
| navigation | pass-with-nits | bc-c191eafb |
| browser-modes | pass-with-nits | bc-f2824a51 |
| pointer-selection | pass | bc-a6668e6a |
| keyboard | pass | bc-67187367 |
| file-operations | pass-with-nits | bc-f2c7dcea |
| trash | pass-with-gaps (env: no GVfs trash:///) | bc-336ffd6c |
| archives | **fail** | bc-e6731602 |
| search-filter | pass-with-findings | bc-6e8bc5c4 |
| previews | **fail** | bc-6da40080 |
| dialogs-menus | pass | bc-f1da998e |
| settings-general | pass-with-nits | bc-bb1e5b4b |
| theming | pass-with-nits | bc-f4adb068 |
| updates-about | pass-with-nits | bc-d7225697 |
| file-chooser | pass-with-nits | bc-29879c63 |
| desktop-integration | pass-with-nits | bc-1a382bd7 |
| accessibility | pass-with-nits | bc-5de74638 |

## Product non-nits (deduped, for Filing Clerk A)

1. **[previews] major — Sandboxed media preview / video thumbs fail on Ubuntu 24.04 (missing `/etc/alternatives` bind)**  
   `sandbox_command` does not bind `/etc/alternatives`; `libblas.so.3` is an alternatives symlink → ffmpeg/`ffmpegthumbnailer` fail in bwrap. GUI: “Preview unavailable — The sandboxed preview renderer failed.” Unsandboxed helper works; adding `--ro-bind /etc/alternatives` fixes. Distinct from #773.

2. **[previews] minor — Audio-only files cannot be normalized even outside the sandbox**  
   `media_command` always `-map 0:v:0`. Audio-only ogg fails with “Unable to normalize media preview.” Masked today by finding 1.

3. **[archives] — Extract to… + cancel password hijacks the next archive completion**  
   `show_extract_to_dialog` sets `pending_navigate` before extract; cancel password does not clear it; later Extract here navigates into the empty leftover destination folder.

4. **[search-filter] minor — NFC query does not match NFD filename**  
   No Unicode normalization in search/filter indexing. NFC `résumé` misses NFD `résumé.txt`. Related note: #739 fixed adjacency scoring only.

5. **[accessibility] minor — Pane filter query field has no accessible name**  
   Ctrl+F field is AT-SPI `text ''`; placeholder only.

6. **[accessibility] minor — Global search field name is the locations tooltip**  
   Ctrl+K entry announces multiline “Search locations: …” instead of “Search”.

7. **[desktop-integration] minor — X11 `WM_CLASS` is `strata`/`strata` while `StartupWMClass=io.github.lgse.Strata`**  
   `_GTK_APPLICATION_ID` is correct; X11 shells that key only on WM_CLASS may show a generic icon.

## Known open (not re-filed)

- #594 Icons leftover/card gutter drag replace-marquee (pointer-selection confirmed still open)
- #717 chooser nested filtered-path test readdir order (file-chooser)
- #583 Menu / Shift+F10 does not open context menu (keyboard / a11y)
- #597 leftover Ctrl/Shift early-release — **did not reproduce** on this SHA (#786/#797 hold); candidate to close

## Nits / doc drift (sample; not filed)

- Address bar / #467 closed without landing on main (navigation)
- Footer “Files on clipboard” vs docs “Paste available”; README undo wording (file-operations)
- Azure Glow vs Tokyo Night first-run docs; CSS `!important` warnings; Add-theme editor below fold (theming)
- Release notes show GitHub HTML comment (updates-about)
- Segmented controls no AT-SPI `checked`; video backend no activate (settings-general)
- F1 toggle swallows open reference as name (accessibility)
- Extract-to creates empty folder on password cancel (archives nit)
- Shift+Down in filter query inconsistent across modes (search-filter nit)

## Gaps

- Origin skills not readable from Cloud Agents (no Origin auth) — checklists reconstructed
- Trash view/Restore/Empty not E2E on this VM (`GIO_USE_VFS=local`)
- No GPU for VA-API preview paths; Wayland app_id not checked
