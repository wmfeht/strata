---
name: strata-code-review
description: use for standard private code review of a Strata draft staging PR in the pipeline.
---

# Strata code review

Private review of the **draft staging PR** for this pipeline. Lean: review the change against the plan, not the whole tree.

## Required reads

- The PR diff (focused on the change).
- `working-docs/<n>/plan.md` for intent.
- `working-docs/<n>/code-notes.md` and `status.md` when present.
- `working-docs/<n>/test-cases.md` to see what should be covered.

## How to review

1. Identify `<n>` and the staging PR from the coordinator prompt or `working-docs/*/status.md`.
2. Review the diff for correctness, safety, regressions, and plan alignment.
3. **CI:** if GitHub checks are green, assume they ran. Do not re-run full local CI unless the coordinator asks, or the PR is draft **and** CI was skipped/not running.
4. Classify every finding:
   - **Blocker (non-nit):** correctness, safety, data loss, security, broken invariant, missing coverage for a planned case that can fail in production.
   - **Nit / suggestion:** style, naming, optional tests, tidy-ups. These do **not** force another code round.
5. **Verdict:**
   - `approve` — no blockers.
   - `approve-with-comments` — nits/suggestions only.
   - `request-changes` — one or more blockers. Use this **only** for non-nit correctness/safety issues.

## Write

`working-docs/<n>/review.md`:

- Verdict
- Blockers vs nits/suggestions (separate lists)
- Notes (plan gaps, residual risk)
- Agent id
- Head SHA reviewed

Update `status.md`: stage `round N review complete`, verdict, head SHA, agent id.

## Guardrails

- Prefer recording findings in `working-docs/` (the coordinator may also mirror comments onto the PR).
- Do not squash, cleanup, undraft, or merge.
- Do not implement the fix (route blockers back to `strata-code`).
- Do not treat nits as `request-changes`.
