// weaveback-api/src/coverage.rs
// I'd Really Rather You Didn't edit this generated file.

use crate::lookup;
use serde_json::json;
use std::collections::HashSet;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use weaveback_agent_core::{Workspace as AgentWorkspace, WorkspaceConfig as AgentWorkspaceConfig};
use weaveback_core::PathResolver;
use weaveback_macro::evaluator::EvalConfig;
use weaveback_tangle::lookup::distinctive_suffix_candidates;
use weaveback_tangle::WeavebackError;

mod error;
mod locations;
mod lcov;
mod cargo;
mod text;

pub use error::CoverageApiError;
pub use lcov::{
    build_coverage_summary,
    build_coverage_summary_view,
    parse_lcov_records,
    run_coverage,
};
pub use locations::{
    open_db,
    parse_generated_location,
    run_attribute,
    run_where,
    scan_generated_locations,
};
pub use text::{run_cargo_annotated, run_cargo_annotated_to_writer, run_graph, run_impact, run_search, run_tags, run_trace};

pub(in crate::coverage) use cargo::{
    build_location_attribution_summary,
    collect_cargo_attributions,
    collect_cargo_span_attributions,
    emit_augmented_cargo_message,
    emit_cargo_summary_message,
    trace_generated_location,
    CargoMessageEnvelope,
};
#[cfg(test)]
pub(in crate::coverage) use cargo::{build_cargo_attribution_summary, CargoDiagnostic, CargoDiagnosticSpan};
#[cfg(test)]
pub(in crate::coverage) use lcov::{
    compute_unmapped_ranges,
    explain_unattributed_file,
    find_noweb_entries_for_generated_file,
    print_coverage_summary_to_writer,
};
#[cfg(test)]
pub(in crate::coverage) use text::{collect_text_attributions, emit_text_attribution_message};

#[cfg(test)]
mod tests_coverage;

