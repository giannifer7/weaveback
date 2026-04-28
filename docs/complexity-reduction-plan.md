---
title: |-
  Complexity Reduction Plan
toc: left
---
# Complexity Reduction Plan

This note records the cleanup plan for the current state of the project:

* too many documents try to explain behavior authoritatively
* some generated files drift from their literate sources
* some overview pages duplicate code without owning it
* some generated Rust files are too large to review or maintain sanely
* tests are often physically attached to giant implementation files instead of
  being separated but still linked

The goal is not cosmetic cleanup. The goal is to restore a believable
documentation and generation model:

* one implementation source of truth per boundary
* architecture pages that explain, not impersonate code owners
* generated files that are valid bootstrap artifacts but never an independent
  source of truth
* smaller generated modules and smaller test units

The practical migration mechanism is the two-pass `.wvb` authoring model
described in [two-pass markup migration](two-pass-markup-migration.adoc).
Moving a document to `.wvb` should not be a mechanical syntax conversion.
Each migration is also a review gate for source ownership, module boundaries,
test placement, and generated-file size.

## Problem statement

The current failure mode is not just "bugs exist". It is structural:

1. complexity grew faster than the document model
2. source-of-truth boundaries blurred
3. generated files became sticky enough that drift could survive
4. overview pages accumulated stale code examples and pseudo-assembly sections
5. some files became too large for local reasoning

That produces a delusional self-documentation effect:

* documents describe the intended system better than the actual one
* humans trust the prose too much
* CI becomes the first place where reality pushes back

## Desired invariants

After the cleanup, these statements should be true:

1. Every implementation file has one canonical literate owner, or is explicitly
   non-literate ordinary source.
2. Generated files may be checked in for bootstrap, but they are never treated
   as authoritative over canonical source.
3. Retangling the repository is expected to be a meaningful drift check.
4. Overview and architecture pages do not embed large non-canonical code bodies
   unless they are clearly marked as excerpts.
5. Large generated Rust files are split into smaller modules and separate test
   files.
6. Tests are physically separate from large runtime files while still linked in
   the literate layer by module maps and references.

## Document taxonomy

Every document should belong to one class only.

### 1. Canonical implementation source

Examples:

* crate-level `.adoc` files that generate `.rs`
* root/project `.adoc` files that generate real workspace files

Rules:

* may assemble files
* may contain noweb chunks
* must stay mechanically in sync with generated outputs
* should be local to the code they own

### 2. Architecture / rationale

Examples:

* high-level design notes
* subsystem architecture pages
* sequencing and tradeoff documents

Rules:

* do not assemble files
* do not pretend to own implementation
* use links to canonical sources
* embedded code should be short excerpts, not duplicated file bodies

### 3. User documentation

Examples:

* install docs
* CLI overview
* README

Rules:

* explain supported workflows
* avoid implementation detail unless it directly helps the user
* avoid copying large implementation fragments

### 4. Planning / notes

Examples:

* roadmap
* linter plan
* this document

Rules:

* may be incomplete
* must not describe themselves as settled truth
* should be easy to delete or rewrite later

## Workstreams

## Execution model: `.wvb` as the cleanup vehicle

The two-pass migration and this complexity plan are one workstream, not two.
For each document selected for `.wvb` conversion, perform these checks before
accepting the migration:

1. **Ownership check** — confirm the document is the canonical implementation
   owner. If not, convert it to architecture/user documentation instead.
2. **Output check** — list every generated file and decide whether each output
   still belongs in the same document.
3. **Split check** — if a generated Rust file is too large, split the Rust module
   during the `.wvb` migration rather than preserving the bad shape.
4. **Test check** — move large test tails into separate generated test files
   while keeping links from the runtime module and crate index.
5. **Projection check** — generate both `expanded-adoc/` and `expanded-md/` when
   the document is code-producing, and verify both projections tangle to the same
   generated source.
6. **Drift check** — run a retangle and inspect tracked diffs before committing.

This keeps the migration honest: `.wvb` is not another documentation layer on
top of complexity; it is the tool used to remove complexity.

## Candidate ordering

Use small, leaf documents first to validate workflow, then move toward files
where the structural payoff is high.

1. **Pilot / calibration**
   `crates/weaveback-agent-core/src-wvb/lib.wvb` is the first completed pilot.
   It validates projection parity without changing generated Rust.
2. **Small crate indexes and module maps**
   Convert low-risk files that mostly assemble `lib.rs` or `mod.rs`.
   Use these to stabilize component macros and output-directory policy.
3. **Medium runtime modules**
   Convert modules around 500-900 lines when they have clear boundaries.
   Split tests out during conversion if they are attached as large tails.
4. **Large generated files**
   Convert only with a module-splitting plan.
   Do not preserve a 2000+ line generated Rust file just because the old
   `.adoc` did so.
5. **Architecture and overview pages**
   Do not convert mechanically.
   First decide whether the page owns code or merely explains code.

## Workstream A: Restore Source-Of-Truth Boundaries

Goal:

* eliminate hybrid documents that both explain and pretend to own code they no
  longer generate

Tasks:

1. Audit `project/`, `docs/`, and `cli-spec/` for pages that embed large
   non-canonical code bodies.
2. For each such page, choose exactly one direction:
   make it canonical again, or convert it into pure architecture/user documentation.
3. Prefer local crate-level ownership over cross-crate "god spec" ownership.

Acceptance criteria:

* no page should contain wording like "canonical sources live elsewhere" while
  still embedding large fake file-assembly blocks
* no page should mix architecture prose with non-authoritative full-file code

Initial targets:

* `project/agent-python.adoc`
* any remaining cross-crate overview pages with code duplication

## Workstream B: Make Retangle a Real Drift Check

Goal:

* make full retangle a reliable detector of source/generated mismatch

Tasks:

1. Add a routine local/CI check that retangles and inspects whether tracked
   generated files changed.
2. Classify every post-retangle diff as one of:
   expected source change catch-up, generation bug, or stale generated artifact previously committed.
3. Investigate any file that repeatedly drifts after clean retangle.

Important nuance:

* generated files are legitimate bootstrap artifacts
* but regeneration must still be authoritative

Acceptance criteria:

* retangle-driven diffs are rare and explainable
* repeated source/generated desync is treated as a bug, not normal background
  noise

Likely follow-up work:

* continue auditing places where generated `.rs` was manually patched
* tighten the workflow around checked-in generated files

## Workstream C: Split Giant Generated Rust Files

Goal:

* no unreviewable monster files

Hard rule:

* a 2700-line generated Rust file is not acceptable design, even if generated
  correctly

Tasks:

1. Identify the largest generated Rust files by line count.
2. For each file, split by concern into smaller modules.
3. Keep the module map in the crate index `.adoc`.
4. Avoid using chunks merely to accumulate into one giant output file.

Desired output shape:

* smaller `src/*.rs` runtime modules
* separate `tests/*.rs` or `src/tests/*.rs` files
* no giant `#[cfg(test)] mod tests` tail on a massive runtime file

Likely first targets:

* `crates/weaveback-serve/src/lib.rs`
* any large `weaveback-macro` or `weaveback-tangle` modules that have grown
  beyond sane review size

Acceptance criteria:

* each major runtime concern lives in its own generated module
* test code is no longer a giant tail attached to a runtime file

## Workstream D: Separate Tests But Keep Them Linked

Goal:

* preserve literate linkage without forcing tests into giant implementation
  files

Tasks:

1. For each large subsystem, create dedicated test-oriented `.adoc` files.
2. Keep tests near the crate/module map via links and explicit "see also"
   sections.
3. Generate separate test files rather than appending all tests to the main
   module.

Recommended pattern:

* crate index `.adoc`
  explains module map and assembles `lib.rs` / `mod.rs`
* runtime module `.adoc`
  one concern each
* test module `.adoc`
  dedicated tests for that concern

Acceptance criteria:

* tests remain traceable to prose and rationale
* test files are separated physically
* runtime files become shorter and clearer

## Workstream E: Reduce Documentation Claims

Goal:

* remove claims that are not continuously checked

Tasks:

1. Audit phrases like:
   `generates`, `canonical`, `single source of truth`, `this page assembles`.
2. Keep such wording only where it is mechanically true.
3. Downgrade other wording to:
   `architecture note`, `overview`, `example excerpt`.

Acceptance criteria:

* no misleading authority claims remain in overview pages
* generated docs describe the system honestly, even if more modestly

## Workstream F: CI and Validation Alignment

Goal:

* make CI validate the actual intended boundaries

Tasks:

1. Keep generic Rust tests separate from Python-specific extension validation.
2. Add targeted tests for source/generated-sensitive areas.
3. When a test depends on environment-specific tooling, avoid process-global
   tricks like mutating `PATH` in parallel test suites.

Recent examples already fixed:

* Python agent tests split into their own job
* D2 mock test stopped mutating global `PATH`

Acceptance criteria:

* CI failures localize quickly
* fewer tests fail because of global environment races

## Immediate sequence

This is the recommended order of execution.

### Phase 1: stop the obvious lying

1. Convert hybrid overview pages into true overview pages.
2. Remove non-canonical file-assembly sections.
3. Add explicit canonical-source links where needed.

Exit condition:

* no major page still pretends to own code it does not generate

### Phase 2: clean generated drift

1. Run full retangle.
2. Classify every diff.
3. Commit only legitimate catch-up or source fixes.
4. Open issues/notes for any recurring generator defect.

Exit condition:

* full retangle no longer produces surprising diffs

### Phase 3: split the worst large files

1. Pick the largest generated Rust offender.
2. Split runtime modules first.
3. Split tests second.
4. Validate each split with targeted tests before moving to the next crate.

Suggested first target:

* `weaveback-serve`

Suggested second targets:

* whichever generated files are still largest after that

### Phase 4: institutionalize the boundary

1. Add/document a retangle drift check.
2. Keep architecture docs architecture-only.
3. Keep crate docs canonical and local.

Exit condition:

* the project has fewer authoritative surfaces and more trustworthy ones

## Candidate issue list

These are good cleanup tickets to spin out later.

* Convert remaining hybrid docs to architecture-only pages.
* Add a documented "retangle and inspect diff" maintenance command.
* Split `weaveback-serve` into smaller runtime modules and separate test files.
* Audit all generated Rust files above a chosen line-count threshold.
* Add a linter/check for pages that combine assembly markers with non-canonical
  wording.
* Document the bootstrap-artifact rule explicitly.

## Non-goals

This plan is not trying to:

* remove checked-in generated files unconditionally
* eliminate all overview pages
* make every source literate immediately
* redesign the entire macro/tangle architecture in one pass

The point is to reduce delusion and restore trust, not to start another
explosion in scope.
