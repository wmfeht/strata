# QA: desktop-integration

Origin `wmfeht/strata-skills` (`COMMON.md`, `strata-qa-desktop-integration/SKILL.md`) was not readable from this Cloud Agent (Origin CLI unauthenticated; GitLab MCP discovery failed). This file uses an inline COMMON-style template reconstructed from the sweep prompt and in-repo desktop-integration docs.

## Meta

| Field | Value |
| --- | --- |
| Product | `wmfeht/strata` |
| Head SHA tested | `66fb0e6cb346b2dc127438a50bea48834aaf219f` (Strata 0.15.0) |
| Skill | `strata-qa-desktop-integration` (text unavailable; see Gaps) |
| Agent | `bc-1a382bd7-c3a5-4bce-a8a3-9b0a985d0500` |
| Date | 2026-09-11 |
| Verdict | **pass-with-nits** |

## Environment

- Ubuntu 24.04.4, Cloud Agent XFCE/TigerVNC on `DISPLAY=:1` (never used for GTK)
- GTK 4.14.5, Rust 1.98.1
- Isolated live session: private Xvfb `:98` / `:97` / `:99` (`Xvfb -ac`), `dbus-run-session`, throwaway `HOME` + `XDG_*`, `GDK_BACKEND=x11`, `GTK_A11Y=none`, `NO_AT_BRIDGE=1`
- Binary: `/workspace/target/debug/strata` built at the pinned SHA

## Isolation

- Never `DISPLAY=:1`.
- Private Xvfb + private D-Bus for every GTK/live call.
- Throwaway `HOME` / `XDG_CONFIG_HOME` / `XDG_DATA_HOME` / `XDG_CACHE_HOME` / `XDG_STATE_HOME` / `XDG_RUNTIME_DIR`.
- Artifacts under `/opt/cursor/artifacts`.
- First harness attempt failed X11 auth (Xvfb without `-ac`) and activated host Thunar for `FileManager1`. Retried with `-ac`, user FileManager1 service, and `dbus-update-activation-environment`. That is an isolation lesson, not a product defect.

## Scope (owned)

In-repo desktop integration only. Not owned: portal FileChooser UI, trash, theming, in-app navigation, updates.

- Desktop entry, icon name, application ID, `StartupWMClass`
- `inode/directory` MIME default
- `org.freedesktop.FileManager1` (`ShowFolders`, `ShowItems`, `ShowItemProperties`)
- D-Bus activation (`--gapplication-service`)
- Open With / external launch
- Startup directory/file arguments
- Installer FileManager1 service install / refuse-other-provider

## Automated coverage

| Suite | Command | Result |
| --- | --- | --- |
| FileManager1 unit | `xvfb-run -a … cargo test --all-targets --all-features file_manager1` | **6 passed** |
| Open With unit | `cargo test … 'ui::open_with::'` and `'context_menu::tests::open_with'` | **5 + 3 passed** |
| Open-argument unit | `cargo test … open_argument` | **16 passed** |
| Installer | `python3 scripts/test_installer.py` | **24 passed** (includes FileManager1 install + refuse-other-provider) |
| E2E (container) | `PATH=/tmp/docker-hostnet:$PATH ./scripts/e2e.sh tests/e2e/scenarios/test_open_with.py tests/e2e/scenarios/test_startup_arguments.py` | **21 passed** in 47.26s |

A loose `open_with` filter also matched portal FileChooser `filtered_selection_only_accepts_on_enter_or_open_with_exact_nested_path`, which failed. That case is **file-chooser** scope, not this skill. Tight Open With filters are green.

There is **no** in-repo E2E for `FileManager1`. Live D-Bus cases below cover that gap.

## Cases

| ID | Case | Result | Evidence |
| --- | --- | --- | --- |
| DI-01 | Desktop entry has Type, Name, `Exec=strata %U`, Icon, `MimeType=inode/directory`, `StartupWMClass`, FileManager category | PASS | `data/io.github.lgse.Strata.desktop` |
| DI-01b | `desktop-file-validate` | PASS | clean |
| DI-02 | Archive FileManager1 service is `org.freedesktop.FileManager1` + `--gapplication-service` | PASS | `data/io.github.lgse.Strata.FileManager1.service` |
| DI-03 | Throwaway XDG: `xdg-mime default` / `query` for `inode/directory` | PASS | `io.github.lgse.Strata.desktop` |
| DI-04 | Directory handler association is Strata (`xdg-open` not invoked) | PASS | query only |
| DI-05 | Running Strata acquires `org.freedesktop.FileManager1` | PASS | First wait raced (~40s GVFS probe). Later methods succeeded; window mapped as `Strata` |
| DI-06 | `ShowItems` on `example.md` opens parent and selects the file | PASS | `/opt/cursor/artifacts/filemanager1_show_items_selects_example_md.png` |
| DI-07 | `ShowFolders` on `Pictures` | PASS | `di-07-show-folders.png` |
| DI-08 | `ShowItemProperties` on `second.md` | PASS | `/opt/cursor/artifacts/filemanager1_show_item_properties_second_md.png` |
| DI-09 | `ShowItems` with two files in one directory | PASS (see nit F2) | `di-09-show-items-pair.png`; D-Bus accepted; unit test groups both names |
| DI-10 | Invalid `ShowItems` signature rejected | PASS | `busctl` error |
| DI-11 | GTK application id `io.github.lgse.Strata` | PASS | `_GTK_APPLICATION_ID(UTF8_STRING) = "io.github.lgse.Strata"` |
| DI-12 | Unique-app open of a file argument reveals parent + selects file | PASS | `/opt/cursor/artifacts/startup_file_argument_reveals_notes_txt.png` |
| DI-13 | Unique-app open of a directory argument | PASS | `di-13-open-dir-argument.png` |
| DI-14 | D-Bus activation via per-user service (`ShowFolders`) | PASS | dbus activated `strata --gapplication-service`; window presented `launch-dir`; `di-14-dbus-activation.png` |
| DI-15 | X11 `WM_CLASS` vs `StartupWMClass` | FAIL (nit F1) | `WM_CLASS=strata, strata` vs desktop `StartupWMClass=io.github.lgse.Strata` |
| DI-16 | Installer writes user service with installed binary; refuses a second per-user provider | PASS | installer unit tests |
| DI-17 | Open With E2E (files, folders, mixed types, no handlers, launch failure) | PASS | 18 tests |
| DI-18 | Startup-argument E2E (missing path, retry, non-UTF8) | PASS | 3 tests |

## Findings

### F1 — minor — X11 `WM_CLASS` does not match `StartupWMClass`

- **Area:** desktop entry / window matching
- **Expected:** README and `io.github.lgse.Strata.desktop` say windows report application ID `io.github.lgse.Strata` (`StartupWMClass=io.github.lgse.Strata`) so shells match the launcher icon.
- **Actual:** On private X11, `xprop` reports `WM_CLASS(STRING) = "strata", "strata"` and `_GTK_APPLICATION_ID = "io.github.lgse.Strata"`.
- **Steps:** Launch `strata` under private Xvfb; `xprop` the `Strata` window.
- **Impact:** GNOME can still match via `_GTK_APPLICATION_ID`. X11 shells that key only on `WM_CLASS` vs `StartupWMClass` (some XFCE/task-switcher paths) may show a generic icon. Wayland `app_id` was not exercised here.
- **Evidence:** `/opt/cursor/artifacts/x11_wm_class_mismatch.txt`

### F2 — note — paired `ShowItems` visually highlighted one row

- **Area:** FileManager1 reveal selection
- **Expected:** Two URIs in one directory become one request with both names (`file_manager1` unit test) and both rows selected.
- **Actual:** D-Bus accepted the pair. Screenshot `di-09-show-items-pair.png` shows `example.md` highlighted and `second.md` not clearly selected. Timing vs `select_after_load` was not re-probed with AT-SPI.
- **Do not treat as a confirmed product break.**

### Incidental (other scopes)

- First-launch portal FileChooser opt-in dialog appeared in throwaway HOME (`di-05-initial-window.png`). File-chooser scope.
- GTK theme parser warnings (`Junk at end of value for padding`). Theming scope.
- `unable to read trash contents` (`g-io-error-quark` 15) in the isolated session. Trash scope.
- Host Thunar is the system `FileManager1` provider until Strata owns the name or a per-user service is installed. Matches README.

## Gaps

- Skill files from Origin `wmfeht/strata-skills` were never loaded; case IDs are reconstructed.
- No Wayland `app_id` check (forced `GDK_BACKEND=x11`).
- `xdg-open` / a foreign app “Open file location” was not invoked (would leave the isolated bus).
- GVfs FUSE Open With (`sftp`/`smb`) not live-tested; covered by unit tests only.
- Omarchy default-file-manager installer path not exercised (this image is Ubuntu/XFCE).
- No in-repo E2E for FileManager1.
- F2 not confirmed with AT-SPI.

## Artifacts

- `/opt/cursor/artifacts/filemanager1_show_items_selects_example_md.png`
- `/opt/cursor/artifacts/filemanager1_show_item_properties_second_md.png`
- `/opt/cursor/artifacts/startup_file_argument_reveals_notes_txt.png`
- `/opt/cursor/artifacts/x11_wm_class_mismatch.txt`
- `/opt/cursor/artifacts/desktop-integration-cases.tsv`
- `/opt/cursor/artifacts/e2e-desktop-integration.log`
- `/opt/cursor/artifacts/rust-open-with.log`
- `/opt/cursor/artifacts/installer-all.log`

## Notes

Shared sweep path requested: `working-docs/qa-suite-sweep-20260911/qa/desktop-integration.md` on `docs/qa-suite-sweep-20260911`. This agent used branch `cursor/qa-desktop-integration-0500` so parallel scope agents do not collide. No product fixes, issues, or product PR comments.
