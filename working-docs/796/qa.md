# QA: #796 round 1

- PR: https://github.com/lgse/strata/pull/798 (draft)
- Branch: `fix/796-zipcrypto-wrong-password-crc`
- Head SHA tested: `9e59413dedd6806cf4ba097e98412e5ba7fc262b`
- Product fix: `df669505fae5bde72a8d1f835d787722a349ddd5` (status SHA `a6666015ce265f7303f484144b6dfdc4261aa4c9`)
- Agent id: `bc-ec8afd9e-8ca4-476d-8c0e-01782d5d5676`

## Verdict

`fail`

The issue repro (tiny Info-ZIP `zip -P` → stored member → CRC `Invalid checksum`) is retryable as `MAYBE_BAD_PASSWORD`. Planned stored unit/E2E cases, AES, no-password, non-colliding `InvalidPassword`, unencrypted/truncated damage, and 7z retry all passed.

Deflated ZipCrypto header collisions still stringify as `INVALID_ARCHIVE`. That is the same user-visible dialog as #796 for the common `zip -P` path (Info-ZIP deflates compressible members). Route back to `strata-code`. No product patch in this QA round.

## Coverage

Private Xvfb / E2E container only. Never `DISPLAY=:1`.

| Case | Result | How |
| --- | --- | --- |
| 1. Colliding ZipCrypto is retryable (decoder) | pass | `zipcrypto_header_collision_is_retryable`; tiny `zip -P` CRC probe |
| 2. Non-colliding wrong password stays `InvalidPassword` | pass | `zipcrypto_wrong_password_stays_invalid_password` |
| 3. No password still prompts | pass | `zipcrypto_without_password_still_requires_one` |
| 4. AES ZIP wrong password stays `InvalidPassword` | pass | `aes_zip_wrong_password_stays_invalid_password` |
| 5. Checksum/damage without a password stays #638 | pass | `unencrypted_zip_checksum_failure_stays_damaged`; `checksum_failure_without_a_password_remains_damaged` |
| 6. Truncated / fake ZIP still damaged | pass | `truncated_headers_have_clear_errors`; E2E `fake.zip` |
| 7. Content-encrypted 7z regression | pass | unit `wrong_password_for_content_encrypted_7z_is_retryable`; E2E `test_wrong_extract_password_reopens_dialog_until_password_is_correct` |
| 8. UI: colliding ZipCrypto reopens Extract | pass | container E2E `test_zipcrypto_collision_reopens_extract_dialog` (hand-rolled **stored** ZipCrypto) |
| 9. UI: damaged unencrypted ZIP does not reopen Extract | pass | container E2E `test_invalid_archive_reports_damage_and_allows_another_extraction` (`fake.zip`) |

Commands:

- `xvfb-run -a env -u WAYLAND_DISPLAY GDK_BACKEND=x11 GTK_A11Y=none NO_AT_BRIDGE=1 STRATA_REQUIRE_GTK_TESTS=1 cargo test --all-targets --all-features -- zipcrypto_ aes_zip_wrong_password_stays_invalid_password unencrypted_zip_checksum_failure_stays_damaged wrong_password_for_content_encrypted_7z_is_retryable checksum_failure_without_a_password_remains_damaged truncated_headers_have_clear_errors` — 8 passed
- `PATH=/tmp/docker-hostnet:$PATH STRATA_E2E_WORKERS=1 ./scripts/e2e.sh tests/e2e/scenarios/test_archive_errors.py -n 0` — 6 passed in 22.65s (image rebuild + 51.82s in-container `cargo build`)

## Findings by severity

### Product non-nits

1. **Deflated ZipCrypto collisions still report a damaged archive.** Plan said inflate-after-collision should follow the same `InvalidData` / `UnexpectedEof` remap. It does not.

   - zip crate `CompressionMethod::Deflated` + colliding password: member read is `ErrorKind::InvalidInput` `"corrupt deflate stream"` → `This file is not a valid archive or is damaged.`
   - Info-ZIP 3.0 `zip -P` on a compressible `some.txt` (`deflated 99%`): same `InvalidInput` / `INVALID_ARCHIVE`.
   - Tiny issue repro (`echo 'hello from zipcrypto'` → `stored 0%`) is `InvalidData` `"Invalid checksum"` → `The password may be incorrect.` (fixed).

   `archive_read_error` only remaps `InvalidData` / `UnexpectedEof` when `password_supplied`. `"corrupt deflate stream"` is already classified as gzip damage via `InvalidInput`, so a supplied ZipCrypto password never reaches `MAYBE_BAD_PASSWORD`. `OperationFailed` then shows the #638 dialog (no `password` / `encrypt`).

   Tests and E2E only cover stored ZipCrypto, so this hole is green in CI.

### Nits / process

- E2E `_write_zipcrypto` is still hand-rolled stored ZipCrypto (review nit). It cannot catch the deflate hole.
- Unused password on an unencrypted checksum-damaged ZIP remaps to `MAYBE_BAD_PASSWORD` because `password.is_some()` is archive-wide. Extract does not prompt for unencrypted ZIP, so this is not a user-visible extract path.
- Truncated ZipCrypto (`set_len(12)`) with a password still maps to `INVALID_ARCHIVE` via `ZipArchive::new` / `zip_error` (intended #638).
- AES ZIP with `password: None` still contains `Password required` (adjacent, pass).

## Gaps

- Full isolated `cargo test --all-targets --all-features` and full `./scripts/e2e.sh` were not re-run (targeted archive-error gate, same as the code round).
- No GTK `events.rs` unit test; UI coverage is the container E2E heuristic (`password` / `incorrect` → Extract + “Invalid password”).
- No two-window / preferences probe (decoder-only change).
- Python `zipfile.setpassword` on **write** does not encrypt in 3.12; the plan’s preferred generator still needs `ZipFile.open(..., pwd=)` / equivalent. Not used for classification.
- Throwaway deflate probes were not committed.

## Residual (accepted in plan, still true)

A genuinely corrupt **stored** ZipCrypto member extracted with the correct password would show “The password may be incorrect.” Same ZipCrypto/7z ambiguity as #751. Deflated corrupt members with a supplied password still show damaged today (the product finding above), not this residual.
