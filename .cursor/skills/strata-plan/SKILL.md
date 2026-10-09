---
name: strata-plan
description: use when starting a Strata issue pipeline — investigate the issue and draft architecture + narrow verification tests before coding.
---

# Strata plan

Investigate a Strata GitHub issue and write a plan plus a **narrow** verification set. Do not implement the fix.

## Argument

The user or coordinator passes an **issue number or GitHub issue URL** (for example `655` or `https://github.com/lgse/strata/issues/655`).

Parse `<n>` as that issue number. After a staging PR exists, the coordinator may switch the working-docs folder to the PR number; use the folder already recorded in `working-docs/*/status.md` if one exists.

## Do this

1. **Read the issue.** Fetch the GitHub issue (body, labels, linked PRs, comments) with `gh` or the GitHub MCP. Follow linked discussions. Note version, install method, environment, and repro steps when present.
2. **Read project rules.** `AGENTS.md`, `CONTRIBUTING.md`, and any relevant `docs/` (especially `docs/preferences.md` for settings, `docs/architecture.md` for subsystem context). Honor Cursor Cloud notes in `.cursor/CLOUD.md` when running in that environment.
3. **Read the relevant code.** Locate the modules, tests, and callers that implement the reported behavior. Prefer in-repo search over speculation.
4. **Research the class of problem.** Collect:
   - In-repo precedents (similar fixes, test patterns, preference/view bindings).
   - Sensible external patterns (GTK/GIO/Rust) only when they apply.
   - Constraints: GTK 4 baseline, theme tokens, Lucide icons, test layout (`src/.../tests.rs` vs `tests/`).
5. **Draft a coherent approach** covering: goal, constraints, touch points, risks, and **non-goals**. Keep the design the smallest change that fixes the issue.
6. **Design a narrow test set** (happy path, key edges, likely regressions). Focused, not exhaustive permutations. Each case must be executable later by QA or targeted automated tests.
7. **Write artifacts** under `working-docs/<n>/` (create the directory):

   | File | Contents |
   | --- | --- |
   | `plan.md` | Architecture, research notes, approach, constraints, risks, non-goals |
   | `test-cases.md` | Numbered cases: purpose, setup, steps, expected |
   | `status.md` | Pipeline stage, recommended branch, what is ready |

8. **Stop.** Do not implement. Do not run a drive-by refactor. Do not open or merge PRs unless the coordinator already created a **draft staging PR**. If none exists, record a recommended branch name in `status.md` using `<type>/<issue-number>-<short-kebab-description>` (see `AGENTS.md`).

## `status.md` for this stage

Set:

- `stage`: `plan complete; ready for code`
- `issue`: URL
- `staging_pr`: URL or `none`
- `recommended_branch`: if no staging PR yet
- `head_sha`: `n/a` (no code commit yet) or the current staging SHA if a PR already exists
- `agent_id`: this agent / run id if known

## Guardrails

- Do not change product/runtime code in this skill.
- Do not undraft, squash, or post the pipeline’s final public PR summary (that is `strata-cleanup`).
- Do not treat chat as the source of truth; persist the plan in `working-docs/<n>/`.
