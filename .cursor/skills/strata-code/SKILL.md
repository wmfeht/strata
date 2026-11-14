---
name: strata-code
description: use to implement (or fix) a Strata issue in a draft staging PR after reading the plan.
---

# Strata code

Implement (or fix) the issue on the **draft staging branch**. Keep the PR draft.

## Required reads (do not skip)

1. `working-docs/<n>/plan.md`
2. `working-docs/<n>/test-cases.md`
3. `working-docs/<n>/status.md` (to learn `<n>`, staging PR, round, head SHA)

If this is a **fix round** after review or QA, also read:

- `working-docs/<n>/review.md` (if present)
- `working-docs/<n>/qa.md` (if present)

Address **non-nit / non-suggestion** findings only, unless the coordinator explicitly says to take nits too.

If `plan.md` or `test-cases.md` is missing, stop and tell the coordinator to run `strata-plan` first.

## Do this

1. Confirm you are on the staging branch that backs the draft PR. Do not commit to `main`.
2. Implement the plan’s smallest fix. Follow Strata coding practices in `AGENTS.md`:
   - Adjacent unit tests (`src/.../tests.rs`), integration tests in `tests/`.
   - Preferences via `ThemeManager::bind_preference` / shared control bindings; settings pages must only edit preferences.
   - Themeable UI uses `@theme_*` tokens, never static hex/RGB.
   - New icons from Lucide, namespaced `strata-`, via `assets::primary_icon` / `assets::set_primary_icon`.
3. Add or extend tests that cover the narrow cases in `test-cases.md` when they belong in automation. Do not invent an exhaustive matrix.
4. Run **targeted** checks (fmt, clippy, relevant tests). In Cloud Agent / headless environments use a private Xvfb display and never `DISPLAY=:1`:

   ```bash
   cargo fmt --all --check
   cargo clippy --all-targets --all-features -- -D warnings
   xvfb-run -a env -u WAYLAND_DISPLAY GDK_BACKEND=x11 \
     GTK_A11Y=none NO_AT_BRIDGE=1 STRATA_REQUIRE_GTK_TESTS=1 \
     cargo test --all-targets --all-features -- <filter>
   ```

   Escalate to the full isolated suite and `./scripts/e2e.sh` only when `AGENTS.md` says the impact is broad, or the coordinator asks. Record what you ran in `code-notes.md`.
5. Keep the GitHub PR **draft**. Push the staging branch.
6. Update `working-docs/<n>/`:
   - `code-notes.md` — what changed, files, why, tests run, remaining gaps.
   - `status.md` — `round N code complete`, head SHA, agent id.

## Guardrails

- Do not mark the PR ready for review.
- Do not squash, rewrite history, or delete `working-docs/` (that is `strata-cleanup`).
- Do not post the final public summary comment on the PR (`strata-cleanup` owns that).
- Do not open extra PRs. Work on the existing draft staging PR.
- Do not merge.

## Round numbering

`N` is the current code/review/QA cycle (1–3). Read `status.md` and increment only when starting a new code pass after review/QA findings.
