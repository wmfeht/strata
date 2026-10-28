# Cursor skills (Strata)

Project-local Cursor skills for the **issue development pipeline**. These live under `.cursor/skills/` and are distinct from lockfile-restored skills in `skills-lock.json` (installed into gitignored `.agents/skills/`).

## Pipeline skills

| Skill | When to use |
| --- | --- |
| [strata-plan](strata-plan/SKILL.md) | Start an issue: investigate, draft architecture, write a narrow test set. No implementation. |
| [strata-code](strata-code/SKILL.md) | Implement or fix on the draft staging PR after reading the plan. |
| [strata-code-review](strata-code-review/SKILL.md) | Private review of the draft staging PR; record verdict in working-docs. |
| [strata-exploratory-qa](strata-exploratory-qa/SKILL.md) | Exploratory/functional QA of the draft PR against the planned cases. |
| [strata-cleanup](strata-cleanup/SKILL.md) | End of pipeline: squash, drop temporary docs, post one PR comment. |

Coordinator (Chief of Staff) flow, round limits, `working-docs/<n>/` layout, and the cloud-agent fork workaround: **[strata-dev-pipeline/ORCHESTRATION.md](strata-dev-pipeline/ORCHESTRATION.md)**.
