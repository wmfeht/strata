# QA — trash

Scope: `strata-qa-trash` only.
Product: `wmfeht/strata` @ `66fb0e6cb346b2dc127438a50bea48834aaf219f` (v0.15.0).
Agent: `bc-336ffd6c-3e39-49e2-a7d6-4d721b76d894`.

Skills: Origin `wmfeht/strata-skills` (`COMMON.md`, `strata-qa-trash/SKILL.md`) was not readable from this VM (`origin` CLI unauthenticated; GitHub has no `strata-skills` repo). This report uses the COMMON exploratory-QA layout reconstructed from sibling sweep reports plus the product trash contract in `docs/trash-restore-testing.md`, `src/ui/browser/trash.rs`, `src/adapters/trash.rs`, and `src/adapters/trash_restore.rs`.

No product fixes. No GitHub issues filed. No product PR comments.

## Verdict

`pass-with-gaps`

Delete-to-trash, context-menu Move to Trash, Ctrl+Z undo, Shift+Delete cancel/confirm, folder trash (List), spaced names, and the empty-Trash sidebar menu hiding all hold on this SHA under isolated HOME/XDG. Restore-safety, empty-trash batching, trash thumbnails, and menu visibility are covered by the Rust suite. Trash **view**, Restore confirmation, and Empty Trash could not be exercised end-to-end: the pinned E2E harness forces `GIO_USE_VFS=local` (no `trash:///` backend), and a private GVfs session still leaves `trash:///` empty even after XDG Trash has files. The ignored GVfs capabilities test failed for the same ENOENT when restoring via `trash:///`.

## Environment

| Item | Value |
| --- | --- |
| SHA | `66fb0e6cb346b2dc127438a50bea48834aaf219f` |
| Version | 0.15.0 |
| Host | Ubuntu 24.04.4 LTS, GTK 4.14.5, GVfs 1.54.4 |
| Display | Private Xvfb via `./scripts/test-headless.py` / `./scripts/e2e.sh` / `e2e-native.sh` (never `DISPLAY=:1`) |
| Session | Private D-Bus + throwaway `HOME` / XDG (`strata-e2e-home-*`, `/tmp/qa-trash-home-*`) |
| AT-SPI | Enabled for E2E; Rust unit tests used `GTK_A11Y=none` |
| Binary | Host `/workspace/target/debug/strata`; container suite built inside `strata-e2e:2e7a1a2e797d3068-1000-1000` |

## Coverage

| Case | Result | Notes |
| --- | --- | --- |
| Rust `trash` filter (bin: 76 pass, 1 ignored) | pass | Empty-trash batches, restore-safety, thumbnails, menus, undo bookkeeping |
| Rust `restore` filter (bin: 72 pass, 2 ignored) | pass | Confirmation copy, destination presentation, GTK restore widgets |
| Rust ignored `isolated_trash_supports_read_copy_move_restore_and_delete` | fail (env) | Ran under `dbus-run-session` + throwaway HOME. Restore: `No such file or directory` for `trash:///strata-433-restore.txt` |
| E2E `test_delete_moves_the_entry_to_trash` | pass | Delete writes isolated `XDG_DATA_HOME/Trash/files` |
| E2E Shift+Delete cancel / confirm / symlink parent | pass | Permanent delete does not enter Trash |
| E2E undo of a completed move | pass | Adjacent Ctrl+Z, not trash-specific |
| Exploratory Delete key (Columns / Icons / List) | pass | File leaves the tree; XDG trash gains a payload |
| Exploratory context menu Move to Trash | pass | Restore is absent outside Trash |
| Exploratory Ctrl+Z after Delete | pass | `todo.txt` returns to the fixture |
| Exploratory Shift+Delete cancel / confirm | pass | Confirm leaves XDG trash unchanged |
| Exploratory folder trash (List) | pass | `documents/` and contents leave the tree |
| Exploratory spaced name `my notes.txt` | pass | Trashes via Delete |
| Sidebar Trash place present | pass | Named `Trash`, description `trash:///` |
| Empty-Trash sidebar row when empty | pass | Right-click Trash offers Properties only; no Empty Trash… |
| Address bar `trash:///` (E2E `GIO_USE_VFS=local`) | fail (harness) | Dialog `Unable to open location`: trash backend not installed |
| Sidebar open Trash (E2E local VFS) | fail (harness) | Pane title Trash; `Unable to read this directory` / `Operation not supported`; Empty Trash button present but not sensitive |
| Restore / Empty Trash from Trash view | skip | Blocked on `trash:///` enumeration |
| GVfs probe: `gio trash` then `gio list trash:///` | fail (env) | Files land in XDG Trash; `trash:///` item-count 0; `gvfsd-trash` running; `$XDG_RUNTIME_DIR/gvfsd` socket missing |

Skipped: volume-trash / sticky `.Trash/$uid` on a real extra mount; unsupported atomic-rename filesystems; other-app `trash:///` monitor refresh; Open With on trash items (URI-only); nested trash-folder Restore hidden (unit-covered only); two-window undo of the latest trash.

## What works

- Delete and Move to Trash send files and folders into the isolated Freedesktop trash (`XDG_DATA_HOME/Trash/{files,info}`).
- Ctrl+Z undoes the latest trash and puts the original path back.
- Shift+Delete asks first; Cancel keeps the file; Confirm deletes without a trash payload.
- Permanent delete through a symlinked parent still removes the target (canonical E2E).
- Sidebar always shows Trash. When Trash is empty/unknown, the sidebar menu hides Empty Trash… and its separator (Properties remains).
- Address bar explains a missing trash backend instead of hanging.
- Rust restore planner rejects cross-volume, parent escapes, destinations inside trash trees, missing parents, and non-atomic-rename classes. Empty Trash walks in bounded batches and can abort mid-flight.
- Trash thumbnails use the local `standard::target-uri` payload path, not the `trash:///` identity.

## Findings

### Product non-nits

None demonstrated on a working `trash:///` backend. Delete / undo / permanent delete / empty-menu hiding match the documented contract.

### Environment / harness gaps

#### 1 — E2E `GIO_USE_VFS=local` cannot open Trash

Area: `tests/e2e/harness/environment.py` (`GIO_USE_VFS=local`) vs sidebar / address bar `trash:///`.
Related: not a product regression; harness skips GVfs on purpose.
Repro:

1. Isolated E2E HOME. Delete `todo.txt` (XDG trash fills).
2. Click sidebar Trash, or type `trash:///` in the address bar.

Result: sidebar shows an empty Trash pane with `Unable to read this directory` / `Operation not supported` and Retry. Address bar: `Unable to open location` — “The trash:// backend isn't installed on this system, so trash:/// locations can't be opened.” Empty Trash in the pane header is visible but not `sensitive`.
Expected for this harness: Trash view is untestable. On a desktop with GVfs Trash the backend should list the just-trashed item.
Evidence: `/opt/cursor/artifacts/trash_view_unsupported.png`, `/opt/cursor/artifacts/trash_uri_error_dialog.png`, `/opt/cursor/artifacts/e2e_trash_tests.log`.

#### 2 — Isolated GVfs session still does not enumerate `trash:///`

Area: GVfs `gvfsd-trash` under `dbus-run-session` + throwaway HOME/XDG (the setup the ignored Rust test documents).
Related: `src/adapters/local_operations/tests/trash_capabilities.rs` is `#[ignore]` for this reason.
Repro:

1. `dbus-run-session` with disposable `HOME`, `XDG_*`, `XDG_RUNTIME_DIR`; unset `GIO_USE_VFS` and `DISPLAY`.
2. `gio trash $HOME/probe-file.txt`.
3. `gio list trash:///` / `gio info trash:///probe-file.txt`.

Result: payload and `.trashinfo` appear under `XDG_DATA_HOME/Trash`. `gvfsd` + `gvfsd-trash` start. `trash:///` lists nothing (`trash::item-count: 0`). Clients warn that `$XDG_RUNTIME_DIR/gvfsd` is missing and fall back to the session bus. The ignored capabilities test then fails restore with `strata-433-restore.txt: No such file or directory`.
Expected: `trash:///` should list the XDG item so Restore / Empty Trash can run. This VM cannot provide that without attaching to the Cloud Agent desktop session (forbidden).
Evidence: `/opt/cursor/artifacts/rust_trash_tests.log` (ignored test), `/tmp/qa-trash-home-*` probe output in this agent log.

### Nits / process

#### 3 — NIT — Columns Delete after opening a folder targets the child pane

Area: Columns selection vs trash. Not a Trash implementation bug.
Related: pointer-selection / browser-modes scopes.
Repro: Columns, click `documents`, press Delete.
Result: current directory becomes `documents`; the folder stays on disk (Delete applies to the child listing). List mode Delete on the same folder succeeds.
Expected: users who want to trash the folder must keep selection in the parent column. Recorded so Trash QA does not treat it as a delete failure.

## Gaps

- Origin `strata-qa-trash` checklist was not available; cases follow `docs/trash-restore-testing.md` and the Trash UI/adapters.
- No Restore confirmation (full destination, cancel during lookup, collision, cancel during confirm) on a live `trash:///` listing.
- No Empty Trash confirmation / progress / cancel / partial-error dialog on a live Trash.
- No other-application trash monitor (GIO `FileMonitor` on `trash:///`).
- No extra-volume `.Trash/$uid`, bind-mount, or NFS/FUSE `RENAME_NOREPLACE` rejection on this host.
- No Open With / Print / Quick preview on trash items (URI-only handlers).
- Nested `trash:///folder` child: Restore/Permanently delete hidden (GTK unit coverage only).
- Two-window “latest trash undo” (unit-covered in `app::browser::tests`).

## Evidence

| File | What |
| --- | --- |
| `/opt/cursor/artifacts/rust_trash_tests.log` | Isolated `trash` + `restore` filters; ignored GVfs capabilities failure |
| `/opt/cursor/artifacts/e2e_trash_tests.log` | Pinned `./scripts/e2e.sh` 18 passed / 6 failed (Trash-view cases) |
| `/opt/cursor/artifacts/e2e_native_trash_pass.log` | Native re-run: 11 passed (delete/undo/folder/spaces/sidebar) |
| `/opt/cursor/artifacts/delete_key_after_trash.png` | After Delete: `todo.txt` gone from the listing |
| `/opt/cursor/artifacts/undo_trash_restored.png` | After Ctrl+Z: `todo.txt` back |
| `/opt/cursor/artifacts/sidebar_trash_place.png` | Sidebar Trash place |
| `/opt/cursor/artifacts/empty_trash_sidebar_menu_when_empty.png` | Trash context menu is Properties only |
| `/opt/cursor/artifacts/trash_view_unsupported.png` | Sidebar Trash pane: Operation not supported |
| `/opt/cursor/artifacts/trash_uri_error_dialog.png` | Address bar: trash backend not installed |
