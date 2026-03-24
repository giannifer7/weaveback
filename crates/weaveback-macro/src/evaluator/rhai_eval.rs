// crates/weaveback-macro/src/evaluator/rhai_eval.rs

use rhai::{Dynamic, Engine, Scope};
use std::collections::HashMap;
pub struct RhaiEvaluator {
    engine: Engine,
}

impl Default for RhaiEvaluator {
    fn default() -> Self {
        Self::new()
    }
}
impl RhaiEvaluator {
    pub fn new() -> Self {
        let mut engine = Engine::new();
        engine.set_max_operations(100_000);

        engine.register_fn("parse_int", |s: &str| -> i64 {
            s.trim().parse::<i64>().unwrap_or(0)
        });
        engine.register_fn("parse_float", |s: &str| -> f64 {
            s.trim().parse::<f64>().unwrap_or(0.0)
        });
        engine.register_fn("to_hex", |n: i64| -> String { format!("0x{:X}", n) });

        Self { engine }
    }

    /// Evaluate a standalone Rhai expression and return the Dynamic result.
    /// Used by `%rhaiexpr` to initialise store entries with typed literals.
    pub fn eval_expr(&self, expr: &str) -> Result<Dynamic, String> {
        let mut scope = Scope::new();
        self.engine
            .eval_with_scope::<Dynamic>(&mut scope, expr)
            .map_err(|e| e.to_string())
    }

    /// Evaluate a rhaidef script.
    ///
    /// `variables` — weaveback string scope (injected first, lower priority)
    /// `store`     — persistent Rhai store (injected on top of variables)
    ///
    /// After the script runs, every store key whose value changed in the scope
    /// is written back into `store`, preserving full `Dynamic` types (maps,
    /// arrays, integers, …).  New variables set by the script that are not
    /// already in the store are NOT auto-persisted; use `%rhaiset` to
    /// initialise a key before its first use if you want it persisted.
    pub fn evaluate(
        &self,
        code: &str,
        variables: &HashMap<String, String>,
        store: &mut HashMap<String, Dynamic>,
        name: Option<&str>,
    ) -> Result<String, String> {
        let mut scope = Scope::new();

        // Weaveback string variables (lower priority — store values override them)
        for (k, v) in variables {
            if !store.contains_key(k) {
                scope.push_dynamic(k, Dynamic::from(v.clone()));
            }
        }

        // Store values (higher priority, full Dynamic types)
        for (k, v) in store.iter() {
            scope.push_dynamic(k, v.clone());
        }

        let result: Dynamic = self
            .engine
            .eval_with_scope(&mut scope, code)
            .map_err(|e| format!("rhaidef '{}': {}", name.unwrap_or("?"), e))?;

        // Write back any store key whose value was touched by the script
        for key in store.keys().cloned().collect::<Vec<_>>() {
            if let Some(val) = scope.get_value::<Dynamic>(&key) {
                store.insert(key, val);
            }
        }

        Ok(dynamic_to_string(result))
    }
}
pub fn dynamic_to_string(d: Dynamic) -> String {
    if d.is::<String>() {
        d.cast::<String>()
    } else if d.is_unit() {
        String::new()
    } else {
        d.to_string()
    }
}
