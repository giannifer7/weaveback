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

