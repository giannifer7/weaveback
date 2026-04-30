// weaveback-docgen/src/xref.rs
// I'd Really Rather You Didn't edit this generated file.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use weaveback_lsp::LspClient;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct XrefLink {
    pub key: String,
    pub label: String,
    /// HTML path relative to docs/html/
    pub html: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct XrefEntry {
    /// HTML path relative to docs/html/ for this module's own page (may not exist yet)
    pub html: String,
    pub imports: Vec<XrefLink>,
    pub imported_by: Vec<XrefLink>,
    pub symbols: Vec<String>,
    /// Precise semantic links from LSP
    #[serde(default)]
    pub lsp_links: Vec<XrefLink>,
}
mod adoc_scan;
mod analysis;
mod build;
mod exclude;
mod module_key;
mod resolve;
mod workspace;

pub use adoc_scan::scan_adoc_file_declarations;
pub use build::build_xref;
pub use module_key::{html_path_for_key, module_key};

#[cfg(test)]
use analysis::{analyze_file, collect_items, collect_use_tree};
#[cfg(test)]
use build::find_line_col;
#[cfg(test)]
use exclude::is_excluded;
#[cfg(test)]
use resolve::{resolve_import, resolve_to_module};
#[cfg(test)]
use workspace::workspace_crate_names;

#[cfg(test)]
mod tests;

