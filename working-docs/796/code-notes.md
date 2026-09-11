# Code notes: #796 round 1

## Change

`extract_zip_from_archive` now sets `password_supplied = password.is_some()` and wraps each ZIP member in `ArchiveReader { inner, password_supplied }` instead of `ArchiveReader::new` (which always forced `false`). ZipCrypto CRC / inflate failures after a supplied password therefore go through `archive_read_error(..., true)` → `MAYBE_BAD_PASSWORD` (“The password may be incorrect.”), which already contains `password` and `incorrect` for the #793 Extract retry UI.

This is the same wiring #751 used for content-encrypted 7z. `zip_error` still maps `InvalidArchive` to `INVALID_ARCHIVE`, and `InvalidPassword` / `PASSWORD_REQUIRED` stay on the catch-all. `events.rs` was not changed.

The optional `zip_error(..., password_supplied)` Io hardening was not needed: colliding passwords fail on member read (`Crc32Reader` `Invalid checksum`), not at `by_index_with_options`.

## Files

- `src/adapters/local_operations/archive/decoders.rs` — ZIP `ArchiveReader.password_supplied`; broaden `MAYBE_BAD_PASSWORD` comment.
- `src/adapters/local_operations/archive/decoders/tests.rs` — ZipCrypto collision search + AES / no-password / unencrypted checksum cases.
- `tests/e2e/scenarios/test_archive_errors.py` — ZipCrypto collision reopens Extract with “Invalid password”.

## Tests run

- `cargo fmt --all --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `xvfb-run -a env -u WAYLAND_DISPLAY GDK_BACKEND=x11 GTK_A11Y=none NO_AT_BRIDGE=1 STRATA_REQUIRE_GTK_TESTS=1 cargo test --all-targets --all-features -- adapters::local_operations::archive::decoders::tests` — 31 passed
- `./scripts/e2e-native.sh tests/e2e/scenarios/test_archive_errors.py -n 0` — 6 passed (ZipCrypto collision, 7z retry, `fake.zip` damage)

Did not run the full isolated suite or `./scripts/e2e.sh` (targeted gate per strata-code).

## Gaps

- Full container E2E (`./scripts/e2e.sh`) not run this round.
- `events.rs` left unchanged; decoder mapping was sufficient.
- Native E2E does not replace the container pre-push gate if this branch is later marked ready.
