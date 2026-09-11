# Test cases: #796 ZipCrypto wrong-password CRC

Narrow verification for the plan in `plan.md`. Colliding ZipCrypto passwords are per archive; generate a fixture and **search** for a CRC collision instead of hard-coding a password.

Shared setup unless a case says otherwise:

1. Create a ZipCrypto archive equivalent to `echo 'hello from zipcrypto' > some.txt && zip -P zipsecret password.zip some.txt` (Info-ZIP, Python `zipfile.setpassword`, or `zip::unstable::write::FileOptionsExt::with_deprecated_encryption`).
2. Find a wrong password that passes the 1-byte header check and then fails CRC (issue Python loop over `0..4096`, or the zip crate equivalent). Call it `collision`. If none is found in that bound, fail the test rather than skip.
3. Keep a non-colliding wrong password such as `wrong` that the decoder reports as `provided password is incorrect`.

## 1. Colliding ZipCrypto password is retryable (decoder)

**Purpose:** The reported bug: CRC after a ZipCrypto header collision must not become `INVALID_ARCHIVE`.

**Setup:** ZipCrypto fixture + `collision` from shared setup. Empty destination directory.

**Steps:**

1. `extract_zip_from_archive` / `decode_fixture` with password `collision`.
2. Assert the error string is `The password may be incorrect.` (`MAYBE_BAD_PASSWORD`).
3. Assert the destination directory is empty (failed member not left behind).
4. Extract the same archive with `zipsecret`.
5. Assert `some.txt` contains `hello from zipcrypto`.

**Expected:** Collision is classified as a maybe-bad password; correct password still extracts.

## 2. Non-colliding wrong ZipCrypto password stays InvalidPassword

**Purpose:** Do not rewrite the common (~255/256) ZipCrypto failure path.

**Setup:** Same fixture. Password `wrong` (or any value that fails the header check).

**Steps:** Extract with that password.

**Expected:** Error string remains `provided password is incorrect`. Destination empty.

## 3. ZipCrypto with no password still prompts

**Purpose:** First Extract attempt (password `None`) is unchanged.

**Setup:** Same fixture.

**Steps:** Extract with `password: None`.

**Expected:** Error string still contains `Password required` (zip crate `PASSWORD_REQUIRED` / `UnsupportedArchive`). UI (case 8) shows the first Extract dialog.

## 4. AES ZIP wrong password stays InvalidPassword

**Purpose:** Strata-created encrypted ZIP (AES-256) must not start using the ZipCrypto CRC remap in a way that changes AES MAC failures.

**Setup:** Archive from `write_compression_fixture` / Strata ZIP + password `test-password` (existing decoder tests already do this).

**Steps:** Extract with password `wrong`.

**Expected:** `provided password is incorrect`. Correct password still extracts.

## 5. Checksum / damage without a password stays #638 wording

**Purpose:** Unencrypted or no-password checksum failures must not become retryable.

**Setup:** Use the existing stored-ZIP corrupt-member fixture pattern (`corrupt_members_are_removed_without_losing_completed_or_existing_files`) and/or `archive_read_error(InvalidData, false)`.

**Steps:** Extract with `password: None`.

**Expected:** `This file is not a valid archive or is damaged.` (`INVALID_ARCHIVE`). No password dialog in the UI equivalent (case 9).

## 6. Truncated / fake ZIP still damaged

**Purpose:** `ZipArchive::new` structural failures stay on the #638 path even after ZIP `ArchiveReader` learns `password_supplied`.

**Setup:** Existing `truncated_headers_have_clear_errors` and E2E `fake.zip`.

**Steps:** Extract here on `fake.zip` / truncated zip. No password.

**Expected:** Damaged-archive dialog. Existing `test_invalid_archive_reports_damage_and_allows_another_extraction` remains green.

## 7. Content-encrypted 7z regression

**Purpose:** #751 / #793 must keep working.

**Setup:** Bundled `content-encrypted.7z` (password `secret`).

**Steps:** Run `wrong_password_for_content_encrypted_7z_is_retryable` and E2E `test_wrong_extract_password_reopens_dialog_until_password_is_correct`.

**Expected:** Wrong password → `MAYBE_BAD_PASSWORD` / Extract reopens with “Invalid password”; `secret` extracts.

## 8. UI: colliding ZipCrypto password reopens Extract (E2E)

**Purpose:** End-to-end proof that `OperationFailed` does not show the damaged dialog for a ZipCrypto collision.

**Setup:** Generate ZipCrypto zip in the E2E fixture dir (Python `zipfile`); search for `collision`. Refresh the window.

**Steps:**

1. Right-click the archive → **Extract here**.
2. Confirm first Extract prompt (empty stays open with “Enter a password” — #793).
3. Type `collision` → **Extract**.
4. Assert the Extract dialog reopens with focused password field and label **Invalid password**.
5. Assert there is **no** “Unable to complete operation” dialog and **no** “This file is not a valid archive or is damaged.”
6. Type `zipsecret` → **Extract**.
7. Assert `some.txt` exists with the expected contents; progress dialog dismisses.

**Expected:** Collision is retryable in the same UI as a normal wrong password; correct password completes.

## 9. UI: damaged unencrypted ZIP does not reopen Extract

**Purpose:** Regression against treating all extract failures as password retries.

**Setup:** Existing `fake.zip` E2E case.

**Steps:** Extract here on `fake.zip`.

**Expected:** “Unable to complete operation” / “This file is not a valid archive or is damaged.” / Close. No Extract password dialog.

## Out of scope for this issue

- Empty extract password (covered by #793).
- 7z header-encrypted (`-mhe=on`) vs content-encrypted (covered by #751).
- Cancellation, unsafe paths, free-space preflight, AES encrypt-on-compress.
