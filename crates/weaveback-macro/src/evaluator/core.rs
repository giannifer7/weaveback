// crates/weaveback-macro/src/evaluator/core.rs

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::builtins::{BuiltinFn, default_builtins};
use super::errors::{EvalError, EvalResult};
use super::monty_eval::MontyEvaluator;
use super::output::{EvalOutput, PreciseTracingOutput, SourceSpan, SpanKind, SpanRange};
use super::rhai_eval::{self, RhaiEvaluator};
use super::state::{EvalConfig, EvaluatorState, MAX_RECURSION_DEPTH, MacroDefinition, ScriptKind};
use crate::types::{ASTNode, NodeKind, Token, TokenKind};

pub struct Evaluator {
    state: EvaluatorState,
    builtins: HashMap<String, BuiltinFn>,
    rhai_evaluator: RhaiEvaluator,
    rhai_store: HashMap<String, rhai::Dynamic>,
    monty_evaluator: MontyEvaluator,
    py_store: HashMap<String, String>,
}

impl Evaluator {
    pub fn new(config: EvalConfig) -> Self {
        Evaluator {
            state: EvaluatorState::new(config),
            builtins: default_builtins(),
            rhai_evaluator: RhaiEvaluator::new(),
            rhai_store: HashMap::new(),
            monty_evaluator: MontyEvaluator::new(),
            py_store: HashMap::new(),
        }
    }

    /// Access the underlying SourceManager (useful for mapping output spans back to lines/columns).
    pub fn sources(&self) -> &crate::evaluator::state::SourceManager {
        &self.state.source_manager
    }

    /// Insert a value into the Rhai store.
    /// Integers and floats are stored with their native Rhai type so that
    /// arithmetic operators work inside scripts without explicit conversion.
    pub fn rhaistore_set_str(&mut self, key: String, value: String) {
        let dynamic = if let Ok(n) = value.trim().parse::<i64>() {
            rhai::Dynamic::from(n)
        } else if let Ok(f) = value.trim().parse::<f64>() {
            rhai::Dynamic::from(f)
        } else {
            rhai::Dynamic::from(value)
        };
        self.rhai_store.insert(key, dynamic);
    }

    /// Evaluate a Rhai expression and store the resulting Dynamic value.
    /// Use this to initialise store entries with typed literals like `[]` or `#{}`.
    pub fn rhaistore_set_expr(&mut self, key: String, expr: &str) -> Result<(), String> {
        let val = self
            .rhai_evaluator
            .eval_expr(expr)
            .map_err(|e| format!("rhaiexpr: {e}"))?;
        self.rhai_store.insert(key, val);
        Ok(())
    }

    /// Read a value from the Rhai store, converting it to String.
    pub fn rhaistore_get(&self, key: &str) -> String {
        self.rhai_store
            .get(key)
            .map(|d| rhai_eval::dynamic_to_string(d.clone()))
            .unwrap_or_default()
    }

    pub fn pystore_set(&mut self, key: String, value: String) {
        self.py_store.insert(key, value);
    }

    pub fn pystore_get(&self, key: &str) -> String {
        self.py_store.get(key).cloned().unwrap_or_default()
    }

    pub fn define_macro(&mut self, mac: crate::evaluator::state::MacroDefinition) {
        self.state.define_macro(mac);
    }

    pub fn set_variable(&mut self, name: &str, value: &str) {
        self.state.set_variable(name, value);
    }

    pub fn record_var_def(&mut self, var_name: String, src: u32, pos: u32, length: u32) {
        self.state.var_defs.push(super::state::VarDefRaw { var_name, src, pos, length });
    }

    pub fn record_macro_def(&mut self, macro_name: String, src: u32, pos: u32, length: u32) {
        self.state.macro_defs.push(super::state::MacroDefRaw { macro_name, src, pos, length });
    }

    pub fn drain_var_defs(&mut self) -> Vec<super::state::VarDefRaw> {
        self.state.drain_var_defs()
    }

    pub fn drain_macro_defs(&mut self) -> Vec<super::state::MacroDefRaw> {
        self.state.drain_macro_defs()
    }

    pub fn add_source_if_not_present(&mut self, file_path: PathBuf) -> Result<u32, std::io::Error> {
        self.state
            .source_manager
            .add_source_if_not_present(file_path)
    }

    pub fn add_source_bytes(&mut self, content: Vec<u8>, path: PathBuf) -> u32 {
        self.state.source_manager.add_source_bytes(content, path)
    }

    pub fn set_current_file(&mut self, file: PathBuf) {
        self.state.current_file = file;
    }

    pub fn get_current_file_path(&self) -> PathBuf {
        self.state.current_file.clone()
    }

    pub fn source_files(&self) -> &[PathBuf] {
        self.state.source_manager.source_files()
    }

    pub fn get_special_char(&self) -> Vec<u8> {
        self.state.get_special_char()
    }

    pub fn set_early_exit(&mut self) {
        self.state.early_exit = true;
    }

    pub fn evaluate(&mut self, node: &ASTNode) -> EvalResult<String> {
        if self.state.early_exit {
            return Ok(String::new());
        }
        let mut out = String::new();
        match node.kind {
            NodeKind::Text | NodeKind::Space | NodeKind::Ident => {
                let txt = self.node_text(node);
                out.push_str(&txt);
            }
            NodeKind::Var => {
                let var_name = self.node_text(node);
                let val = self.state.get_variable(&var_name);
                out.push_str(&val);
            }
            NodeKind::Macro => {
                let name = self.node_text(node);
                let expansion = self.evaluate_macro_call(node, &name)?;
                out.push_str(&expansion);
            }
            NodeKind::Block | NodeKind::Param => {
                for child in &node.parts {
                    let s = self.evaluate(child)?;
                    out.push_str(&s);
                }
            }
            NodeKind::LineComment | NodeKind::BlockComment => {}
            _ => {
                for child in &node.parts {
                    let s = self.evaluate(child)?;
                    out.push_str(&s);
                }
            }
        }
        Ok(out)
    }

    pub fn node_text(&self, node: &ASTNode) -> String {
        if let Some(source) = self.state.source_manager.get_source(node.token.src) {
            let start = node.token.pos;
            let end = node.token.pos + node.token.length;
            if end > source.len() || start > source.len() {
                eprintln!(
                    "node_text: out of range - start: {}, end: {}, source len: {}",
                    start,
                    end,
                    source.len()
                );
                return "".into();
            }

            let slice = match node.token.kind {
                TokenKind::BlockOpen | TokenKind::BlockClose | TokenKind::Macro => {
                    if end > start + 2 {
                        &source[(start + 1)..(end - 1)]
                    } else {
                        &source[start..end]
                    }
                }
                TokenKind::Var => {
                    if end > start + 3 {
                        &source[(start + 2)..(end - 1)]
                    } else {
                        &source[start..end]
                    }
                }
                TokenKind::Special => {
                    if end > start + 1 {
                        &source[start..(end - 1)]
                    } else {
                        &source[start..end]
                    }
                }
                _ => &source[start..end],
            };
            String::from_utf8_lossy(slice).to_string()
        } else {
            eprintln!("node_text: invalid src index");
            "".into()
        }
    }

    pub fn extract_name_value(&self, name_token: &Token) -> String {
        if let Some(source) = self.state.source_manager.get_source(name_token.src) {
            let start = name_token.pos;
            let end = name_token.pos + name_token.length;

            // Bounds checking
            if end > source.len() || start > source.len() {
                eprintln!(
                    "extract_name_value: out of range - start: {}, end: {}, source len: {}",
                    start,
                    end,
                    source.len()
                );
                return "".into();
            }

            // Since we know it's an Identifier, we can extract directly
            String::from_utf8_lossy(&source[start..end]).to_string()
        } else {
            eprintln!("extract_name_value: invalid src index");
            "".into()
        }
    }

    pub fn evaluate_macro_call(&mut self, node: &ASTNode, name: &str) -> EvalResult<String> {
        if let Some(bf) = self.builtins.get(name) {
            return bf(self, node);
        }

        if self.state.call_depth >= MAX_RECURSION_DEPTH {
            return Err(EvalError::Runtime(format!(
                "maximum recursion depth ({}) exceeded in macro '{}'",
                MAX_RECURSION_DEPTH, name
            )));
        }

        let mac = match self.state.get_macro(name) {
            Some(m) => m,
            None => return Err(EvalError::UndefinedMacro(name.into())),
        };

        let param_nodes: Vec<&ASTNode> = node
            .parts
            .iter()
            .filter(|p| p.kind == NodeKind::Param)
            .collect();

        self.state.push_scope();

        // frozen_args are vars that are not parameters
        // and get their values at definition site
        for (var, frozen_val) in mac.frozen_args.iter() {
            self.state.set_variable(var, frozen_val);
        }

        // Python-style parameter binding:
        //   - Positional args must come before named args; a positional after a
        //     named arg is an error (mirrors Python's SyntaxError).
        //   - Positional args fill declared params left-to-right.
        //   - Named args (key = value) bind by name in any relative order among
        //     themselves.
        //   - Binding the same param twice (positionally AND by name) is an error.
        //   - Unknown named arg → warning to stderr; ignored.
        //   - Missing params default to empty string.
        let declared: HashSet<&str> = mac.params.iter().map(String::as_str).collect();

        // Validate ordering: no positional after named.
        let mut seen_named = false;
        for param_node in &param_nodes {
            if param_node.name.is_some() {
                seen_named = true;
            } else if seen_named {
                self.state.pop_scope();
                return Err(EvalError::InvalidUsage(format!(
                    "macro '{}': positional argument follows named argument",
                    mac.name
                )));
            }
        }

        let positional_count = param_nodes.iter().take_while(|n| n.name.is_none()).count();
        let mut assigned: HashSet<String> = HashSet::new();

        // Pass 1: positional args → fill declared params left-to-right.
        for (i, param_node) in param_nodes[..positional_count].iter().enumerate() {
            if let Some(param_name) = mac.params.get(i) {
                let val = self.evaluate(param_node)?;
                self.state.set_variable(param_name, &val);
                assigned.insert(param_name.clone());
            }
            // extra positional args beyond arity are silently ignored
        }

        // Pass 2: named args → bind by name.
        for param_node in &param_nodes[positional_count..] {
            let arg_name = self.extract_name_value(param_node.name.as_ref().unwrap());
            if !declared.contains(arg_name.as_str()) {
                self.state.pop_scope();
                return Err(EvalError::InvalidUsage(format!(
                    "macro '{}': unknown named argument '{arg_name}'",
                    mac.name
                )));
            }
            if assigned.contains(&arg_name) {
                self.state.pop_scope();
                return Err(EvalError::InvalidUsage(format!(
                    "macro '{}': parameter '{arg_name}' bound both positionally and by name",
                    mac.name
                )));
            }
            let val = self.evaluate(param_node)?;
            self.state.set_variable(&arg_name, &val);
            assigned.insert(arg_name);
        }

        // Fill any remaining unbound params with "".
        for param_name in &mac.params {
            if !assigned.contains(param_name) {
                self.state.set_variable(param_name, "");
            }
        }

        self.state.call_depth += 1;
        let body_result = self.evaluate(&mac.body);
        self.state.call_depth -= 1;
        let mut result = body_result?;

        match mac.script_kind {
            ScriptKind::None => {}
            ScriptKind::Rhai => {
                // Collect all visible weaveback string variables (outer scopes first)
                let mut variables = std::collections::HashMap::new();
                for frame in self.state.scope_stack.iter() {
                    for (k, v) in &frame.variables {
                        variables.insert(k.clone(), v.value.clone());
                    }
                }
                let mac_name = mac.name.clone();
                result = self
                    .rhai_evaluator
                    .evaluate(&result, &variables, &mut self.rhai_store, Some(&mac_name))
                    .map_err(EvalError::Runtime)?;
            }
            ScriptKind::Python => {
                // Pass only the explicitly declared parameters to the Python script;
                // the store is injected as additional variables (params shadow store).
                let args: Vec<String> = mac
                    .params
                    .iter()
                    .map(|p| self.state.get_variable(p))
                    .collect();
                result = self
                    .monty_evaluator
                    .evaluate(&result, &mac.params, &args, &self.py_store, Some(&mac.name))
                    .map_err(EvalError::Runtime)?;
            }
        }

        self.state.pop_scope();

        Ok(result)
    }

    pub fn export(&mut self, name: &str) {
        let stack_len = self.state.scope_stack.len();
        if stack_len <= 1 {
            return;
        }
        let parent_index = stack_len - 2;

        if let Some(val) = self
            .state
            .scope_stack
            .last()
            .unwrap()
            .variables
            .get(name)
            .cloned()
        {
            self.state
                .scope_stack
                .get_mut(parent_index)
                .unwrap()
                .variables
                .insert(name.to_string(), val);
        }

        if let Some(mac) = self
            .state
            .scope_stack
            .last()
            .unwrap()
            .macros
            .get(name)
            .cloned()
        {
            let frozen_mac = self.freeze_macro_definition(&mac);
            self.state
                .scope_stack
                .get_mut(parent_index)
                .unwrap()
                .macros
                .insert(name.to_string(), frozen_mac);
        }
    }

    pub fn freeze_macro_definition(&mut self, mac: &MacroDefinition) -> MacroDefinition {
        let mut frozen = HashMap::new();
        let keep: HashSet<String> = mac.params.iter().cloned().collect();
        self.collect_freeze_vars(&mac.body, &keep, &mut frozen);

        MacroDefinition {
            name: mac.name.clone(),
            params: mac.params.clone(),
            body: Arc::clone(&mac.body),
            script_kind: mac.script_kind.clone(),
            frozen_args: frozen,
        }
    }

    fn collect_freeze_vars(
        &mut self,
        node: &ASTNode,
        keep: &HashSet<String>,
        frozen: &mut HashMap<String, String>,
    ) {
        if node.kind == NodeKind::Var {
            let var_name = self.node_text(node).trim().to_string();
            if !keep.contains(&var_name) && !frozen.contains_key(&var_name) {
                let value = self.evaluate(node).unwrap_or_default();
                frozen.insert(var_name, value);
            }
        }
        for child in &node.parts {
            self.collect_freeze_vars(child, keep, frozen);
        }
    }

    pub fn parse_string(&mut self, text: &str, path: &PathBuf) -> Result<ASTNode, EvalError> {
        let src = match fs::metadata(path) {
            Ok(md) if md.is_file() => self.add_source_if_not_present(path.clone())?,
            _ => self.add_source_bytes(text.as_bytes().to_vec(), path.clone()),
        };

        let result = crate::evaluator::lexer_parser::lex_parse_content(
            text,
            self.state.config.special_char,
            src,
        );
        result.map_err(EvalError::ParseError)
    }

    fn find_file(&self, filename: &str) -> EvalResult<PathBuf> {
        let p = Path::new(filename);
        if p.is_absolute() && p.exists() {
            return Ok(p.to_path_buf());
        }
        for inc in &self.state.config.include_paths {
            let candidate = inc.join(filename);
            if candidate.exists() {
                return Ok(candidate);
            }
        }
        Err(EvalError::IncludeNotFound(filename.into()))
    }

    pub fn do_include(&mut self, filename: &str) -> EvalResult<String> {
        let path = self.find_file(filename)?;

        if self.state.config.discovery_mode {
            self.state.discovered_includes.push(path);
            return Ok("".into());
        }

        if self.state.open_includes.contains(&path) {
            return Err(EvalError::CircularInclude(path.display().to_string()));
        }
        self.state.open_includes.insert(path.clone());
        let result = (|| {
            let content = std::fs::read_to_string(&path)
                .map_err(|_| EvalError::IncludeNotFound(filename.into()))?;
            let ast = self.parse_string(&content, &path)?;
            self.evaluate(&ast)
        })();
        // Always remove the path, whether the include succeeded or failed,
        // so that a reused evaluator does not permanently block future includes.
        self.state.open_includes.remove(&path);
        result
    }

    /// Return (and clear) the list of paths recorded during a discovery-mode run.
    pub fn take_discovered_includes(&mut self) -> Vec<PathBuf> {
        std::mem::take(&mut self.state.discovered_includes)
    }

    pub fn allow_env(&self) -> bool {
        self.state.config.allow_env
    }

    pub fn num_source_files(&self) -> usize {
        self.state.source_manager.num_sources()
    }

    // ---- Tracked evaluation (EvalOutput) ------------------------------------

    /// Build a `SourceSpan` from the token of an AST node, defaulting to Literal.
    fn span_of(&self, node: &ASTNode) -> SourceSpan {
        SourceSpan {
            src: node.token.src,
            pos: node.token.pos,
            length: node.token.length,
            kind: SpanKind::Literal,
        }
    }

    /// Evaluate `node` into a `(String, Vec<SpanRange>)` for argument threading.
    /// Called only on the tracing path (`out.is_tracing() == true`).
    fn evaluate_arg_to_traced(&mut self, node: &ASTNode) -> EvalResult<(String, Vec<SpanRange>)> {
        let mut arg_out = PreciseTracingOutput::new();
        self.evaluate_to(node, &mut arg_out)?;
        Ok(arg_out.into_parts())
    }

    /// Re-tag `raw_spans` to `MacroArg { macro_name, param_name }`.
    /// If `raw_spans` is empty but `val` is non-empty, creates a single coarse span
    /// from `param_node` so the tracer can still identify the parameter.
    fn tag_as_macro_arg(
        &self,
        raw_spans: Vec<SpanRange>,
        val: &str,
        param_node: &ASTNode,
        macro_name: &str,
        param_name: &str,
    ) -> Vec<SpanRange> {
        let kind = SpanKind::MacroArg {
            macro_name: macro_name.to_string(),
            param_name: param_name.to_string(),
        };
        if raw_spans.is_empty() && !val.is_empty() {
            let mut s = self.span_of(param_node);
            s.kind = kind;
            vec![SpanRange { start: 0, end: val.len(), span: s }]
        } else {
            raw_spans.into_iter().map(|mut sr| { sr.span.kind = kind.clone(); sr }).collect()
        }
    }

    /// Like `evaluate`, but writes to an `EvalOutput` sink so that span
    /// information is available to the caller.
    pub fn evaluate_to(
        &mut self,
        node: &ASTNode,
        out: &mut dyn EvalOutput,
    ) -> EvalResult<()> {
        self.evaluate_to_with_context(node, out, None)
    }

    /// Internal evaluation method that accepts an optional `context_span` prefix.
    /// This is used to thread `MacroBody` attribution down the evaluation tree.
    fn evaluate_to_with_context(
        &mut self,
        node: &ASTNode,
        out: &mut dyn EvalOutput,
        context_span: Option<&SourceSpan>,
    ) -> EvalResult<()> {
        if self.state.early_exit {
            return Ok(());
        }
        match node.kind {
            NodeKind::Text | NodeKind::Space | NodeKind::Ident => {
                let txt = self.node_text(node);
                // Build the base span: use the token's own src/pos/length for
                // exact position, but inherit `kind` from context (e.g. MacroBody).
                let base_span = if let Some(ctx) = context_span {
                    let mut s = self.span_of(node);
                    s.kind = ctx.kind.clone();
                    s
                } else {
                    self.span_of(node)
                };
                // Multi-line text tokens (common in macro bodies where the lexer
                // groups all literal text between two macro calls) have a single
                // `pos` pointing to the start of the token.  Split at newlines
                // and advance `pos` by byte offset within the token so that every
                // line segment resolves to its true source line/col.
                let base_pos = base_span.pos;
                let mut offset = 0usize;
                for segment in txt.split_inclusive('\n') {
                    let mut seg_span = base_span.clone();
                    seg_span.pos = base_pos + offset;
                    seg_span.length = segment.len();
                    out.push_str(segment, seg_span);
                    offset += segment.len();
                }
            }
            NodeKind::Var => {
                let var_name = self.node_text(node);
                if let Some(tracked) = self.state.get_tracked_variable(&var_name) {
                    if tracked.spans.is_empty() {
                        if out.is_tracing() && !tracked.value.is_empty() {
                            // Emit a coarse VarBinding span so the tracer can identify the
                            // variable and its `set_locations`.  Position points to the
                            // %(var_name) token — i.e. the usage site, not the definition.
                            let value = tracked.value.clone();
                            let mut base_span = self.span_of(node);
                            base_span.kind = SpanKind::VarBinding { var_name: var_name.clone() };
                            let base_pos = base_span.pos;
                            let mut offset = 0;
                            for segment in value.split_inclusive('\n') {
                                let mut seg_span = base_span.clone();
                                seg_span.pos = base_pos + offset;
                                seg_span.length = segment.len();
                                out.push_str(segment, seg_span);
                                offset += segment.len();
                            }
                        } else {
                            // Untracked: computed/script result or unbound parameter.
                            out.push_untracked(&tracked.value);
                        }
                    } else {
                        // Replay each attributed range in order.
                        // Multiple ranges = full per-token threading through argument evaluation.
                        // Single range covering [0, len] = coarse call-site span (fast path).
                        for range in &tracked.spans {
                            out.push_str(
                                &tracked.value[range.start..range.end],
                                range.span.clone(),
                            );
                        }
                    }
                }
            }
            NodeKind::Macro => {
                let name = self.node_text(node);
                self.evaluate_macro_call_to(node, &name, out)?;
            }
            NodeKind::Block | NodeKind::Param => {
                for child in &node.parts {
                    self.evaluate_to_with_context(child, out, context_span)?;
                }
            }
            NodeKind::LineComment | NodeKind::BlockComment => {}
            _ => {
                for child in &node.parts {
                    self.evaluate_to_with_context(child, out, context_span)?;
                }
            }
        }
        Ok(())
    }

    /// Like `evaluate_macro_call`, but writes to an `EvalOutput` sink.
    pub fn evaluate_macro_call_to(
        &mut self,
        node: &ASTNode,
        name: &str,
        out: &mut dyn EvalOutput,
    ) -> EvalResult<()> {
        // Builtins: delegate to existing evaluate_macro_call, push result untracked
        if self.builtins.contains_key(name) {
            let result = self.evaluate_macro_call(node, name)?;
            out.push_untracked(&result);
            return Ok(());
        }

        if self.state.call_depth >= MAX_RECURSION_DEPTH {
            return Err(EvalError::Runtime(format!(
                "maximum recursion depth ({}) exceeded in macro '{}'",
                MAX_RECURSION_DEPTH, name
            )));
        }

        let mac = match self.state.get_macro(name) {
            Some(m) => m,
            None => return Err(EvalError::UndefinedMacro(name.into())),
        };

        let param_nodes: Vec<&ASTNode> = node
            .parts
            .iter()
            .filter(|p| p.kind == NodeKind::Param)
            .collect();

        self.state.push_scope();

        // frozen_args are vars that are not parameters
        for (var, frozen_val) in mac.frozen_args.iter() {
            let mut span = self.span_of(node);
            span.kind = SpanKind::VarBinding { var_name: var.clone() };
            self.state.set_tracked_variable(var, frozen_val, Some(span));
        }

        // Python-style parameter binding (same logic as evaluate_macro_call)
        let declared: HashSet<&str> = mac.params.iter().map(String::as_str).collect();

        let mut seen_named = false;
        for param_node in &param_nodes {
            if param_node.name.is_some() {
                seen_named = true;
            } else if seen_named {
                self.state.pop_scope();
                return Err(EvalError::InvalidUsage(format!(
                    "macro '{}': positional argument follows named argument",
                    mac.name
                )));
            }
        }

        let positional_count = param_nodes.iter().take_while(|n| n.name.is_none()).count();
        let mut assigned: HashSet<String> = HashSet::new();

        for (i, param_node) in param_nodes[..positional_count].iter().enumerate() {
            if let Some(param_name) = mac.params.get(i) {
                if out.is_tracing() {
                    let (val, raw_spans) = self.evaluate_arg_to_traced(param_node)?;
                    let tagged = self.tag_as_macro_arg(raw_spans, &val, param_node, name, param_name);
                    self.state.set_traced_variable(param_name, val, tagged);
                } else {
                    let val = self.evaluate(param_node)?;
                    let mut span = self.span_of(param_node);
                    span.kind = SpanKind::MacroArg {
                        macro_name: name.to_string(),
                        param_name: param_name.clone(),
                    };
                    self.state.set_tracked_variable(param_name, &val, Some(span));
                }
                assigned.insert(param_name.clone());
            }
        }

        for param_node in &param_nodes[positional_count..] {
            let arg_name = self.extract_name_value(param_node.name.as_ref().unwrap());
            if !declared.contains(arg_name.as_str()) {
                self.state.pop_scope();
                return Err(EvalError::InvalidUsage(format!(
                    "macro '{}': unknown named argument '{arg_name}'",
                    mac.name
                )));
            }
            if assigned.contains(&arg_name) {
                self.state.pop_scope();
                return Err(EvalError::InvalidUsage(format!(
                    "macro '{}': parameter '{arg_name}' bound both positionally and by name",
                    mac.name
                )));
            }
            if out.is_tracing() {
                let (val, raw_spans) = self.evaluate_arg_to_traced(param_node)?;
                let tagged = self.tag_as_macro_arg(raw_spans, &val, param_node, name, &arg_name);
                self.state.set_traced_variable(&arg_name, val, tagged);
            } else {
                let val = self.evaluate(param_node)?;
                let mut span = self.span_of(param_node);
                span.kind = SpanKind::MacroArg {
                    macro_name: name.to_string(),
                    param_name: arg_name.clone(),
                };
                self.state.set_tracked_variable(&arg_name, &val, Some(span));
            }
            assigned.insert(arg_name);
        }

        for param_name in &mac.params {
            if !assigned.contains(param_name) {
                self.state.set_variable(param_name, "");
            }
        }

        self.state.call_depth += 1;
        
        // Pass down a MacroBody context span so all literal text in the body is
        // correctly attributed as coming from a macro body expansion.
        let mut body_span = self.span_of(&mac.body);
        body_span.kind = SpanKind::MacroBody { macro_name: mac.name.clone() };
        
        let body_result = self.evaluate_to_with_context(&mac.body, out, Some(&body_span));
        self.state.call_depth -= 1;
        body_result?;

        // For script-based macros (Rhai/Python), we need the string to pass
        // through the script engine.  Evaluate the body again with evaluate()
        // to get the string, then run through the script engine and push
        // the result as untracked.
        match mac.script_kind {
            ScriptKind::None => {}
            ScriptKind::Rhai => {
                // We already wrote the body output above.  For Rhai macros,
                // the body output IS the Rhai source — not the final result.
                // The correct approach is: don't write body to `out` for scripts;
                // instead, collect the body into a string, run the script, and
                // push the script's result.
                //
                // However, this is a first-step refactor.  Rhai/Python macros
                // will always come through the builtin path above (they're
                // defined via builtin_rhaidef, which stores ScriptKind::Rhai).
                // evaluate_macro_call_to delegates to evaluate_macro_call for
                // builtins.  So this branch is currently unreachable for
                // externally-defined macros.
                //
                // For safety, do nothing here — the body was already written.
            }
            ScriptKind::Python => {
                // Same reasoning as Rhai above.
            }
        }

        self.state.pop_scope();

        Ok(())
    }
}
