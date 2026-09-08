# Unsafe Code Policy

Strata is a safe-Rust codebase by default. Unsafe code is an exception for a narrow platform boundary, not a general implementation tool.

## Requirements

Unsafe code may be introduced only when all of the following are true:

1. A required capability is unavailable through a suitable maintained safe API.
2. Avoiding it would remove an intentional product capability or create a worse operational compromise.
3. The unsafe operation is isolated behind a small safe interface.
4. The unsafe block is reduced to the exact FFI or pointer operation that requires it.
5. Every block has a `SAFETY:` comment describing the concrete invariants that make it sound.
6. The containing item uses `#[expect(unsafe_code, reason = "...")]` with a specific reason.
7. Error paths, null pointers, ownership, lifetimes, threading, and retained-pointer behavior are considered explicitly.
8. The change receives focused review and appropriate tests or runtime validation.

Do not use `#[allow(unsafe_code)]`. `#[expect]` is intentional: the compiler reports the attribute if the unsafe operation is later removed, preventing stale exceptions.

## Automated enforcement

`Cargo.toml` configures the compiler and Clippy to:

- Deny unsafe code globally
- Deny unsafe operations hidden inside unsafe functions
- Deny unnecessary unsafe blocks
- Deny `#[allow(...)]` attributes
- Require reasons on lint overrides
- Require `SAFETY:` documentation on every unsafe block
- Require safety documentation for public unsafe functions
- Limit each unsafe block to one unsafe operation

CI runs Clippy with warnings treated as errors, so violations block merges.

## Current inventory

### Bundled font registration

Location: `src/assets.rs::register_application_fonts`

Reason: Fontconfig exposes application-private font registration through its C API, and the available safe wrapper does not expose that capability. Strata uses three small FFI calls during single-threaded startup and presents the rest of the application with a safe function.

The operations are individually scoped and document:

- How the Fontconfig configuration pointer is obtained and null-checked
- The lifetime and ownership of the C path string
- Fontconfig's path-copy behavior
- Why rebuilding the font set occurs before GTK/Pango creates the application font map

If a maintained safe API gains this capability, this exception should be removed.

### libarchive bindings

Location: `src/adapters/local_operations/archive/libarchive.rs` and `src/adapters/local_operations/archive/libarchive/ffi.rs`

Reason: libarchive exposes archive read and write only through its C API. The only maintained safe wrapper that both reads and writes (`libarchive2`) hard-depends on a `-sys` crate that vendors libarchive and links libxml2 unconditionally; its constructors cannot allowlist formats, cannot open a descriptor after `add_passphrase`, and cannot distinguish `ARCHIVE_WARN`. `compress-tools` links the system library but is extract-only.

The FFI layer declares only the functions Strata calls. The safe wrapper:

- Allowlists ZIP, 7z, TAR, RAR, and RAR5 with gzip, bzip2, xz, and zstd filters
- Creates, uses, and frees each handle on one worker thread (`!Send + !Sync`)
- Treats `ARCHIVE_WARN` on headers as a logged warning rather than a hard failure
- Copies libarchive strings before the next `archive_read_next_header`
- Owns the `File` whose descriptor outlives the archive handle
- Frees each handle exactly once (`WriteArchive::finish` takes it out of an `Option` so `Drop` does not double-free)

If a maintained safe API gains these capabilities against the system library, this exception should be removed.
