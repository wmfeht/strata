# Pipeline status

- issue: https://github.com/lgse/strata/issues/796
- staging_pr: https://github.com/lgse/strata/pull/798 (draft)
- staging_branch: fix/796-zipcrypto-wrong-password-crc
- folder: working-docs/796
- round: 1
- stage: plan complete; ready for code
- review_verdict: n/a
- qa_verdict: n/a
- head_sha: 22087fc9c4c1debebe1844c8b55abe1a5ce58238
- agent_id: bc-699b814a-1f82-43f5-aaf5-e9e7a3e34f85
- notes: Staging draft opened against lgse/strata `main` (`f4e2a9b`, v0.15.0). Plan artifacts only; no product fix. Fork PR wmfeht/strata#11 was closed in favor of this upstream draft. `head_sha`Branch tip after recording the PR is 22087fc9c4c1debebe1844c8b55abe1a5ce58238.

## History

- plan: complete (bc-699b814a-1f82-43f5-aaf5-e9e7a3e34f85, 2026-09-11) — investigated ZipCrypto CRC vs `INVALID_ARCHIVE`; proposed #751-style `ArchiveReader.password_supplied` for ZIP; no `events.rs` change expected on v0.15.0 (#793 heuristic).
- staging_pr: https://github.com/lgse/strata/pull/798 (draft)
- round 1 code: pending
