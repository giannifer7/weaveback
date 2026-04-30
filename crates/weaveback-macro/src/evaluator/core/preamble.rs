// weaveback-macro/src/evaluator/core/preamble.rs
// I'd Really Rather You Didn't edit this generated file.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use super::builtins::{BuiltinFn, default_builtins};
use super::errors::{EvalError, EvalResult};
use super::monty_eval::MontyEvaluator;
use super::output::{EvalOutput, PreciseTracingOutput, SourceSpan, SpanKind, SpanRange};
use super::state::{EvalConfig, EvaluatorState, MacroDefinition, ScriptKind};
use crate::types::{ASTNode, NodeKind, Token, TokenKind};

