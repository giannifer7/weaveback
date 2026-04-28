# ChatGPT-5.5 Handoff

Repository: `/home/g4/_prj/weaveback`  
Branch: `main`

## Current state

- Worktree was clean at handoff time.
- Latest completed docs migration commit: `73abf4c` `Migrate macro language revision to wvb`
- Recent docs migration series also includes:
  - `be921f3` `Migrate planning docs to wvb`
  - `d505e22` `Migrate more docs to wvb`
  - `5937140` `Migrate macro planning docs to wvb`
  - `73abf4c` `Migrate macro language revision to wvb`

## What is true now

- Top-level `docs/` is fully migrated to canonical `.wvb` sources.
- Generated `.adoc` and `.md` outputs are tracked in place.
- No `docs/*.adoc` remain without a matching `docs/*.wvb` owner.
- Structural lint is active and should stay green.

## First checks to run

1. `git status --short`
2. `cargo run -p wb-tangle -- --force-generated`
3. `cargo run -p wb-query -- lint --strict`
4. `just docs`

## Working conventions

- Use `apply_patch` for manual edits.
- Prefer `rg` and `rg --files`.
- Do not revert unrelated changes.
- Keep progress updates concise and factual.
- When changing literate sources, verify generated outputs immediately.

## Recurring nuisance

Git has occasionally reported a stale `.git/index.lock`.

If that happens:

1. confirm no real git process is still using the index
2. remove the stale lock
3. retry `git add`, `git commit`, and `git push` in sequence

## Likely next useful work

- Continue broader complexity-reduction and `.wvb` migration in crate-level or project-level literate sources.
- Review remaining non-doc areas for one-source-of-truth consistency.
- Keep source/generated drift checks strict.

## What ChatGPT-5.5 should confirm first

1. `git status` is clean
2. top-level docs migration is complete
3. no remaining `docs/*.adoc` without matching `docs/*.wvb`
