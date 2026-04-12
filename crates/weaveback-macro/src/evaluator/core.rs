// crates/weaveback-macro/src/evaluator/core.rs

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use super::builtins::{BuiltinFn, default_builtins};
use super::errors::{EvalError, EvalResult};
use super::monty_eval::MontyEvaluator;
use super::output::{EvalOutput, PreciseTracingOutput, SourceSpan, SpanKind, SpanRange};
use super::state::{EvalConfig, EvaluatorState, MAX_RECURSION_DEPTH, MacroDefinition, ScriptKind};
use crate::types::{ASTNode, NodeKind, Token, TokenKind};
pub struct Evaluator {
    state: EvaluatorState,
    builtins: HashMap<String, BuiltinFn>,
    monty_evaluator: MontyEvaluator,
    py_store: HashMap<String, String>,
}
#[derive(Clone, Copy)]
struct PositionalBinding<'a> {
    param_name: &'a str,
    param_node: &'a ASTNode,
}

#[derive(Clone)]
struct NamedBinding<'a> {
    arg_name: String,
    param_node: &'a ASTNode,
}

struct BindingPlan<'a> {
    positional: Vec<PositionalBinding<'a>>,
    named: Vec<NamedBinding<'a>>,
    unbound: Vec<&'a str>,
}
impl Evaluator {
    fn macro_param_nodes(node: &ASTNode) -> Vec<&ASTNode> {
        node.parts
            .iter()
            .filter(|p| p.kind == NodeKind::Param)
            .collect()
    }

    fn validate_param_order(param_nodes: &[&ASTNode], macro_name: &str) -> EvalResult<usize> {
        let mut seen_named = false;
        for param_node in param_nodes {
            if param_node.name.is_some() {
                seen_named = true;
            } else if seen_named {
                return Err(EvalError::InvalidUsage(format!(
                    "macro '{}': positional argument follows named argument",
                    macro_name
                )));
            }
        }
        Ok(param_nodes.iter().take_while(|n| n.name.is_none()).count())
    }

    fn validate_named_arg_binding(
        declared: &HashSet<&str>,
        assigned: &HashSet<String>,
        arg_name: &str,
        macro_name: &str,
    ) -> EvalResult<()> {
        if !declared.contains(arg_name) {
            return Err(EvalError::InvalidUsage(format!(
                "macro '{}': unknown named argument '{arg_name}'",
                macro_name
            )));
        }
        if assigned.contains(arg_name) {
            return Err(EvalError::InvalidUsage(format!(
                "macro '{}': parameter '{arg_name}' bound both positionally and by name",
                macro_name
            )));
        }
        Ok(())
    }

    fn arg_contains_builtin_call(&self, node: &ASTNode, builtin_name: &str) -> bool {
        if node.kind == NodeKind::Macro && self.node_text(node) == builtin_name {
            return true;
        }

        node.parts
            .iter()
            .any(|child| self.arg_contains_builtin_call(child, builtin_name))
    }

    fn validate_argument_side_effects(
        &self,
        param_nodes: &[&ASTNode],
        macro_name: &str,
    ) -> EvalResult<()> {
        for param_node in param_nodes {
            if self.arg_contains_builtin_call(param_node, "set") {
                return Err(EvalError::InvalidUsage(format!(
                    "macro '{macro_name}': %set is not allowed in argument position"
                )));
            }
        }
        Ok(())
    }

    fn plan_macro_bindings<'a>(
        &self,
        mac: &'a MacroDefinition,
        param_nodes: &[&'a ASTNode],
    ) -> EvalResult<BindingPlan<'a>> {
        let declared: HashSet<&str> = mac.params.iter().map(String::as_str).collect();
        let positional_count = Self::validate_param_order(param_nodes, &mac.name)?;
        let mut assigned: HashSet<String> = HashSet::new();
        let mut positional = Vec::new();
        let mut named = Vec::new();

        if positional_count > mac.params.len() {
            return Err(EvalError::InvalidUsage(format!(
                "macro '{}': {} positional argument(s) given, but only {} parameter(s) declared",
                mac.name,
                positional_count,
                mac.params.len()
            )));
        }

        for (i, param_node) in param_nodes[..positional_count].iter().enumerate() {
            let param_name = &mac.params[i];
            positional.push(PositionalBinding {
                param_name,
                param_node,
            });
            assigned.insert(param_name.clone());
        }

        for param_node in &param_nodes[positional_count..] {
            let arg_name = self.extract_name_value(param_node.name.as_ref().unwrap());
            Self::validate_named_arg_binding(&declared, &assigned, &arg_name, &mac.name)?;
            named.push(NamedBinding {
                arg_name: arg_name.clone(),
                param_node,
            });
            assigned.insert(arg_name);
        }

        let unbound = mac
            .params
            .iter()
            .filter_map(|param_name| {
                (!assigned.contains(param_name)).then_some(param_name.as_str())
            })
            .collect();

        Ok(BindingPlan {
            positional,
            named,
            unbound,
        })
    }

    pub fn new(config: EvalConfig) -> Self {
        Evaluator {
            state: EvaluatorState::new(config),
            builtins: default_builtins(),
            monty_evaluator: MontyEvaluator::new(),
            py_store: HashMap::new(),
        }
    }

    /// Access the underlying SourceManager (useful for mapping output spans back to lines/columns).
    pub fn sources(&self) -> &crate::evaluator::state::SourceManager {
        &self.state.source_manager
    }
    pub fn pystore_set(&mut self, key: String, value: String) {
        self.py_store.insert(key, value);
    }

    pub fn pystore_get(&self, key: &str) -> String {
        self.py_store.get(key).cloned().unwrap_or_default()
    }
    pub fn define_macro(&mut self, mac: crate::evaluator::state::MacroDefinition) -> EvalResult<()> {
        self.state.define_macro(mac)
    }

    pub fn redefine_macro(&mut self, mac: crate::evaluator::state::MacroDefinition) -> EvalResult<()> {
        self.state.redefine_macro(mac)
    }

    pub fn get_macro(&self, name: &str) -> Option<crate::evaluator::state::MacroDefinition> {
        self.state.get_macro(name)
    }

    pub fn is_builtin(&self, name: &str) -> bool {
        self.builtins.contains_key(name)
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

    pub fn push_warning(&mut self, msg: String) {
        self.state.push_warning(msg);
    }

    pub fn take_warnings(&mut self) -> Vec<String> {
        self.state.drain_warnings()
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

    pub fn get_sigil(&self) -> Vec<u8> {
        self.state.get_sigil()
    }

    pub fn set_early_exit(&mut self) {
        self.state.early_exit = true;
    }

    pub fn allow_env(&self) -> bool {
        self.state.config.allow_env
    }

    pub fn num_source_files(&self) -> usize {
        self.state.source_manager.num_sources()
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
                let val = match self.state.get_variable_opt(&var_name) {
                    Some(v) => v,
                    None if self.state.config.strict_undefined_vars => {
                        return Err(EvalError::UndefinedVariable(var_name));
                    }
                    None => String::new(),
                };
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

            let special_len = std::str::from_utf8(&source[start..])
                .ok()
                .and_then(|s| s.chars().next())
                .map(|c| c.len_utf8())
                .unwrap_or(1);

            let slice = match node.token.kind {
                TokenKind::BlockOpen | TokenKind::BlockClose | TokenKind::Macro => {
                    if end > start + special_len + 1 {
                        &source[(start + special_len)..(end - 1)]
                    } else {
                        &source[start..end]
                    }
                }
                TokenKind::Var => {
                    if end > start + special_len + 2 {
                        &source[(start + special_len + 1)..(end - 1)]
                    } else {
                        &source[start..end]
                    }
                }
                TokenKind::Special => {
                    if end > start + special_len {
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

        let param_nodes = Self::macro_param_nodes(node);

        // Evaluate ALL arguments in CALLER scope, before pushing the callee frame.
        // This means %(var) in an argument resolves against the caller's bindings.
        // Effectful builtins like %set are rejected in argument position.
        self.validate_argument_side_effects(&param_nodes, &mac.name)?;
        let binding_plan = self.plan_macro_bindings(&mac, &param_nodes)?;
        if self.state.config.strict_unbound_params
            && let Some(param_name) = binding_plan.unbound.first()
        {
            return Err(EvalError::UnboundParameter {
                macro_name: mac.name.clone(),
                param_name: (*param_name).to_string(),
            });
        }

        let mut positional_vals: Vec<String> = Vec::new();
        for binding in &binding_plan.positional {
            positional_vals.push(self.evaluate(binding.param_node)?);
        }
        let mut named_vals: Vec<String> = Vec::new();
        for binding in &binding_plan.named {
            named_vals.push(self.evaluate(binding.param_node)?);
        }

        self.state.push_scope();

        // frozen_args: free variables pre-bound by %alias(…, k=v) overrides
        for (var, frozen_val) in mac.frozen_args.iter() {
            self.state.set_variable(var, frozen_val);
        }

        for (binding, val) in binding_plan.positional.iter().zip(positional_vals.iter()) {
            self.state.set_variable(binding.param_name, val);
        }
        for (binding, val) in binding_plan.named.iter().zip(named_vals.iter()) {
            self.state.set_variable(&binding.arg_name, val);
        }
        for param_name in &binding_plan.unbound {
            self.state.set_variable(param_name, "");
        }

        self.state.call_depth += 1;
        let result = self.evaluate(&mac.body);
        self.state.call_depth -= 1;
        let mut result = result?;

        match mac.script_kind {
            ScriptKind::None => {}
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
            self.state.push_warning(format!(
                "%export('{name}') at global scope has no effect (no parent frame to export into)"
            ));
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
            // Plain upward copy — no automatic free-variable freezing.
            // Use %alias(new, src, k=v) for explicit capture.
            self.state
                .scope_stack
                .get_mut(parent_index)
                .unwrap()
                .macros
                .insert(name.to_string(), mac);
        }
    }
    pub fn parse_string(&mut self, text: &str, path: &PathBuf) -> Result<ASTNode, EvalError> {
        let src = match fs::metadata(path) {
            Ok(md) if md.is_file() => self.add_source_if_not_present(path.clone())?,
            _ => self.add_source_bytes(text.as_bytes().to_vec(), path.clone()),
        };

        let result = crate::evaluator::lexer_parser::lex_parse_content(
            text,
            self.state.config.sigil,
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

    /// Like `do_include`, but registers every newly-defined macro under an
    /// additional `prefix_name` alias in the current scope.  The originals
    /// also remain so that internal cross-references inside the file continue
    /// to resolve correctly.
    pub fn do_include_prefixed(&mut self, filename: &str, prefix: &str) -> EvalResult<String> {
        // Snapshot which macros already exist in the current scope frame.
        let before: std::collections::HashSet<String> = self
            .state
            .scope_stack
            .last()
            .map(|f| f.macros.keys().cloned().collect())
            .unwrap_or_default();

        self.do_include(filename)?;

        // Collect all macros that were newly defined and register them prefixed.
        let new_macros: Vec<crate::evaluator::state::MacroDefinition> = self
            .state
            .scope_stack
            .last()
            .map(|f| {
                f.macros
                    .iter()
                    .filter(|(name, _)| !before.contains(*name))
                    .map(|(_, mac)| mac.clone())
                    .collect()
            })
            .unwrap_or_default();

        for mac in new_macros {
            let prefixed_name = format!("{prefix}_{}", mac.name);
            let mut prefixed = mac;
            prefixed.name = prefixed_name;
            self.state.define_macro(prefixed)?;
        }

        Ok("".into())
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
                } else if self.state.config.strict_undefined_vars {
                    return Err(EvalError::UndefinedVariable(var_name));
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
        // Builtins: delegate to plain path, then emit with a Computed span so
        // the tracer attributes the call-site line/byte position.
        // Builtins that return "" (set, def, include, …) produce no output.
        if self.builtins.contains_key(name) {
            let result = self.evaluate_macro_call(node, name)?;
            if !result.is_empty() {
                let mut span = self.span_of(node);
                span.kind = SpanKind::Computed;
                out.push_str(&result, span);
            }
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

        let param_nodes = Self::macro_param_nodes(node);

        // Plan bindings and evaluate arguments in CALLER scope, before push_scope.
        self.validate_argument_side_effects(&param_nodes, &mac.name)?;
        let binding_plan = self.plan_macro_bindings(&mac, &param_nodes)?;
        if self.state.config.strict_unbound_params
            && let Some(param_name) = binding_plan.unbound.first()
        {
            return Err(EvalError::UnboundParameter {
                macro_name: mac.name.clone(),
                param_name: (*param_name).to_string(),
            });
        }

        // Pre-evaluate all args: (param_name, val, tagged_spans, coarse_span).
        // tagged_spans is non-empty only when out.is_tracing().
        let mut positional_pre: Vec<(String, String, Vec<SpanRange>, SourceSpan)> = Vec::new();
        for binding in &binding_plan.positional {
            if out.is_tracing() {
                let (val, raw_spans) = self.evaluate_arg_to_traced(binding.param_node)?;
                let tagged = self.tag_as_macro_arg(
                    raw_spans,
                    &val,
                    binding.param_node,
                    name,
                    binding.param_name,
                );
                positional_pre.push((
                    binding.param_name.to_string(),
                    val,
                    tagged,
                    self.span_of(binding.param_node),
                ));
            } else {
                let val = self.evaluate(binding.param_node)?;
                let mut span = self.span_of(binding.param_node);
                span.kind = SpanKind::MacroArg {
                    macro_name: name.to_string(),
                    param_name: binding.param_name.to_string(),
                };
                positional_pre.push((binding.param_name.to_string(), val, vec![], span));
            }
        }

        let mut named_pre: Vec<(String, String, Vec<SpanRange>, SourceSpan)> = Vec::new();
        for binding in &binding_plan.named {
            if out.is_tracing() {
                let (val, raw_spans) = self.evaluate_arg_to_traced(binding.param_node)?;
                let tagged = self.tag_as_macro_arg(
                    raw_spans,
                    &val,
                    binding.param_node,
                    name,
                    &binding.arg_name,
                );
                named_pre.push((
                    binding.arg_name.clone(),
                    val,
                    tagged,
                    self.span_of(binding.param_node),
                ));
            } else {
                let val = self.evaluate(binding.param_node)?;
                let mut span = self.span_of(binding.param_node);
                span.kind = SpanKind::MacroArg {
                    macro_name: name.to_string(),
                    param_name: binding.arg_name.clone(),
                };
                named_pre.push((binding.arg_name.clone(), val, vec![], span));
            }
        }

        let unbound_names: Vec<String> =
            binding_plan.unbound.iter().map(|s| s.to_string()).collect();

        // NOW push the callee frame
        self.state.push_scope();

        // frozen_args: pre-bound free variables from %alias(…, k=v) overrides
        for (var, frozen_val) in mac.frozen_args.iter() {
            let mut span = self.span_of(node);
            span.kind = SpanKind::VarBinding { var_name: var.clone() };
            self.state.set_tracked_variable(var, frozen_val, Some(span));
        }

        // Bind pre-evaluated positional args
        for (param_name, val, tagged, coarse_span) in positional_pre {
            if out.is_tracing() {
                self.state.set_traced_variable(&param_name, val, tagged);
            } else {
                self.state.set_tracked_variable(&param_name, &val, Some(coarse_span));
            }
        }

        // Bind pre-evaluated named args
        for (param_name, val, tagged, coarse_span) in named_pre {
            if out.is_tracing() {
                self.state.set_traced_variable(&param_name, val, tagged);
            } else {
                self.state.set_tracked_variable(&param_name, &val, Some(coarse_span));
            }
        }

        for param_name in &unbound_names {
            self.state.set_variable(param_name, "");
        }

        self.state.call_depth += 1;

        // Pass down a MacroBody context span so all literal text in the body is
        // correctly attributed as coming from a macro body expansion.
        let mut body_span = self.span_of(&mac.body);
        body_span.kind = SpanKind::MacroBody { macro_name: mac.name.clone() };

        let body_result = self.evaluate_to_with_context(&mac.body, out, Some(&body_span));
        self.state.call_depth -= 1;
        body_result?;

        // For script-based macros, we need the string to pass
        // through the script engine.  Evaluate the body again with evaluate()
        // to get the string, then run through the script engine and push
        // the result as untracked.
        if matches!(mac.script_kind, ScriptKind::Python) {}

        self.state.pop_scope();

        Ok(())
    }
}
