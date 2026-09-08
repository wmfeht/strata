# Archive backend: exarch-core

This document supersedes the earlier plan to link the system `libarchive.so.13`.
Strata now uses [`exarch-core`](https://crates.io/crates/exarch-core) for
compress and extract. There is no libarchive FFI layer and no
`libarchive.so.13` `NEEDED` entry.

## Why

`exarch-core` is a safe-Rust archive library with built-in path-traversal,
zip-bomb, symlink, and hard-link checks. Replacing the C bindings removes the
unsafe inventory entry for archive support and drops the distribution
`libarchive` package from build and runtime requirements.

## Behaviour

- Create: ZIP, TAR, TAR.GZ. 7z creation is refused.
- Extract: those plus `.tar.xz`, `.tar.zst`, `.tar.bz2`, and 7z. RAR is not
  supported.
- Encrypted ZIP and 7z fail; there is no password prompt.
- Compression stages through a `0o600` tempfile, then publishes.
- Extraction quotas come from free space at the destination (64 MiB reserve),
  with member-count, path-depth, and compression-ratio limits.

## Build

`build.rs` no longer probes libarchive. Codec libraries (`liblzma`, `libbz2`)
may still be linked transitively by `xz2` / `bzip2` for tar.xz and tar.bz2.
