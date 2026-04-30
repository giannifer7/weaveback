// weaveback-api/src/coverage/lcov.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;

mod parse;
mod summary;
mod output;
mod run;

pub use output::build_coverage_summary_view;
pub use parse::parse_lcov_records;
pub use run::run_coverage;
pub use summary::build_coverage_summary;

pub(in crate::coverage) use output::print_coverage_summary_to_writer;
#[cfg(test)]
pub(in crate::coverage) use output::explain_unattributed_file;
#[cfg(test)]
pub(in crate::coverage) use summary::{compute_unmapped_ranges, find_noweb_entries_for_generated_file};

