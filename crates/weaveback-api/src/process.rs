// weaveback-api/src/process.rs
// I'd Really Rather You Didn't edit this generated file.

mod args;
mod expanded_paths;
mod fs;
mod macro_prelude;
mod markdown_normalize;
mod run;
mod skip;

pub use args::{ProcessError, SinglePassArgs};
pub use fs::{find_files, write_depfile};
pub use run::run_single_pass;
pub use skip::compute_skip_set;

#[cfg(test)]
pub(crate) use markdown_normalize::{normalize_adoc_tables_for_markdown, normalize_expanded_document};

#[cfg(test)]
mod tests;

