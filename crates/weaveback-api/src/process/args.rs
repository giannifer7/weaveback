// weaveback-api/src/process/args.rs
// I'd Really Rather You Didn't edit this generated file.

use std::path::PathBuf;

use weaveback_macro::evaluator::EvalError;
use weaveback_tangle::WeavebackError;

/// Combined error type for a single tangle pass.
#[derive(Debug, thiserror::Error)]
pub enum ProcessError {
    #[error("{0}")]
    Tangle(#[from] WeavebackError),
    #[error("{0}")]
    Macro(#[from] EvalError),
    #[error("{0}")]
    Io(#[from] std::io::Error),
}


/// All arguments needed for one tangle pass.
///
/// Constructed by the CLI layer from parsed `clap` args and passed to
/// `run_single_pass`.
pub struct SinglePassArgs {
    /// Explicit input file paths (used when `directory` is `None`).
    pub inputs: Vec<PathBuf>,
    /// Process all files with matching extensions under this directory.
    pub directory: Option<PathBuf>,
    /// Base directory for resolving relative `inputs` paths.
    pub input_dir: PathBuf,
    /// Output directory for generated files.
    pub gen_dir: PathBuf,
    /// Chunk opening delimiter (default `<<`).
    pub open_delim: String,
    /// Chunk closing delimiter (default `>>`).
    pub close_delim: String,
    /// Chunk end marker (default `@`).
    pub chunk_end: String,
    /// Comma-separated comment markers (default `#,//`).
    pub comment_markers: String,
    /// File extension(s) to scan in `--dir` mode.
    pub ext: Vec<String>,
    /// Skip macro expansion and feed raw source directly to tangle.
    pub no_macros: bool,
    /// Prelude files evaluated before pass inputs in macro-enabled mode.
    pub macro_prelude: Vec<PathBuf>,
    /// Extension assigned to macro-expanded virtual documents before tangling.
    pub expanded_ext: Option<String>,
    /// Directory for expanded `.adoc` intermediates.
    pub expanded_adoc_dir: PathBuf,
    /// Directory for expanded `.md` intermediates.
    pub expanded_md_dir: PathBuf,
    /// Stop after macro expansion and write expanded documents.
    pub macro_only: bool,
    /// Print discovered `@file` chunk names and exit (no writes).
    pub dry_run: bool,
    /// Path to the weaveback SQLite database.
    pub db: PathBuf,
    /// Write a Makefile depfile to this path.
    pub depfile: Option<PathBuf>,
    /// Touch this file after a successful run (stamp target for `make`).
    pub stamp: Option<PathBuf>,
    /// Treat undefined chunk references as errors.
    pub strict: bool,
    /// Warn about defined-but-unused chunks.
    pub warn_unused: bool,
    /// Allow `%%env(NAME)` builtins to read environment variables.
    pub allow_env: bool,
    /// Allow writing generated files outside the home directory.
    pub allow_home: bool,
    /// Overwrite generated files even if they were externally modified.
    pub force_generated: bool,
    /// Macro sigil character (default `%%`).
    pub sigil: char,
    /// Path separator-separated include search paths.
    pub include: String,
    /// Formatter commands per output extension, e.g. `"rs=rustfmt"`.
    pub formatter: Vec<String>,
    /// Skip rebuilding the prose FTS index after this run.
    pub no_fts: bool,
    /// Print macro-expanded text to stderr before tangle processing.
    pub dump_expanded:  bool,
    /// Override project root (defaults to CWD).
    pub project_root:   Option<PathBuf>,
}

impl SinglePassArgs {
    #[cfg(test)]
    pub fn default_for_test() -> Self {
        Self {
            inputs: vec![],
            directory: None,
            input_dir: PathBuf::new(),
            gen_dir: PathBuf::new(),
            open_delim: "<<".to_string(),
            close_delim: ">>".to_string(),
            chunk_end: "@".to_string(),
            comment_markers: "//,#".to_string(),
            ext: vec!["adoc".to_string()],
            no_macros: true,
            macro_prelude: vec![],
            expanded_ext: None,
            expanded_adoc_dir: PathBuf::from("expanded-adoc"),
            expanded_md_dir: PathBuf::from("expanded-md"),
            macro_only: false,
            dry_run: false,
            db: PathBuf::new(),
            depfile: None,
            stamp: None,
            strict: false,
            warn_unused: false,
            allow_env: false,
            allow_home: true,
            force_generated: false,
            sigil: '%',
            include: String::new(),
            formatter: vec![],
            no_fts: true,
            dump_expanded: false,
            project_root: None,
        }
    }
}

