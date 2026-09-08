# Linking the system libarchive

Strata's archive support currently builds its own copy of libarchive 3.8.1 from a vendored, patched `libarchive2-sys` (`vendor/libarchive2-sys`, ~150k lines of C) and drives it through the `libarchive2` crate. This document is the plan for replacing that with a small in-tree FFI layer that links the distribution's `libarchive.so.13` through `pkg-config`, removing the vendored tree, the `[patch.crates-io]` override, and the codec build and runtime dependencies that came with it.

## Why

The vendored build exists to work around one problem: upstream `libarchive2-sys` links `libxml2` unconditionally for XAR, and the libxml2 soname differs between the release builder (`libxml2.so.2` on Ubuntu 24.04) and Arch (`libxml2.so.16`). Building libarchive statically makes Strata responsible for every one of libarchive's dependencies, so the patch disables XAR and the release binary gains direct `NEEDED` entries for `libb2`, `libacl`, `libcrypto`, `liblz4`, `libzstd`, `liblzma`, `libbz2`, and `libz`.

Linking the system `libarchive.so.13` inverts that: libxml2, BLAKE2, ACL, and the codecs become libarchive's dependencies, resolved by each distribution's package manager. The soname is `libarchive.so.13` on Ubuntu 24.04, Arch, Debian, and Fedora, and it is a base dependency on Arch (pacman links it), so one release binary keeps working everywhere Strata already supports.

Secondary benefits, each of which addresses a finding from the libarchive branch review:

- Distribution security updates for libarchive apply to Strata without a release.
- Strata controls the format and filter allowlist; `support_format_all`/`support_filter_all` (which register RAR, LHA, CAB, ISO, WARC, mtree readers and the `lzop`/`lrzip`/`grzip` filters that `exec` external programs) go away.
- `ARCHIVE_WARN` from `archive_read_next_header` can be treated as a warning instead of a hard failure, which the `libarchive2` crate cannot do.
- Build requirements drop back to `libarchive-dev` plus `pkg-config`; `cmake`, `clang`, `libclang`, and eight `-dev` codec packages are no longer needed.

## Goals and non-goals

Goals:

- Same user-visible archive behaviour as the current branch: create ZIP, 7z, TAR, TAR.GZ; extract those plus `.tar.xz`, `.tar.zst`, `.tar.bz2`, and RAR; password prompt for encrypted ZIP; all existing zip-bomb limits.
- No vendored C. No `[patch.crates-io]`. No `bindgen`, `cmake`, or `clang` at build time.
- Release binary `NEEDED` set for archive support is exactly `libarchive.so.13`.
- Works against libarchive 3.7.2 (Ubuntu 24.04, the release builder and glibc floor) through 3.8.x (Arch).

Non-goals for this change:

- Fixing the encrypted-ZIP retry duplication, the `normalized_archive_name` panic, or the e2e `ids` mismatch. Those are separate fixes on the same branch and do not depend on how libarchive is linked.
- Supporting distributions with libarchive older than 3.7.2. The glibc 2.39 floor already excludes them.
- Restoring password-protected archive creation or encrypted 7z extraction. libarchive cannot do either regardless of linkage.

## Design

### Runtime linkage

`build.rs` probes for libarchive and emits the link line:

```rust
pkg_config::Config::new()
    .atleast_version("3.7.2")
    .probe("libarchive")
    .expect("libarchive >= 3.7.2 development files are required (libarchive-dev / libarchive)");
```

`pkg-config` becomes a `[build-dependencies]` entry (it is already in `Cargo.lock` transitively). On the aarch64 release job the runner is native `ubuntu-24.04-arm`, so no cross `PKG_CONFIG_SYSROOT_DIR` handling is required.

### API floor

Every function Strata calls must exist in libarchive 3.7.2. The current `libarchive2` crate references two that do not, `archive_entry_uid_is_set` and `archive_entry_gid_is_set` (added in 3.7.3), which is one reason the plan replaces the crate rather than only its `-sys` layer. The inventory below was checked against the 3.7.2 headers. `archive_read_support_format_rar5` (3.4), `archive_read_has_encrypted_entries` and `archive_entry_is_data_encrypted` (3.2), and the `_utf8` string accessors (3.1) are all well inside the floor.

### Module layout

```
src/adapters/local_operations/archive/
  libarchive.rs        safe wrapper: ReadArchive, WriteArchive, ArchiveMember (exists today, ported)
  libarchive/ffi.rs    extern "C" declarations, opaque types, constants (new)
```

`ffi.rs` is the only file that declares foreign functions. `libarchive.rs` is the only file that calls them, and it exposes the same safe surface `archive.rs` uses today (`ReadArchive::open`, `next_member`, `skip_data`, `Read`, `WriteArchive::create`, `write_*`, `finish`, `Write`), so `archive.rs` should need only the small changes listed under "Error typing".

`struct archive` and `struct archive_entry` are opaque in libarchive's ABI. Declare them as zero-sized, non-constructible types and keep the wrappers `!Send + !Sync` via `PhantomData<*mut ()>`. Each archive handle is created, used, and freed on one `gio::spawn_blocking` worker, which is the existing contract.

### Function inventory

Read side:

| Purpose | Functions |
| --- | --- |
| Lifecycle | `archive_read_new`, `archive_read_free` |
| Formats (allowlist) | `archive_read_support_format_zip`, `_7zip`, `_tar`, `_rar`, `_rar5` |
| Filters (allowlist) | `archive_read_support_filter_gzip`, `_bzip2`, `_xz`, `_zstd` |
| Open | `archive_read_add_passphrase`, `archive_read_open_fd` |
| Iterate | `archive_read_next_header`, `archive_read_data`, `archive_read_data_skip`, `archive_read_has_encrypted_entries` |
| Errors | `archive_errno`, `archive_error_string` |

Entry accessors (read):

`archive_entry_pathname_utf8` with `archive_entry_pathname` fallback, `archive_entry_filetype`, `archive_entry_size`, `archive_entry_size_is_set`, `archive_entry_mtime`, `archive_entry_mtime_is_set`, `archive_entry_symlink_utf8`/`archive_entry_symlink`, `archive_entry_hardlink_utf8`/`archive_entry_hardlink`, `archive_entry_is_encrypted`, `archive_entry_is_data_encrypted`, `archive_entry_is_metadata_encrypted`.

Write side:

| Purpose | Functions |
| --- | --- |
| Lifecycle | `archive_write_new`, `archive_write_close`, `archive_write_free` |
| Formats | `archive_write_set_format_zip`, `_7zip`, `_pax_restricted` |
| Filters | `archive_write_add_filter_gzip`, `_none`; `_xz`, `_zstd`, `_bzip2` under `#[cfg(test)]` for the extract-only round-trip test |
| Options | `archive_write_set_format_option(a, "zip", "compression", "deflate")` |
| Open and write | `archive_write_open_fd`, `archive_write_header`, `archive_write_data` |

Entry construction (write):

`archive_entry_new`, `archive_entry_free`, `archive_entry_set_pathname_utf8`, `archive_entry_set_filetype`, `archive_entry_set_perm`, `archive_entry_set_size`, `archive_entry_set_symlink_utf8`, `archive_entry_set_hardlink_utf8`, `archive_entry_set_mtime`.

Constants: `ARCHIVE_EOF` (1), `ARCHIVE_OK` (0), `ARCHIVE_RETRY` (-10), `ARCHIVE_WARN` (-20), `ARCHIVE_FAILED` (-25), `ARCHIVE_FATAL` (-30); `AE_IFMT` and the `AE_IF*` file types; `ARCHIVE_READ_FORMAT_ENCRYPTION_UNSUPPORTED` (-2) and `_DONT_KNOW` (-1). Copy the values from `archive.h`/`archive_entry.h` with a comment naming the header; they are ABI constants and have not changed.

Roughly 50 functions total. Nothing else from the two headers is declared.

### Return-code handling

The `libarchive2` crate treats every negative return as an error. The new wrapper distinguishes them:

- `archive_read_next_header`: `ARCHIVE_EOF` ends iteration; `ARCHIVE_OK` yields the entry; `ARCHIVE_WARN` yields the entry and logs `archive_error_string` at `tracing::warn!` with the archive path; `ARCHIVE_RETRY` retries; `ARCHIVE_FAILED`/`ARCHIVE_FATAL` return `DecoderError`.
- `archive_read_data`: non-negative is a byte count (0 is end of entry); negative is an error. A bad CRC or a truncated stream surfaces here and must stay an error.
- `archive_read_data_skip`, `archive_write_header`, `archive_write_close`: `ARCHIVE_WARN` logs and continues; anything lower is an error.
- `archive_write_data`: non-negative is bytes accepted; negative is an error. Keep the existing "accepted no data" guard in `Write::write`.

Put the mapping in one pure function (`fn status(code: c_int) -> Status`) so it can be unit-tested without an archive.

### Error typing

`DecoderError` gains the libarchive `code` and keeps `message` and `encrypted`. The `Read` impl for `ReadArchive` wraps `DecoderError` in `io::Error::other`, and `copy_with_big_buf`'s caller in `archive.rs` downcasts (`error.get_ref().and_then(|e| e.downcast_ref::<DecoderError>())`) instead of matching on the string `"libarchive error"`. This is the one behavioural touch to `archive.rs`; the message-based `classify_encrypted_failure` heuristics stay as they are.

### Locale

libarchive converts member names through the process `LC_CTYPE`. Rust binaries start in the `C` locale; GTK sets the user's locale at `gtk_init`, but tests and any non-GTK caller do not, and a user session with `LANG=C` breaks non-ASCII names in every case. The `libarchive2` crate papered over this with a thread-local `uselocale` guard bound to the environment locale (`""`), which is why extraction fails under a C locale today.

The wrapper installs its own guard around `archive_read_next_header`, `archive_write_header`, and the `set_*_utf8` calls: `newlocale(LC_CTYPE_MASK, "C.UTF-8", null)`, falling back to `""` if `C.UTF-8` is unavailable, applied with `uselocale` and restored on drop. `uselocale` is per-thread, so this cannot disturb GTK on the main thread. This uses `libc::{newlocale, uselocale, freelocale}` and adds `libc` as a direct dependency (already in the lockfile).

### Compatibility with the distribution's build options

Ubuntu 24.04's `libarchive13t64` and Arch's `libarchive` are both built with zlib, bzip2, xz, zstd, lz4, and libxml2, and RAR5 uses libarchive's bundled BLAKE2 when `libb2` is absent. Nothing Strata enables depends on an optional build flag, but the codec round-trip tests (`formats_round_trip`, `extract_only_round_trip`) are the guard if a distribution ships a reduced build.

## Unsafe code policy

`docs/unsafe-code.md` denies `unsafe_code` workspace-wide and requires `#[expect(unsafe_code, reason)]`, a `SAFETY:` comment, and one unsafe operation per block. The FFI layer has to comply, and its justification under requirement 1 ("a required capability is unavailable through a suitable maintained safe API") is:

- `libarchive2`, the only maintained safe wrapper that both reads and writes, hard-depends on a `-sys` crate that vendors libarchive and links libxml2 unconditionally; its constructors cannot allowlist formats, cannot open a descriptor after `add_passphrase`, and cannot distinguish `ARCHIVE_WARN`.
- `compress-tools` links the system library but is extract-only.

Concretely:

- `ffi.rs` holds the `extern "C"` block with a module-level `#[expect(unsafe_code, reason = "...")]` and a header comment naming the libarchive version the declarations were checked against.
- Every call site in `libarchive.rs` is its own `unsafe { }` block with a `SAFETY:` comment covering: pointer non-null (checked after `*_new`), the `CString` outliving the call, string pointers returned by libarchive being valid only until the next `archive_read_next_header` (copy immediately), the fd outliving the archive (the wrapper owns the `File`), and single-thread use.
- `Drop` for `ReadArchive`/`WriteArchive` calls `archive_read_free`/`archive_write_free` exactly once; `WriteArchive::finish` takes the handle out of an `Option` so `Drop` does not double-free.
- Add a "libarchive bindings" entry to the "Current inventory" section of `docs/unsafe-code.md`, mirroring the Fontconfig entry.

Expect the review of this part to be the slowest step; budget for it.

## Implementation steps

Each step should leave the tree building and the archive tests passing so the work can land as a short series of commits or one reviewable PR with clean history.

### 1. Add the FFI layer alongside the crate

- Add `libc` to `[dependencies]` and `pkg-config` to `[build-dependencies]`.
- Extend `build.rs` with the `pkg_config` probe (keep the existing gresource and git-metadata logic).
- Create `src/adapters/local_operations/archive/libarchive/ffi.rs` with the inventory above, the opaque types, constants, and the locale guard.
- Add unit tests for the pure pieces: `status()` mapping, `AE_IF*` to `MemberKind`, `has_encrypted_entries` result mapping.

### 2. Port `libarchive.rs` off `libarchive2`

- Replace `LibReadArchive`/`LibWriteArchive`/`EntryMut` usage with the `ffi` calls, keeping the public `pub(super)` signatures unchanged.
- `ReadArchive::open`: `archive_read_new` → format/filter allowlist → optional `archive_read_add_passphrase` → `archive_read_open_fd`. This removes the UTF-8-path restriction on the passphrase path (the crate's `open_with_passphrase` was path-based).
- `ArchiveMember.size` and `.mtime` become `None` when `*_is_set` is 0 instead of `Some(0)`.
- Implement the `Read`/`Write` impls and `DecoderError` per "Return-code handling" and "Error typing".
- Update `archive.rs`'s `copy_with_big_buf` error branch to downcast instead of string-match.
- Run `cargo test --bin strata -- adapters::local_operations::archive`; all 40 existing tests must pass unchanged.

### 3. Remove the crate, the vendored tree, and the patch

- `Cargo.toml`: remove `libarchive2` and the `[patch.crates-io]` section.
- Delete `vendor/libarchive2-sys/`.
- `typos.toml`: drop `vendor/**`.
- `deny.toml`: drop `BSD-2-Clause` if `cargo deny check licenses` no longer needs it.
- `cargo update -p libarchive2 --precise` is not needed; regenerate `Cargo.lock` by building.

### 4. Revert the dependency lists

| File | Change |
| --- | --- |
| `.github/workflows/ci.yml`, `.github/workflows/release.yml` | Replace `cmake clang libclang-dev zlib1g-dev libbz2-dev liblzma-dev libzstd-dev liblz4-dev libssl-dev libacl1-dev libb2-dev` with `libarchive-dev` |
| `tests/e2e/Dockerfile` | Same substitution |
| `scripts/dev.sh` | `pkg-config --exists 'libarchive >= 3.7.2'` in the check; drop the `cmake`/`clang` checks and the `libb2`/`libcrypto`/`liblzma`/`zlib` probes; pacman `libarchive`, apt `libarchive-dev`, dnf `libarchive-devel` |
| `install.sh` | `REQUIRED_PACKAGES`: replace `acl bzip2 libb2 lz4 openssl xz zlib zstd` with `libarchive` |
| `packaging/aur/PKGBUILD.in` | `depends`: replace the eight codec packages with `libarchive`; regenerate `strata-bin/.SRCINFO` and `strata-rc-bin/.SRCINFO` per `docs/packaging.md` |
| `README.md` | Runtime and build dependency lists in both Arch snippets; "UI and runtime" row: "libarchive (system, ≥ 3.7.2)" |
| `CONTRIBUTING.md` | Development setup sentence and pacman line |
| `docs/architecture.md` | "Archives" section: describe system linkage, the format/filter allowlist, and `ARCHIVE_WARN` handling; remove the vendoring paragraph |
| `docs/packaging.md` | Replace the "Release archives are built on Ubuntu 24.04. `libarchive2-sys` vendors…" paragraph with the `libarchive.so.13` soname note |
| `docs/unsafe-code.md` | Add the inventory entry |

### 5. Verify

- `cargo clippy --all-targets --all-features -- -D warnings` (unsafe lints included).
- `cargo test --all-targets --all-features` on Arch (libarchive 3.8.x) and inside the e2e Docker image (Ubuntu 24.04, 3.7.2). The second run is the API-floor check; a missing symbol fails at link time.
- `readelf -d target/release/strata | grep NEEDED` contains `libarchive.so.13` and none of `libxml2`, `libb2`, `libacl`, `libcrypto`, `liblz4`, `libzstd`, `liblzma`, `libbz2`. Consider adding this as a release-workflow assertion so a future dependency cannot reintroduce a distro-specific soname silently.
- New tests: a ZIP with a non-ASCII member name extracts with the correct name when the test process runs with `LC_ALL=C` (exercises the locale guard), and `status()` maps `ARCHIVE_WARN` on headers to continue.
- e2e `test_archive.py` round trip passes in the Docker image.

## Risks

- **Behaviour tracks the distribution's libarchive.** A bug fixed in 3.8.x is present for Ubuntu users until Ubuntu updates. This is the same position every other libarchive consumer on the system is in, and the alternative (static vendoring) means Strata receives no fixes at all without a release.
- **ABI drift.** Strata uses only opaque pointers and functions; no struct layouts. libarchive has kept `libarchive.so.13` since 3.0 and adds functions without removing them, so the risk is a symbol Strata uses disappearing, which the 3.7.2 test run guards against.
- **Unsafe surface.** About 50 `extern` declarations and roughly as many call sites is a real increase over the current three Fontconfig calls. Mitigation is the isolation described above and keeping the extern block minimal; do not declare functions "for later".
- **Distribution builds without a codec.** Unlikely for the mainstream distributions Strata targets; the codec round-trip tests fail loudly if it happens.

## Rollback

The change is self-contained: reverting the PR restores the vendored build. Nothing on disk or in user data depends on which libarchive was linked.

## Acceptance checklist

- [ ] `vendor/` is gone and `Cargo.toml` has no `[patch.crates-io]`.
- [ ] `cargo build` needs `pkg-config` and `libarchive-dev`/`libarchive` only; no `cmake`, `clang`, or codec `-dev` packages.
- [ ] Release binary `NEEDED` contains `libarchive.so.13` and no codec or libxml2 sonames.
- [ ] All archive unit tests pass on libarchive 3.7.2 and 3.8.x.
- [ ] Format/filter allowlist is explicit; no `support_*_all`.
- [ ] `ARCHIVE_WARN` on headers continues with a logged warning.
- [ ] Non-ASCII member names extract correctly under `LC_ALL=C`.
- [ ] `docs/unsafe-code.md` inventory updated; every unsafe block has a `SAFETY:` comment and passes the workspace lints.
- [ ] PKGBUILD, `.SRCINFO`, `install.sh`, `scripts/dev.sh`, CI, Dockerfile, README, CONTRIBUTING, and architecture/packaging docs reflect the single `libarchive` dependency.
