# Review: #796 round 1

- PR: https://github.com/lgse/strata/pull/798 (draft)
- Branch: `fix/796-zipcrypto-wrong-password-crc`
- Head SHA reviewed: `a6666015ce265f7303f484144b6dfdc4261aa4c9`
- Agent id: `bc-ad92cc78-4e0e-4763-a2d2-f98dd7aef72c`

## Verdict

`approve-with-comments`

The decoder change matches the plan. ZipCrypto CRC after a header collision maps to `MAYBE_BAD_PASSWORD`; `InvalidPassword`, no-password, AES, and unencrypted damage stay on their existing paths. No `events.rs` change. No blockers.

## Blockers

None.

## Nits / suggestions

- E2E `_write_zipcrypto` is a hand-rolled ZipCrypto stream cipher and ZIP headers instead of Python `zipfile.setpassword` (the plan’s preferred portable generator). Native E2E passed per code notes, and the PKWARE CRC-byte fixture (no data descriptor) is complementary to zip-crate unit fixtures (Info-ZIP data descriptor / time-byte check). Still more custom crypto than needed; `zipfile` would be smaller if this is touched again.
- Optional coverage: tests use stored ZipCrypto only. The production remap is all `InvalidData` / `UnexpectedEof` when a password was supplied, so deflated inflate-after-collision should already follow the same path. A deflated collision case is not required for this round.
- `zipcrypto_crc_collision` treats any `read_to_end` error as a hit and aborts the search on any non-`InvalidPassword` open error. Fine for the observed `Crc32Reader` path; continuing on unexpected `ZipError::Io` at `by_index_with_options` would make the helper slightly more robust.

## Notes

- Plan alignment: `extract_zip_from_archive` sets `password_supplied = password.is_some()` and constructs `ArchiveReader { inner, password_supplied }` the same way #751 did for 7z. `zip_error` still maps `InvalidArchive` to `INVALID_ARCHIVE` and leaves `InvalidPassword` / `PASSWORD_REQUIRED` on the catch-all. `ZipArchive::new` in `archive.rs` is unchanged.
- Optional `zip_error(..., password_supplied)` Io hardening was correctly skipped. ZipCrypto header rejection is `InvalidPassword`; CRC is `Crc32Reader` `Invalid checksum` (`ErrorKind::InvalidData`) on member read through `ArchiveReader`. Truncated header `read_exact` Io at open still stringifies as damaged, which is the #638 path.
- `MAYBE_BAD_PASSWORD` (“The password may be incorrect.”) already contains `password` and `incorrect`, so #793 reopens Extract with the inline helper. Confirmed `events.rs` is untouched.
- Test-cases.md: new decoder tests cover cases 1–5 and 7; new E2E covers case 8; cases 6 and 9 remain the existing truncated/`fake.zip` tests. Collision search is `0..4096` and fails if none is found. Destination is empty after a colliding extract.
- Accepted residual (plan): a genuinely corrupt ZipCrypto member extracted with the *correct* password will show “The password may be incorrect.” Same ZipCrypto/7z ambiguity as #751.
- CI on the draft is skipped except Metadata policy (success). Targeted decoder tests at this SHA: `zipcrypto_*`, AES wrong password, unencrypted checksum, 7z retry, truncated headers — 7 passed.
- Full `./scripts/e2e.sh` was not re-run (draft / targeted gate). Native `test_archive_errors.py` (6 passed) is recorded in code notes and does not replace the container pre-push gate.
