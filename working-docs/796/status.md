# Pipeline status

- issue: https://github.com/lgse/strata/issues/796
- staging_pr: https://github.com/lgse/strata/pull/798 (draft)
- staging_branch: fix/796-zipcrypto-wrong-password-crc
- folder: working-docs/796
- round: 1
- stage: round 1 review complete
- review_verdict: approve-with-comments
- qa_verdict: n/a
- head_sha: a6666015ce265f7303f484144b6dfdc4261aa4c9
- agent_id: bc-ad92cc78-4e0e-4763-a2d2-f98dd7aef72c
- notes: Round 1 review of a666601. ZIP `ArchiveReader.password_supplied` mirrors #751. No blockers. Nits are optional E2E writer / deflate coverage / collision-search robustness. Draft kept.

## History

- plan: complete (bc-699b814a-1f82-43f5-aaf5-e9e7a3e34f85, 2026-09-11) — investigated ZipCrypto CRC vs `INVALID_ARCHIVE`; proposed #751-style `ArchiveReader.password_supplied` for ZIP; no `events.rs` change expected on v0.15.0 (#793 heuristic).
- staging_pr: https://github.com/lgse/strata/pull/798 (draft)
- round 1 code: complete (bc-c3da1e3a-1ce9-4809-a069-cfd14f4f1696, 2026-09-11) — decoder flag + unit/E2E tests; targeted fmt/clippy/decoder tests + native `test_archive_errors.py`.
- round 1 review: complete (bc-ad92cc78-4e0e-4763-a2d2-f98dd7aef72c, 2026-09-11) — verdict `approve-with-comments` on a666601; no blockers; nits do not force another code round.
