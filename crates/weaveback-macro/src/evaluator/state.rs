// crates/weaveback-macro/src/evaluator/state.rs

use crate::evaluator::output::{SourceSpan, SpanRange};
use crate::types::ASTNode;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
#[derive(Debug, Clone)]
pub struct EvalConfig {
    pub sigil: char,
    pub include_paths: Vec<PathBuf>,
    /// When true, `%include`/`%import` evaluate their path argument but do not
    /// recurse into the file.  The resolved path is recorded in
    /// `EvaluatorState::discovered_includes` instead.  Used by the directory
    /// driver-discovery pass.
    pub discovery_mode: bool,
    /// When true, the `%env(NAME)` builtin is permitted to read environment
    /// variables.  Disabled by default so that templates cannot silently
    /// exfiltrate secrets without the user opting in via `--allow-env`.
    pub allow_env: bool,
}

impl Default for EvalConfig {
    fn default() -> Self {
        Self {
            sigil: '%',
            include_paths: vec![PathBuf::from(".")],
            discovery_mode: false,
            allow_env: false,
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub enum ScriptKind {
    None,
    Rhai,
    Python,
}
#[derive(Debug, Clone)]
pub struct MacroDefinition {
    pub name: String,
    pub params: Vec<String>,
    pub body: Arc<ASTNode>,
    pub script_kind: ScriptKind,
    pub frozen_args: HashMap<String, String>,
}
#[derive(Debug, Clone)]
pub struct TrackedValue {
    pub value: String,
    /// Per-token span ranges relative to `value[0]`.
    /// Empty means untracked (script/builtin result).
    /// Single entry covering `[0, value.len()]` is the coarse-span fast path.
    /// Multiple entries carry full per-token attribution threaded through argument evaluation.
    pub spans: Vec<SpanRange>,
}
#[derive(Debug, Default, Clone)]
pub struct ScopeFrame {
    pub variables: HashMap<String, TrackedValue>,
    pub macros: HashMap<String, MacroDefinition>,
}
pub struct SourceManager {
    source_files: Vec<Vec<u8>>,
    file_names: Vec<PathBuf>,
    sources_by_path: HashMap<PathBuf, usize>,
}

impl SourceManager {
    pub fn new() -> Self {
        Self {
            source_files: Vec::new(),
            file_names: Vec::new(),
            sources_by_path: HashMap::new(),
        }
    }

    pub fn add_source_if_not_present(&mut self, file_path: PathBuf) -> Result<u32, std::io::Error> {
        let file_path = file_path.canonicalize()?;
        if let Some(&src) = self.sources_by_path.get(&file_path) {
            return Ok(src as u32);
        }
        let content = std::fs::read(file_path.clone())?;
        let src = self.add_source_bytes(content, file_path.clone());
        Ok(src)
    }

    pub fn add_source_bytes(&mut self, content: Vec<u8>, path: PathBuf) -> u32 {
        let index = self.source_files.len();
        self.source_files.push(content);
        self.file_names.push(path.clone());
        self.sources_by_path.insert(path, index);
        index as u32
    }

    pub fn get_source(&self, src: u32) -> Option<&[u8]> {
        self.source_files.get(src as usize).map(|v| v.as_slice())
    }

    pub fn num_sources(&self) -> usize {
        self.source_files.len()
    }

    pub fn source_files(&self) -> &[PathBuf] {
        &self.file_names
    }
}
/// Raw record of a `%set(var_name, ...)` call site, captured during evaluation.
/// Positions are absolute byte offsets in the source file (same as Token.pos / Token.length).
#[derive(Debug, Clone)]
pub struct VarDefRaw {
    pub var_name: String,
    /// Source file index (same as Token.src).
    pub src: u32,
    /// Byte offset of the `set` keyword in the source.
    pub pos: u32,
    /// Byte length of the whole `set(...)` call.
    pub length: u32,
}
/// Raw record of a `%def / %rhaidef / %pydef(name, ...)` call site.
#[derive(Debug, Clone)]
pub struct MacroDefRaw {
    pub macro_name: String,
    /// Source file index (same as Token.src).
    pub src: u32,
    /// Byte offset of the def keyword in the source.
    pub pos: u32,
    /// Byte length of the whole def(...) call.
    pub length: u32,
}
pub use weaveback_core::MAX_RECURSION_DEPTH;
pub struct EvaluatorState {
    pub config: EvalConfig,
    pub scope_stack: Vec<ScopeFrame>,
    pub open_includes: HashSet<PathBuf>,
    pub current_file: PathBuf,
    pub source_manager: SourceManager,
    pub call_depth: usize,
    /// Set by `%here` to stop further evaluation cleanly (not an error).
    pub early_exit: bool,
    /// Populated during discovery mode: every path resolved by `%include`/`%import`.
    pub discovered_includes: Vec<PathBuf>,
    /// Accumulated `%set` call sites for the var_defs_map.
    pub var_defs: Vec<VarDefRaw>,
    /// Accumulated `%def/%rhaidef/%pydef` call sites for the macro_defs_map.
    pub macro_defs: Vec<MacroDefRaw>,
}

impl EvaluatorState {
    pub fn new(config: EvalConfig) -> Self {
        Self {
            config,
            scope_stack: vec![ScopeFrame::default()],
            open_includes: HashSet::new(),
            current_file: PathBuf::from(""),
            source_manager: SourceManager::new(),
            call_depth: 0,
            early_exit: false,
            discovered_includes: Vec::new(),
            var_defs: Vec::new(),
            macro_defs: Vec::new(),
        }
    }

    pub fn drain_var_defs(&mut self) -> Vec<VarDefRaw> {
        std::mem::take(&mut self.var_defs)
    }

    pub fn drain_macro_defs(&mut self) -> Vec<MacroDefRaw> {
        std::mem::take(&mut self.macro_defs)
    }

    pub fn push_scope(&mut self) {
        self.scope_stack.push(ScopeFrame::default());
    }

    pub fn pop_scope(&mut self) {
        if self.scope_stack.len() > 1 {
            self.scope_stack.pop();
        }
    }

    pub fn current_scope_mut(&mut self) -> &mut ScopeFrame {
        self.scope_stack.last_mut().unwrap()
    }

    /// Set a variable with no origin tracking (legacy/computed path).
    pub fn set_variable(&mut self, name: &str, value: &str) {
        self.current_scope_mut().variables.insert(
            name.into(),
            TrackedValue {
                value: value.into(),
                spans: vec![],
            },
        );
    }

    /// Set a variable with a single coarse origin span (fast path).
    pub fn set_tracked_variable(&mut self, name: &str, value: &str, span: Option<SourceSpan>) {
        let spans = if let Some(sp) = span {
            vec![SpanRange { start: 0, end: value.len(), span: sp }]
        } else {
            vec![]
        };
        self.current_scope_mut().variables.insert(
            name.into(),
            TrackedValue { value: value.into(), spans },
        );
    }

    /// Set a variable with full per-token span attribution (precise tracing path).
    pub fn set_traced_variable(&mut self, name: &str, value: String, spans: Vec<SpanRange>) {
        self.current_scope_mut().variables.insert(
            name.into(),
            TrackedValue { value, spans },
        );
    }

    /// Retrieve just the string value of a variable.
    pub fn get_variable(&self, name: &str) -> String {
        for frame in self.scope_stack.iter().rev() {
            if let Some(tv) = frame.variables.get(name) {
                return tv.value.clone();
            }
        }
        "".to_string()
    }

    /// Retrieve the tracked value of a variable.
    pub fn get_tracked_variable(&self, name: &str) -> Option<TrackedValue> {
        for frame in self.scope_stack.iter().rev() {
            if let Some(tv) = frame.variables.get(name) {
                return Some(tv.clone());
            }
        }
        None
    }

    pub fn define_macro(&mut self, mac: MacroDefinition) {
        self.current_scope_mut()
            .macros
            .insert(mac.name.clone(), mac);
    }

    pub fn get_macro(&self, name: &str) -> Option<MacroDefinition> {
        for frame in self.scope_stack.iter().rev() {
            if let Some(m) = frame.macros.get(name) {
                return Some(m.clone());
            }
        }
        None
    }

    pub fn get_sigil(&self) -> Vec<u8> {
        self.config.sigil.to_string().into_bytes()
    }
}
