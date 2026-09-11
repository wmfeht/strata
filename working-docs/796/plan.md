# Plan: #796 ZipCrypto wrong-password CRC reported as damaged archive

## Goal

A traditional `zip -P` (ZipCrypto) archive that is given a wrong password must stay on the extract-password retry path. Today a colliding password that slips ZipCrypto’s 1-byte header check and then fails CRC is rewritten to “This file is not a valid archive or is damaged.” That string has no `password` / `encrypt`, so `OperationFailed` shows the generic error dialog instead of reopening Extract.

Truly corrupt ZIPs (truncated, fake, checksum failure with no password) keep the #638 wording.

## Issue

- https://github.com/lgse/strata/issues/796
- Label: `bug`. Open. No closing PR.
- Reported on **v0.15.0** (`f4e2a9b`), which already includes #751 (7z checksum → retryable) and #793 (inline extract-password retry).
- Not a duplicate of #689 / #751 (7z), #790 / #793 (one-shot retry UI), or #756 / #783 (raw `PasswordRequired` text).

## Background

ZipCrypto prefixes each member with a 12-byte encrypted header and checks **one byte** (CRC high byte or Info-ZIP DOS time). That check rejects most wrong passwords as `ZipError::InvalidPassword` (`provided password is incorrect`). About 1 in 256 wrong passwords pass, decrypt garbage, and fail the member CRC.

In `zip` 8.6.0 that CRC is `Crc32Reader`: `io::ErrorKind::InvalidData` with `"Invalid checksum"` when the stream hits EOF. Strata maps that through `archive_read_error(..., password_supplied: false)` to `INVALID_ARCHIVE`.

Measured on v0.15.0 for `zip -P zipsecret password.zip some.txt`:

| Password | Decoder | UI |
| --- | --- | --- |
| none | `unsupported Zip archive: Password required to decrypt file` | first Extract prompt |
| non-colliding wrong | `provided password is incorrect` | Extract reopens, “Invalid password” (#793) |
| ZipCrypto header collision (per-archive; examples `165` / `341`) | `Invalid checksum` → **This file is not a valid archive or is damaged.** | Unable to complete operation / Close |
| `zipsecret` | extracts | success |
| Strata AES zip, wrong | `provided password is incorrect` | Extract reopens |

The colliding value is per archive (encrypted header is random). Tests must search for a CRC collision, not hard-code one.

## Code path

1. `src/adapters/local_operations/archive.rs` opens `ZipArchive::new` (structural parse; no password) then `extract_zip_from_archive`.
2. `extract_zip_from_archive` in `src/adapters/local_operations/archive/decoders.rs`:
   - `by_index_with_options` + password → ZipCrypto header check (`InvalidPassword` preserved by `zip_error`’s catch-all).
   - Member body is wrapped in `ArchiveReader::new`, which **always** sets `password_supplied: false`.
   - CRC / inflate errors on read go to `archive_read_error(error, false)` → `INVALID_ARCHIVE`.
3. `zip_error` maps `ZipError::InvalidArchive` and `ZipError::Io` with `password_supplied: false`. CRC is an Io checksum on read, not `InvalidArchive`.
4. `src/ui/browser/events.rs` `OperationFailed` (after #793): if `pending_extract_retry` is armed and the message contains `password` or `encrypt`, reopen Extract; `incorrect` in the message sets the inline “Invalid password” helper. `INVALID_ARCHIVE` matches neither token.

`archive_read_error` already remaps `InvalidData` / `UnexpectedEof` / 7z checksum to `MAYBE_BAD_PASSWORD` (“The password may be incorrect.”) when `password_supplied` is true. #751 wired that flag only for 7z `ArchiveReader`. ZIP extraction never sets it.

## Research

### In-repo precedent: #689 / #751

Content-encrypted 7z (`-mhe=off`) cannot distinguish a wrong password from a content CRC failure. #751 did **not** change the UI heuristic. It set `ArchiveReader.password_supplied` for 7z so checksum failures after a supplied password stringify as `MAYBE_BAD_PASSWORD`, which contains `password` and `incorrect`.

That is the same class of bug for ZipCrypto. Mirror it; do not invent a ZIP-specific UI branch.

### UI already in place: #790 / #793

v0.15.0 re-arms `pending_extract_retry` on Extract confirm and shows inline “Invalid password” when the failure text contains `incorrect`. `MAYBE_BAD_PASSWORD` already satisfies that. No `events.rs` change is required if the decoder maps colliding CRC to that string (or any string containing `password`/`encrypt` and, for the inline helper, `incorrect`).

Broadening the heuristic to treat “damaged” / “valid archive” as retryable would reopen Extract for truncated encrypted ZIPs and fight #638. Out of scope.

### ZipCrypto vs AES

Strata’s compressor writes AES-256 ZIP (`with_aes_encryption`). AES wrong passwords fail the MAC as `InvalidPassword` and are already retryable. The hole is traditional ZipCrypto (`zip -P`, Python `zipfile` password, Info-ZIP).

`FileOptions::with_deprecated_encryption` is `pub(crate)`. Tests can still write ZipCrypto via `zip::unstable::write::FileOptionsExt` or a committed fixture. Prefer the zip crate (or Python `zipfile` in E2E) over shelling out to Info-ZIP so CI stays portable.

### Constraints (AGENTS.md / architecture)

- Decoder error translation stays in `decoders.rs`; `ExtractionSession` policy is unchanged.
- Unit tests in `src/adapters/local_operations/archive/decoders/tests.rs`, not inline in `decoders.rs`.
- E2E for extract-password UX lives in `tests/e2e/scenarios/test_archive_errors.py`.
- No new icons, theme tokens, or preferences.
- GTK tests (if any UI unit tests are added) must run under private Xvfb, not `DISPLAY=:1`.

## Approach

Smallest fix: treat ZIP the same way #751 treated 7z.

In `extract_zip_from_archive`:

1. `let password_supplied = password.is_some();`
2. Wrap the member stream as `ArchiveReader { inner: &mut entry, password_supplied }` instead of `ArchiveReader::new` (which forces `false`).
3. Keep `zip_error`’s `InvalidArchive` arm as `INVALID_ARCHIVE` (structural damage from `ZipArchive::new` / bad local headers).
4. Keep `ZipError::InvalidPassword` and `PASSWORD_REQUIRED` on the catch-all so existing retry text is unchanged.
5. Broaden the `MAYBE_BAD_PASSWORD` comment so it covers ZipCrypto as well as plain-header 7z.

`archive_read_error` already remaps `InvalidData` and `UnexpectedEof` when a password was supplied. That covers:

- CRC `"Invalid checksum"` (the reported bug)
- Deflate/garbage failures that can also follow a colliding ZipCrypto header

Optional hardening (only if a colliding password is observed failing at `by_index_with_options` as `ZipError::Io` rather than on read): pass `password_supplied` into `zip_error`’s Io arm from `extract_zip_from_archive`. `ZipArchive::new` in `archive.rs` must stay `password_supplied: false`. Do not remap `InvalidArchive` just because a password was supplied.

Do **not** change `events.rs` unless decoder mapping proves insufficient.

## Touch points

| File | Change |
| --- | --- |
| `src/adapters/local_operations/archive/decoders.rs` | Set ZIP `ArchiveReader.password_supplied`; comment on `MAYBE_BAD_PASSWORD`. Optional: `zip_error(..., password_supplied)` for Io only. |
| `src/adapters/local_operations/archive/decoders/tests.rs` | ZipCrypto collision + AES/unencrypted regressions. |
| `src/adapters/local_operations/archive/decoders/fixtures/` | Optional committed ZipCrypto zip if generation via `FileOptionsExt` is awkward; 7z used `content-encrypted.7z`. |
| `tests/e2e/scenarios/test_archive_errors.py` | ZipCrypto collision reopens Extract with “Invalid password”, not the damaged dialog. |
| `src/ui/browser/events.rs` | No change expected. |
| `src/adapters/local_operations/archive.rs` | No change expected (`ZipArchive::new` stays damaged-on-parse). |

## Risks

- **Ambiguous CRC with the correct password.** A genuinely corrupt ZipCrypto member extracted with the right password will show “The password may be incorrect.” instead of damaged. Same tradeoff as #751 for content-encrypted 7z; acceptable because ZipCrypto cannot tell them apart after the 1-byte check.
- **Collision is per archive.** Tests that hard-code `341` will flake. Search at fixture generation time (issue Python snippet, or zip crate equivalent). Bound the search (e.g. 4096 candidates); fail the test if none is found.
- **Partial writes.** A colliding password may write garbage before CRC fails. Existing `corrupt_members_are_removed_without_losing_completed_or_existing_files` covers session cleanup; the new test should assert the destination has no extracted member after the colliding attempt.
- **Inflate vs CRC.** Stored ZipCrypto typically fails CRC; deflated members may fail inflate first. Remapping all `InvalidData`/`UnexpectedEof` when a password was supplied covers both. Do not key only on the `"Invalid checksum"` string.

## Non-goals

- Implementing the fix in this staging commit (plan artifacts only).
- Changing ZipCrypto, the `zip` crate, or Strata’s AES ZIP writer.
- Changing 7z mapping, `INVALID_ARCHIVE` copy, or the `password`/`encrypt` heuristic.
- Treating checksum failures **without** a password as retryable.
- Empty-password UX (#793 already keeps the dialog with “Enter a password”).
- Refactors of `ExtractionSession`, progress, or extract-to destination picking.
