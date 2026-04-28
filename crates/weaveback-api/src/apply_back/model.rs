// weaveback-api/src/apply_back/model.rs
// I'd Really Rather You Didn't edit this generated file.

#[derive(Clone)]
pub struct ApplyBackOptions {
    pub db_path: PathBuf,
    pub gen_dir: PathBuf,
    pub dry_run: bool,
    /// Relative paths within gen/ to process; empty = all modified files.
    pub files: Vec<String>,
    /// When present, enables two-level tracing through macro expansion.
    pub eval_config: Option<EvalConfig>,
}

/// Where a patch lands and how to apply it.
enum PatchSource {
    /// Hunk from noweb-level expanded text (no macro attribution available).
    Noweb { src_file: String, src_line: usize, len: usize },
    /// Literal text from the original literate source — safe to auto-patch.
    Literal { src_file: String, src_line: usize, len: usize },
    /// Macro body text with no variable references — safe to auto-patch.
    MacroBodyLiteral { src_file: String, src_line: usize, macro_name: String },
    /// Macro body text containing `%%(...)` references.
    /// Attempt structural fix + oracle verification; report if it fails.
    MacroBodyWithVars { src_file: String, src_line: usize, macro_name: String },
    /// Argument value at a macro call site.
    /// Attempt col-based replacement + oracle verification; report if it fails.
    MacroArg {
        src_file: String,
        src_line: usize,
        src_col: u32,
        macro_name: String,
        param_name: String,
    },
    /// VarBinding or Computed — report only.
    Unpatchable { src_file: String, src_line: usize, kind_label: String },
}

impl PatchSource {
    fn src_file(&self) -> &str {
        match self {
            PatchSource::Noweb              { src_file, .. }
            | PatchSource::Literal          { src_file, .. }
            | PatchSource::MacroBodyLiteral { src_file, .. }
            | PatchSource::MacroBodyWithVars{ src_file, .. }
            | PatchSource::MacroArg         { src_file, .. }
            | PatchSource::Unpatchable      { src_file, .. } => src_file,
        }
    }
}

fn patch_source_rank(source: &PatchSource) -> i32 {
    match source {
        PatchSource::MacroArg { .. } => 50,
        PatchSource::Literal { .. } => 40,
        PatchSource::MacroBodyLiteral { .. } => 35,
        PatchSource::MacroBodyWithVars { .. } => 30,
        PatchSource::Noweb { .. } => 20,
        PatchSource::Unpatchable { .. } => 0,
    }
}

fn patch_source_location(source: &PatchSource) -> (&str, usize) {
    match source {
        PatchSource::Noweb { src_file, src_line, .. }
        | PatchSource::Literal { src_file, src_line, .. }
        | PatchSource::MacroBodyLiteral { src_file, src_line, .. }
        | PatchSource::MacroBodyWithVars { src_file, src_line, .. }
        | PatchSource::MacroArg { src_file, src_line, .. }
        | PatchSource::Unpatchable { src_file, src_line, .. } => (src_file, *src_line),
    }
}

struct Patch {
    source: PatchSource,
    /// Indent-stripped baseline gen/ text (may be multiple lines).
    old_text: String,
    /// Indent-stripped modified gen/ text (may be multiple lines).
    new_text: String,
    /// 0-indexed first line in the macro-expanded intermediate.
    expanded_line: u32,
}

struct CandidateResolution {
    line_idx: usize,
    new_line: String,
    score: i32,
}

#[derive(Clone)]
struct LspDefinitionHint {
    src_file: String,
    src_line: usize,
}

struct MacroArgSearch<'a> {
    db: &'a WeavebackDb,
    lines: &'a [String],
    hinted_line: usize,
    src_col: u32,
    old_text: &'a str,
    new_text: &'a str,
    eval_config: &'a EvalConfig,
    src_path: &'a std::path::Path,
    expanded_line: u32,
}

struct MacroBodySearch<'a> {
    db: &'a WeavebackDb,
    lines: &'a [String],
    hinted_line: usize,
    body_template: Option<&'a str>,
    old_text: &'a str,
    new_text: &'a str,
    sigil: char,
    eval_config: &'a EvalConfig,
    src_path: &'a std::path::Path,
    expanded_line: u32,
}

struct MacroCallSearch<'a> {
    lines: &'a [String],
    macro_name: &'a str,
    sigil: char,
    old_text: &'a str,
    new_text: &'a str,
    eval_config: &'a EvalConfig,
    src_path: &'a std::path::Path,
    expanded_line: u32,
}

