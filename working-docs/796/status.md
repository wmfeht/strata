# Pipeline status

- issue: https://github.com/lgse/strata/issues/796
- staging_pr: https://github.com/lgse/strata/pull/798 (draft)
- staging_branch: fix/796-zipcrypto-wrong-password-crc
- folder: working-docs/796
- round: 1
- stage: round 1 code complete
- review_verdict: n/a
- qa_verdict: n/a
- head_sha: 017b7ad8c57c5023c8c0f1d99724cd315621c841
- agent_id: bc-c3da1e3a-1ce9-4809-a069-cfd14f4f1696
- notes: ZIP `ArchiveReader.password_supplied` mirrors #751. Colliding ZipCrypto CRC maps to `MAYBE_BAD_PASSWORD`. No `events.rs` change. Draft kept.

## History

- plan: complete (bc-699b814a-1f82-43f5-aaf5-e9e7a3e34f85, 2026-09-11) — investigated ZipCrypto CRC vs `INVALID_ARCHIVE`; proposed #751-style `ArchiveReader.password_supplied` for ZIP; no `events.rs` change expected on v0.15.0 (#793 heuristic).
- staging_pr: https://github.com/lgse/strata/pull/798 (draft)
- round 1 code: complete (bc-c3da1e3a-1ce9-4809-a069-cfd14f4f1696, 2026-09-11) — decoder flag + unit/E2E tests; targeted fmt/clippy/decoder tests + native `test_archive_errors.py`.
