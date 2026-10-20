# Strata issue development pipeline (coordinator)

Chief of Staff (CoS) orchestration for a single GitHub issue. CoS **routes** agents; it does not implement the fix inline.

## Flow

```
plan → (code → code-review → exploratory-qa)×≤3 → cleanup
```

Skills:

| Stage | Skill |
| --- | --- |
| Plan | `strata-plan` |
| Code (or fix round) | `strata-code` |
| Review | `strata-code-review` |
| QA | `strata-exploratory-qa` |
| Cleanup | `strata-cleanup` |

Rounds **2–4** in informal wording are the **code / review / QA** loop iterating **up to 3 times**. Round 1 is the first code pass after plan.

## Source of truth

The draft PR is staging plus the eventual public comment chain.

**Stage coordination artifacts live in `working-docs/<pr#>/`** (not ad-hoc chat-only). Until a staging PR exists, use the **issue number** as `<n>`. Once a PR exists, prefer the **PR number** and record it in `status.md`. Do not keep a second parallel folder.

## After plan

1. Run `strata-plan` with the issue number or URL.
2. **Create or ensure a draft staging PR** on a branch named `<type>/<issue-number>-<short-kebab-description>` from latest `main`.
3. Then run `strata-code` against that draft PR.

If plan recommended a branch and no PR exists yet, the CoS creates the draft PR before coding.

## Between rounds

- If **review** is `request-changes` **or** QA has **product non-nit** findings → another `strata-code` round, then review + QA again.
- **Nits / suggestions do not force another round.**
- **Stop early** on clean `approve` (or `approve-with-comments`) **and** QA `pass` / `pass-with-nits` with **no** non-nit product findings.
- Maximum **3** code/review/QA cycles. If still blocked, stop and report; do not silently start a fourth.

## Cleanup

When the loop stops (success or coordinator halt):

1. Run `strata-cleanup` (squash, remove `working-docs/<n>/`, post **one** final PR comment).
2. Leave the PR draft unless the coordinator says undraft.

## Launch target (cloud agents)

Cloud agents for `lgse/strata` may need a **`wmfeht/strata` launch workaround** (start the agent on the fork when the org repo cannot be the launch target). Still **target `lgse/strata` draft PRs** for comments, status, and the staging PR the pipeline will undraft later.

Working tree and branch: implement on the staging branch that backs the lgse draft PR (push to the fork head that PR uses).

## CoS rules

- Route; do not implement, review, or QA in the same turn as coordination unless a skill is missing and the coordinator must record a blocker.
- Pass `<n>`, PR URL, round number, and head SHA into each agent prompt.
- Do not squash or delete working-docs except via `strata-cleanup`.
- Do not undraft during plan/code/review/QA.

## Sample `working-docs/<n>/status.md`

```markdown
# Pipeline status

- issue: https://github.com/lgse/strata/issues/NNN
- staging_pr: https://github.com/lgse/strata/pull/NNN (draft)
- staging_branch: type/NNN-short-kebab
- folder: working-docs/NNN
- round: 1
- stage: plan complete; ready for code
- review_verdict: n/a
- qa_verdict: n/a
- head_sha: n/a
- agent_id: n/a
- notes: recommended branch type/NNN-short-kebab; no staging PR yet

## History

- plan: complete (<agent>, <date>)
- round 1 code: pending
```

Update this file at the end of every skill. Append history; do not delete prior rounds.
