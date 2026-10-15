# QA — navigation

- **verdict:** pass-with-nits
- **head_sha:** `66fb0e6cb346b2dc127438a50bea48834aaf219f`
- **build:** `cargo build --bin strata --locked` → Strata 0.15.0 debug (`/workspace/target/debug/strata`); rustc 1.98.1; GTK 4.14.5; private Xvfb 1440×900 + private session/AT-SPI buses + throwaway HOME/XDG. Never drove GTK on Cloud Agent `DISPLAY=:1`.
- **agent_id:** `bc-c191eafb-a031-4483-8f3f-44164393b811`

Origin skills `strata-qa-suite/COMMON.md` and `strata-qa-navigation/SKILL.md` were not readable from this VM (`origin auth` / `CURSOR_API_KEY` unavailable; raw Origin URLs 401/Cloudflare). Coverage below follows the user-stated COMMON.md environment rules and report shape, plus product docs (`docs/keyboard-navigation.md`, `docs/preferences.md`, `docs/architecture.md`, README) and the in-tree location/keyboard E2E scenarios.

## Coverage

Exercised on disposable fixtures under isolated XDG (`GIO_USE_VFS=local`, `GIO_USE_VOLUME_MONITOR=unix`). First pass left a context menu open after looking for menu item `Pin` (actual label is `Pin to sidebar`); those later cases were re-run cleanly.

| Area | Result | Notes |
| --- | --- | --- |
| CLI start location | pass | Fixture listing matches argv directory |
| Enter / Columns Right into folder | pass | Child Miller column opens; Right on a file does not preview |
| Alt+Up / Backspace parent | pass | Columns, List, Icons |
| Alt+Left / Alt+Right history | pass | Columns and List |
| Breadcrumb ancestor click | pass | Returns to fixture root |
| Click empty breadcrumb space → edit | pass | Location field + Navigate/Cancel |
| Copy path on current crumb | pass | Tooltip becomes “Path copied” |
| Ctrl+L absolute path | pass | |
| Ctrl+L `~` and `~/Documents` | pass | |
| Ctrl+L `~other-user/…` | pass | Dialog: only `~` and `~/` for current home |
| Ctrl+L relative path | pass | “Enter an absolute path.” |
| Ctrl+L UNC / `//host/share` | pass | Asks for `smb://` / `sftp://` / … |
| Ctrl+L `file://` | pass (intentional reject) | See nits / doc drift |
| Sidebar Home / Desktop / XDG places | pass | Documents/Downloads/Pictures/Videos appear when `user-dirs.dirs` exists |
| Pin to sidebar / unpin | pass | Context menu `Pin to sidebar`; sidebar `Unpin` |
| Ctrl+B hide/show sidebar | pass | |
| Ctrl+Shift+B focus sidebar, Right back to files | pass | Restored `readme.md` |
| F5 refresh | pass | Externally created `appeared-later.txt` listed |
| Unreadable directory | pass | “You do not have permission to open that location.” |
| Missing path | pass | “That location does not exist.” |
| Second window via extra argv | pass | Documents window listed its marker; history stays window-local |
| Native E2E corroboration | pass | `test_locations.py` + location/history subset of `test_keyboard_navigation.py`: **16 passed** |

Not exercised (gaps, not failures): mouse buttons 8/9 history; live SMB/SFTP; GVfs Trash contents (local VFS stub); sidebar drag-reorder (unit-covered); remote mount auth dialog.

## Findings — product non-nits

None confirmed on this SHA. Core navigation (sidebar places, breadcrumbs, location bar, parent, history, pins, mode switches, errors, second window) behaved as documented.

## Findings — nits/process

1. **Address bar still tracks the deepest open path after focusing a parent Miller column.** After Columns Right into `documents` then Left, the parent column owns keyboard focus and “Keyboard · Paste here”, but breadcrumbs / Ctrl+L still show `…/documents`. Issue #467 is **closed completed**, yet `feat/467-address-bar-follows-focused-column` (`5ade46ae`) is **not an ancestor of** `66fb0e6`. Treat as process drift (issue closed without landing on main), not a new product regression.

2. **`file://` in the location bar is rejected** with “The file:// scheme isn't supported. Use an absolute local path or one of: smb://, sftp://, …”. This matches `location_input_rejects_unsupported_uri_schemes` in `src/app/browser/tests.rs`. Fine as product policy; README still says Ctrl+L is “for a path or URI” (see Doc drift).

3. **Error dialogs** always add the generic subtitle “The operation could not be completed” above a specific reason. Readable, slightly redundant.

4. **Origin skill fetch failed** in this environment (no Origin login). Scope checklist reconstructed as noted above.

## Out-of-scope observations

- **Trash** sidebar row opens `Trash`, but with `GIO_USE_VFS=local` the pane shows “Unable to read this directory” + Retry. Expected under the E2E isolation profile; real GVfs Trash belongs to the trash sibling.
- **Network** place: clear dialog “The network:/// backend isn't installed on this system…”. Correct for this VM; remotes/desktop-integration own live GVfs.
- Selection leftover Shift+click is the just-merged `#797` / `#795` work — pointer-selection sibling.

## Doc drift

- README useful-shortcuts line: “Ctrl+L for a path or URI”. Location bar accepts native paths, `~` / `~/…`, `trash:///`, and remote schemes (`smb`/`sftp`/`ftp`/`ftps`/`dav`/`davs`/`network`). It does **not** accept `file://` or `https://`.
- `docs/todo.md` still has unchecked “Add, remove, activate, and reorder bookmarks” and “Persist sidebar state and bookmarks”. Pin / unpin / GTK-bookmark persistence / standard-place reorder already exist on this SHA.
- GitHub issue #467 marked completed while the address-bar-follows-focus branch is unmerged on `main` @ `66fb0e6`.
