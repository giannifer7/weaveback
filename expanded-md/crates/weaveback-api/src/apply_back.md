# Apply-Back

`apply_back.rs` implements `wb-tangle apply-back`: it propagates edits made
directly in `gen/` back to the literate source.  It is also the backend for
the `weaveback_apply_back` MCP tool in [`mcp.rs`](mcp.adoc).

Most of weaveback is designed around a forward, disciplined workflow:
edit the literate source, tangle it, then verify the generated result.
`apply-back` exists for the inverse case.  Sometimes the fastest way to see
the right answer is to change the generated file first: an editor command,
an LSP code action, a quick local experiment, or an agent acting on the
surface Rust, Python, or TypeScript file.  Once that happens, the generated
file becomes a valuable artifact, but also a problem: weaveback must recover
the *source* change that best explains it, so the literate project remains
the source of truth.

This command therefore exists for one very specific recovery workflow:

* the user, editor, or agent already changed a generated file
* the generated output is now the *desired* result
* weaveback must infer which literate source edit best explains that result

That is a fundamentally different problem from `weaveback_apply_fix`.
`apply_fix` starts from a known source edit and verifies its output.
`apply-back` starts from an edited output file and tries to reconstruct a
plausible source edit that would regenerate it.

The easy cases are truly local ones:

* a literal line in a `@file` chunk changed
* a noweb-mapped line changed with no macro ambiguity
* an insertion or deletion lands in a continuous source region

The hard cases are the ones that matter for agentic workflows:

* the changed output line came from a macro body, not from a literal line
* the changed token was an argument value rather than part of the body template
* the source line moved after the original tangle, so stale line numbers are no
  longer enough
* several source locations are *plausible*, and only one actually regenerates
  the desired output

That is why `apply-back` is now built around three layers rather than one:

* provenance from `noweb_map` and `perform_trace`
* bounded candidate search near the traced source region
* oracle verification by re-evaluating the patched source

The guiding rule is simple: a candidate source edit is only acceptable if it
reproduces the changed generated line.  Attribution and heuristics narrow the
search space; the oracle is what makes the final decision safe.

In other words, `apply-back` is not "reverse tangle" in the naive sense.
It is a constrained reconstruction problem:

* provenance says where a change could have come from
* search proposes a small number of source-side rewrites
* re-evaluation checks whether one of those rewrites really yields the edited
  generated file

That design matters for coding agents.  An agent can safely operate on the
generated view that LSPs and editors understand best, while weaveback keeps
the durable edit in the literate source.  The agent does not need arbitrary
file writes into the prose tree; it needs a way to propose or recover source
changes that still satisfy the weaveback invariants.

The rest of this file is structured from that perspective:

* first, how the command classifies and traces changed output lines
* then, how it searches when tracing alone is not enough
* finally, how it uses the regenerated output as the hard oracle before any
  change is accepted

## Algorithm

For each modified `gen/` file (current bytes ≠ stored baseline):

. *Diff* the current file against the baseline using `similar::TextDiff`.
. For each changed line in a `Replace` hunk of equal size:
  .. Look up `noweb_map` to find the expanded-text source and line.
  .. Probe several changed columns, not just one, and call
     `perform_trace` ([`lookup.rs`](lookup.adoc)) to collect macro-level
     attribution candidates.
  .. Optionally query the language-specific LSP definition at the changed
     generated position and use that as a ranking hint.
  .. Classify the best-ranked attribution as one of the `PatchSource`
     variants.
. *Group* patches by true source file and apply them all at once.
. *Update the baseline* for files with no unapplied patches.

Size-changing hunks (`Delete`, `Insert`, or `Replace` with different line
counts) cannot be automatically applied and are reported for manual attention.

### Fast path vs search path

For literal and simple noweb patches, weaveback still uses a direct patching
path because it is cheap and unambiguous.

For `MacroBodyWithVars` and `MacroArg` patches, weaveback no longer trusts a
single guessed rewrite.  Instead it searches over a bounded set of nearby
candidate source lines and call sites, then *verifies* each candidate by
re-evaluating the patched source with the macro expander and confirming the
relevant expanded line matches the desired output.  A wrong candidate simply
fails the oracle check and is not written.

### Context-guided ranking

When several candidates are plausible, weaveback prefers the ones that:

* stay close to the hinted source line
* remain inside the same chunk definition when possible
* overlap lexically with the changed text
* agree with an optional LSP definition hint from the generated position

This ranking is intentionally heuristic.  The oracle remains the hard
constraint; ranking only decides which *verified* candidate is the best fit
for the literate structure.

### Fuzzy line matching

When the source line is not found at the expected index (e.g., after an
unrelated edit above it), a broader bounded search is used:

* whitespace-normalised fuzzy matching around the hinted line
* nearby-line exploration inside the same source file
* macro-call fallback search when body-level attribution is too coarse

See [weaveback.adoc](lib.adoc) for the module map.

## Implementation Sources

The implementation chunks are split by concern under `crates/weaveback-api/src-wvb/apply_back/`:

* `impl-api.wvb` defines public API types, errors, and internal patch models.
* `impl-fuzzy.wvb` contains fuzzy line matching.
* `impl-oracle.wvb` contains oracle re-expansion checks.
* `impl-heuristics.wvb` contains macro-local patch search heuristics.
* `impl-resolve.wvb` chooses the best patch source.
* `impl-apply.wvb` applies line-level patches to source files.
* `impl-run.wvb` exposes `run_apply_back` and orchestrates diffs.

## Tests

The generated test module lives at `crates/weaveback-api/src/apply_back/tests.rs`, but its literate source is split by concern under `crates/weaveback-api/src-wvb/apply_back/`:

* `tests-primitives.wvb` covers pure helpers and local transformations.
* `tests-workspace.wvb` defines temporary workspace fixtures and runner/source-map edge cases.
* `tests-resolution.wvb` covers source resolution, LSP hints, and oracle search.
* `tests-apply-file.wvb` covers direct file patch application.
* `tests-runner.wvb` covers `run_apply_back` entry-point behavior.
* `tests-batch.wvb` covers batch diff/application orchestration.
* `tests-assembly.wvb` assembles those chunks into the Rust test module.

## Assembly

The Rust implementation is split into focused generated files under
`crates/weaveback-api/src/apply_back/`.  The public module remains
`weaveback_api::apply_back`; sibling modules share an explicit
`pub(in crate::apply_back)` surface instead of relying on text inclusion.

```rust
// <[@file weaveback-api/src/apply_back/types.rs]>=
// weaveback-api/src/apply_back/types.rs
// I'd Really Rather You Didn't edit this generated file.

// <[applyback-types]>

// @
```


```rust
// <[@file weaveback-api/src/apply_back/model.rs]>=
// weaveback-api/src/apply_back/model.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;

// <[applyback-structs]>

// @
```


```rust
// <[@file weaveback-api/src/apply_back/fuzzy.rs]>=
// weaveback-api/src/apply_back/fuzzy.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;

// <[applyback-fuzzy]>

// @
```


```rust
// <[@file weaveback-api/src/apply_back/oracle.rs]>=
// weaveback-api/src/apply_back/oracle.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;

// <[applyback-oracle]>

// @
```


```rust
// <[@file weaveback-api/src/apply_back/heuristics.rs]>=
// weaveback-api/src/apply_back/heuristics.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;

// <[applyback-heuristics]>

// @
```


```rust
// <[@file weaveback-api/src/apply_back/resolve.rs]>=
// weaveback-api/src/apply_back/resolve.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;

// <[applyback-resolve]>

// @
```


```rust
// <[@file weaveback-api/src/apply_back/apply.rs]>=
// weaveback-api/src/apply_back/apply.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;

// <[applyback-apply]>

// @
```


```rust
// <[@file weaveback-api/src/apply_back/run.rs]>=
// weaveback-api/src/apply_back/run.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;

// <[applyback-run]>

// @
```


```rust
// <[@file weaveback-api/src/apply_back.rs]>=
// weaveback-api/src/apply_back.rs
// I'd Really Rather You Didn't edit this generated file.

use weaveback_core::PathResolver;
use weaveback_lsp::LspClient;
use weaveback_macro::evaluator::{EvalConfig, Evaluator};
use weaveback_macro::macro_api::process_string;
use weaveback_tangle::db::{NowebMapEntry, WeavebackDb};
use weaveback_tangle::lookup::find_best_noweb_entry;
use regex::Regex;
use similar::TextDiff;
use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;

use crate::lookup;

mod types;
mod model;
mod fuzzy;
mod oracle;
mod heuristics;
mod resolve;
mod apply;
mod run;

pub(in crate::apply_back) use apply::{apply_patches_to_file, strip_indent, FilePatchContext};
pub(in crate::apply_back) use fuzzy::fuzzy_find_line;
pub(in crate::apply_back) use heuristics::{
    attempt_macro_arg_patch,
    resolve_noweb_entry,
    search_macro_arg_candidate,
    search_macro_body_candidate,
    search_macro_call_candidate,
};
pub(in crate::apply_back) use model::{
    patch_source_location,
    patch_source_rank,
    CandidateResolution,
    LspDefinitionHint,
    MacroArgSearch,
    MacroBodySearch,
    MacroCallSearch,
    Patch,
    PatchSource,
};
pub(in crate::apply_back) use oracle::{
    differing_token_pair,
    splice_line,
    token_overlap_score,
    verify_candidate,
};
pub(in crate::apply_back) use resolve::{
    lsp_definition_hint,
    resolve_best_patch_source,
};

#[cfg(test)]
pub(in crate::apply_back) use apply::do_patch;
#[cfg(test)]
pub(in crate::apply_back) use heuristics::{choose_best_candidate, rank_candidate};
#[cfg(test)]
pub(in crate::apply_back) use heuristics::attempt_macro_body_fix;
#[cfg(test)]
pub(in crate::apply_back) use resolve::resolve_patch_source;

pub use model::ApplyBackOptions;
pub use run::run_apply_back;
pub use types::ApplyBackError;

#[cfg(test)]
mod tests;

// @
```

