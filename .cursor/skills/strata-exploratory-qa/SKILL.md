---
name: strata-exploratory-qa
description: use for exploratory/functional QA of a Strata draft staging PR — verify the fix and check adjacent regressions.
---

# Strata exploratory QA

Verify the draft staging PR against the planned cases, then probe adjacent regressions. Functional/exploratory, not a second code review.

## Required reads

- `working-docs/<n>/plan.md`
- `working-docs/<n>/test-cases.md`
- Latest `working-docs/<n>/review.md`
- `working-docs/<n>/status.md` (head SHA, staging PR)

## Do this

1. Check out or otherwise exercise the **same head SHA** recorded in `status.md` / the draft PR.
2. Execute or probe each numbered case in `test-cases.md` (happy path, key edges, regressions). Record pass/fail per case.
3. Do **exploratory** checks around the change: adjacent views, callers, preferences, the other window if the bug is app-wide, empty/error states called out in the plan.
4. Follow environment rules: never drive GTK/E2E against the Cloud Agent desktop (`DISPLAY=:1`). Use private Xvfb, the pinned `./scripts/e2e.sh` container, or documented native debug helpers as appropriate.
5. Classify findings:
   - **Product non-nits:** the fix fails a planned case, or an adjacent regression that users would hit.
   - **Nits / process:** wording, coverage gaps that are not product breaks, test-plan ambiguity.

## Write

`working-docs/<n>/qa.md`:

- Verdict (`pass` / `pass-with-nits` / `fail`)
- Coverage (which cases ran, which were skipped and why)
- Findings by severity (product non-nits vs nits/process)
- Gaps (what you could not exercise)
- Agent id
- Head SHA tested

Update `status.md`: stage `round N QA complete`, QA verdict, head SHA, agent id.

## Guardrails

- Do not file GitHub issues unless the coordinator says so.
- Keep the PR **draft**. Do not undraft, squash, cleanup, or merge.
- Do not implement fixes (route product non-nits back to `strata-code`).
- Do not post the pipeline’s final public PR summary (`strata-cleanup`).
