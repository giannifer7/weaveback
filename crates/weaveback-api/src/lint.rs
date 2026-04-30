// weaveback-api/src/lint.rs
// I'd Really Rather You Didn't edit this generated file.

use std::fs;
use std::path::{Path, PathBuf};
use weaveback_tangle::NowebSyntax;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum LintRule {
    ChunkBodyOutsideFence,
    UnterminatedChunkDefinition,
    RawWvbLink,
    RawWvbSourceBlock,
    RawWvbTable,
}

impl LintRule {
    pub fn id(self) -> &'static str {
        match self {
            Self::ChunkBodyOutsideFence => "chunk-body-outside-fence",
            Self::UnterminatedChunkDefinition => "unterminated-chunk-definition",
            Self::RawWvbLink => "raw-wvb-link",
            Self::RawWvbSourceBlock => "raw-wvb-source-block",
            Self::RawWvbTable => "raw-wvb-table",
        }
    }
}

impl std::str::FromStr for LintRule {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "chunk-body-outside-fence" => Ok(Self::ChunkBodyOutsideFence),
            "unterminated-chunk-definition" => Ok(Self::UnterminatedChunkDefinition),
            "raw-wvb-link" => Ok(Self::RawWvbLink),
            "raw-wvb-source-block" => Ok(Self::RawWvbSourceBlock),
            "raw-wvb-table" => Ok(Self::RawWvbTable),
            _ => Err(format!(
                "unknown lint rule '{s}' (supported: chunk-body-outside-fence, unterminated-chunk-definition, raw-wvb-link, raw-wvb-source-block, raw-wvb-table)"
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct LintViolation {
    pub file:    PathBuf,
    pub line:    usize,
    pub rule:    LintRule,
    pub message: String,
    pub hint:    Option<String>,
}
mod config;
mod fs_scan;
mod rules;
mod run;

pub use run::run_lint;

#[cfg(test)]
use config::{lint_syntaxes_for_file, load_lint_syntaxes_from};
#[cfg(test)]
use fs_scan::collect_literate_files;
#[cfg(test)]
use rules::{
    lint_chunk_body_outside_fence, lint_raw_wvb_links, lint_raw_wvb_source_blocks,
    lint_raw_wvb_tables, lint_unterminated_chunk_definition, parse_chunk_definition_name,
};

#[cfg(test)]
mod tests;

