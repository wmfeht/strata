# Pipeline status

- issue: https://github.com/lgse/strata/issues/796
- staging_pr: https://github.com/lgse/strata/pull/798 (draft)
- staging_branch: fix/796-zipcrypto-wrong-password-crc
- folder: working-docs/796
- round: 1
- stage: round 1 QA complete
- review_verdict: approve-with-comments
- qa_verdict: fail
- head_sha: 9e59413dedd6806cf4ba097e98412e5ba7fc262b
- agent_id: bc-ec8afd9e-8ca4-476d-8c0e-01782d5d5676
- notes: Round 1 QA of 9e59413. Planned stored CRC / AES / 7z / #638 cases passed (decoder 8 + container E2E archive_errors 6). Fail: deflated ZipCrypto collisions (`InvalidInput` corrupt deflate stream, including Info-ZIP `zip -P` on compressible files) still map to INVALID_ARCHIVE. Draft kept. No product patch.

## History

- plan: complete (bc-699b814a-1f82-43f5-aaf5-e9e7a3e34f85, 2026-09-11) — investigated ZipCrypto CRC vs `INVALID_ARCHIVE`; proposed #751-style `ArchiveReader.password_supplied` for ZIP; no `events.rs` change expected on v0.15.0 (#793 heuristic).
- staging_pr: https://github.com/lgse/strata/pull/798 (draft)
- round 1 code: complete (bc-c3da1e3a-1ce9-4809-a069-cfd14f4f1696, 2026-09-11) — decoder flag + unit/E2E tests; targeted fmt/clippy/decoder tests + native `test_archive_errors.py`.
- round 1 review: complete (bc-ad92cc78-4e0e-4763-a2d2-f98dd7aef72c, 2026-09-11) — verdict `approve-with-comments` on a666601; no blockers; nits do not force another code round.
- round 1 QA: complete (bc-ec8afd9e-8ca4-476d-8c0e-01782d5d5676, 2026-09-11) — verdict `fail` on 9e59413; stored CRC path and planned cases pass; deflated `zip -P` collisions still damaged.
