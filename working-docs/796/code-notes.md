# Code notes: #796 round 2

## Change

`archive_read_error` now remaps `ErrorKind::InvalidInput` (as well as `InvalidData` / `UnexpectedEof`) to `MAYBE_BAD_PASSWORD` when `password_supplied` is true. Deflated ZipCrypto header collisions fail inflate as `InvalidInput` `"corrupt deflate stream"`; that used to fall through to the gzip damage arm and stringify as `INVALID_ARCHIVE`.

Stored ZipCrypto CRC (`Invalid checksum` / `InvalidData`) stays retryable. No-password gzip/deflate damage still uses the #638 wording because gzip/TAR `ArchiveReader` keeps `password_supplied: false`. `zip_error`, `ZipArchive::new`, and `events.rs` are unchanged.

## Files

- `src/adapters/local_operations/archive/decoders.rs` — include `InvalidInput` in the password-supplied remap.
- `src/adapters/local_operations/archive/decoders/tests.rs` — deflated ZipCrypto collision + direct corrupt-deflate mapping cases.
- `tests/e2e/scenarios/test_archive_errors.py` — parametrize Extract retry for stored and deflated ZipCrypto.

## Tests run

- `cargo fmt --all --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `xvfb-run -a env -u WAYLAND_DISPLAY GDK_BACKEND=x11 GTK_A11Y=none NO_AT_BRIDGE=1 STRATA_REQUIRE_GTK_TESTS=1 cargo test --all-targets --all-features -- adapters::local_operations::archive::decoders::tests` — 34 passed (includes stored + deflated ZipCrypto, AES, unencrypted checksum, 7z retry, truncated headers, corrupt-deflate mapping)
- `PATH=/tmp/docker-hostnet:$PATH STRATA_E2E_WORKERS=1 ./scripts/e2e.sh tests/e2e/scenarios/test_archive_errors.py -n 0` — 7 passed (stored + deflated ZipCrypto Extract reopen, 7z retry, `fake.zip` damage)

Did not run the full isolated suite or full `./scripts/e2e.sh` (targeted gate per strata-code).

## Gaps

- Full container E2E suite not run this round.
- E2E `_write_zipcrypto` is still a hand-rolled ZipCrypto writer (review nit); it now covers deflated members.
- Unused password on an unencrypted checksum-damaged ZIP still remaps to `MAYBE_BAD_PASSWORD` (not a user-visible Extract path).
