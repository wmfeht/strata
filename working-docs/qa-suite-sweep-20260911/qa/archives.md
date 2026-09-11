# QA — archives

Scope: `strata-qa-archives` only.
Product: `wmfeht/strata` @ `66fb0e6cb346b2dc127438a50bea48834aaf219f` (v0.15.0).
Agent: `bc-e6731602-9723-4543-a2eb-d8b761a92865`.

Skills: Origin `wmfeht/strata-skills` (`COMMON.md`, `strata-qa-archives/SKILL.md`) was not readable from this VM (`origin` CLI unauthenticated; GitHub has no public `strata-skills` repo). This report uses the COMMON exploratory-QA layout reconstructed from sibling sweep reports plus the product archive contract in `src/ui/browser/archive.rs`, `src/ui/browser/events.rs`, `src/adapters/local_operations/archive/`, and `tests/e2e/scenarios/test_archive_*.py`.

No product fixes. No GitHub issues filed. No product PR comments.

## Verdict

`fail`

Compress (ZIP / 7Z / TAR / TAR.GZ), Extract here, Extract to, invalid-archive messaging, password retry for AES zip / content-encrypted 7Z / ZipCrypto CRC collisions, cancel-then-plain-extract, collision replace, and post-compress reveal all hold on this SHA under private Xvfb + D-Bus + throwaway HOME/XDG.

One product non-nit: cancelling the Extract-to password dialog leaves `pending_navigate` set, so the next successful archive completion jumps the window into the leftover empty destination folder.

## Environment

| Item | Value |
| --- | --- |
| SHA | `66fb0e6cb346b2dc127438a50bea48834aaf219f` |
| Version | 0.15.0 |
| Host | Ubuntu 24.04.4 LTS, GTK 4.14.5 |
| Display | Private Xvfb via `xvfb-run` / `./scripts/e2e-native.sh` (never `DISPLAY=:1`) |
| Session | Private D-Bus + throwaway `HOME` / XDG (`/tmp/strata-e2e-home-*`) |
| AT-SPI | Enabled for E2E; Rust unit tests used `GTK_A11Y=none` |
| Binary | Host `/workspace/target/debug/strata` rebuilt at the pin |
| 7z CLI | Not installed (`zip` / `tar` / `gzip` only). Header-encrypted 7z fixtures were not built. |

## Coverage

| Case | Result | Notes |
| --- | --- | --- |
| Rust `archive` filter (88 pass + GTK archive-event cases) | pass | Compression, decoders, destination confinement, cancel, ZipCrypto/AES/7z password mapping |
| E2E `test_archive_errors.py` (7) | pass | Fake zip/7z/tar/tar.gz → damaged; content-encrypted 7z retry; ZipCrypto stored + deflated CRC collision retry |
| E2E `test_archive_reveal.py` (9) | pass | New archive selected and scrolled into view in Columns / Icons / List |
| E2E compress / extract-to Enter + invalid name (3) | pass | `test_dialogs_and_menus.py` archive cases |
| Compress 7Z / TAR.GZ / TAR then Extract here | pass | Unique payload; collision rename `qa-payload (2).txt` |
| TAR / TAR.GZ hide password protection | pass | ZIP shows Password protected; TAR and TAR.GZ hide it |
| Compress password mismatch | pass | Overlay `Passwords do not match` / `Please enter the same password in both fields.` |
| AES zip empty / wrong / correct password | pass | Empty → `Enter a password`; wrong → `Invalid password`; no damaged-archive modal |
| Content-encrypted 7z password + Escape cancel | pass | Cancel dismisses progress; following plain zip Extract here succeeds |
| Compress collision Cancel / Replace | pass | Cancel keeps original zip; Replace overwrites |
| Extract to + password prompt | pass (partial) | Destination folder created immediately; password dialog titled Extract |
| Extract to cancel then later Extract here | **fail** | Window navigates into the cancelled empty folder |
| Header-encrypted 7z (`-mhe=on`) | skip | No `7z` CLI; product compress is content-AES only |
| Mid-extract cancel of a large archive | skip | Unit-covered; not re-driven in the GUI |
| Network / non-native archive paths | skip | UI returns early when `native_path()` is missing |

## What works

- Context menu offers Compress… on regular files and Extract here / Extract to… on recognized extensions.
- Compress dialog formats: ZIP and 7Z can be password-protected; TAR and TAR.GZ hide Protection.
- Invalid archive names (`../escape`) keep the compress dialog open and write nothing.
- Enter submits Compress and Extract to.
- Invalid / damaged archives show `This file is not a valid archive or is damaged.` and a later valid extract still works.
- Password-capable extracts reopen the Extract dialog with inline `Enter a password` / `Invalid password`. No `PasswordRequired` / `MaybeBadPassword` / `Unable to complete operation` modal on the AES zip and content-encrypted 7z paths exercised here.
- ZipCrypto header CRC collisions stay retryable (official E2E stored + deflated).
- After compress, the new archive is selected and scrolled into view (including deep Columns).
- Name collision on compress offers Replace; Cancel leaves the existing archive.

## Findings

### Product non-nits

#### 1 — Extract to + cancel password hijacks the next archive completion

Area: `src/ui/browser/archive.rs` (`show_extract_to_dialog` sets `pending_navigate` before extract) and `src/ui/browser/events.rs` (`OperationFailed` shows the password dialog without clearing `pending_navigate`; `ArchiveCompleted` then `navigate`s it).
Related: extract-to success navigation; leftover empty folder (nit 2).
Repro:

1. Isolated HOME. Compress a file as password-protected 7Z.
2. Right-click the archive → Extract to… → type a new folder name → Extract here / Enter.
3. When the Extract password dialog appears, press Escape.
4. Create or select a plain `later.zip` in the original folder → Extract here.

Result: `later.zip` extracts into the original folder, then the window navigates into the empty leftover destination (`stale-dest`). Address bar shows that folder; the pane says the directory is empty.
Expected: cancelling the password dialog abandons Extract to. A later Extract here / Compress should stay in the current directory.
Evidence: `/opt/cursor/artifacts/archives_extract_to_before_cancel.png`, `/opt/cursor/artifacts/archives_extract_to_stale_navigate.png`, `/opt/cursor/artifacts/e2e_extract_to_stale.log`.

### Nits / process

#### 2 — NIT — Extract to creates the destination folder before a successful extract

Area: `show_extract_to_dialog` `create_dir_all` then `extract`.
Related: finding 1.
Repro: Extract to a new folder on a password-protected archive, then cancel the password dialog.
Result: an empty folder remains in the parent listing (`unpacked-secret/` / `stale-dest/`).
Expected: either create the folder only after a successful extract, or remove an empty leftover on cancel.
Evidence: `/opt/cursor/artifacts/archives_extract_to_password_leftover_folder.png`, fixture tree `unpacked-secret/`.

#### 3 — NIT — Compress password errors are a second modal on top of Compress

Area: `show_error_dialog` for mismatch / empty password while the compress form stays open.
Related: extract password uses inline field errors instead.
Repro: Compress → Password protected → mismatched confirm → Compress.
Result: overlay `Passwords do not match` with Close; the compress dialog remains underneath. Extract password failures stay inline (`Invalid password`).
Expected: consistent inline field errors would match Extract. Not a functional break.
Evidence: `/opt/cursor/artifacts/archives_password_mismatch_dialog.png`.

## Gaps

- Origin `strata-qa-archives` checklist was not available; cases follow product UI/adapters and the official archive E2E files.
- Header-encrypted 7z (7-Zip `-mhe=on`) was not built. Product compress writes content encryption only (`compress_7z` AES + LZMA2).
- No GUI mid-extract cancel of a large/slow archive (decoder/session cancel is unit-covered).
- No non-native / network archive path (code refuses `native_path().is_none()`).
- No archive browsing / mount-as-folder (listed as future work in `docs/todo.md`).
- Pinned `./scripts/e2e.sh` container suite was not re-run; native isolated E2E used the same harness and the rebuilt host binary.

## Evidence

| File | What |
| --- | --- |
| `/opt/cursor/artifacts/rust_archive_tests.log` | Isolated `cargo test … archive` — 88+ GTK archive-event cases passed |
| `/opt/cursor/artifacts/e2e_archives.log` | Native E2E: 19 passed (errors + reveal + compress/extract dialogs) |
| `/opt/cursor/artifacts/e2e_archives_exploratory.log` | Exploratory: 5 passed; 2 harness/assertion misses (mismatch dialog was on screen; extract-to leftover folder) |
| `/opt/cursor/artifacts/e2e_extract_to_stale.log` | Confirmed stale `pending_navigate` after Extract-to cancel |
| `/opt/cursor/artifacts/archives_compress_zip_protection.png` | ZIP shows Password protected |
| `/opt/cursor/artifacts/archives_compress_tar_hides_protection.png` | TAR hides Protection |
| `/opt/cursor/artifacts/archives_aes_zip_invalid_password.png` | AES zip wrong password → inline Invalid password |
| `/opt/cursor/artifacts/archives_7z_password_prompt.png` | 7z Extract password dialog |
| `/opt/cursor/artifacts/archives_7z_cancel_then_plain.png` | After cancel, listing usable again |
| `/opt/cursor/artifacts/archives_compress_collision.png` | File already exists / Replace |
| `/opt/cursor/artifacts/archives_password_mismatch_dialog.png` | Passwords do not match overlay |
| `/opt/cursor/artifacts/archives_extract_to_password_leftover_folder.png` | Extract to password dialog; destination already in the listing |
| `/opt/cursor/artifacts/archives_extract_to_before_cancel.png` | Extract to password dialog before Escape |
| `/opt/cursor/artifacts/archives_extract_to_stale_navigate.png` | After later Extract here: window is inside empty `stale-dest` |
