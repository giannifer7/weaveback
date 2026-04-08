// crates/weaveback-macro/src/evaluator/tests/test_rhai_eval.rs
use std::collections::HashMap;

use rhai::Dynamic;

use crate::evaluator::rhai_eval::{RhaiEvaluator, dynamic_to_string};

#[test]
fn test_rhai_eval_expr_helpers_work() {
    let eval = RhaiEvaluator::new();
    let value = eval
        .eval_expr("to_hex(parse_int(\" 255 \")) + \":\" + parse_float(\"3.5\").to_string()")
        .unwrap();
    assert_eq!(value.cast::<String>(), "0xFF:3.5");
}

#[test]
fn test_rhai_eval_expr_reports_error() {
    let eval = RhaiEvaluator::default();
    let err = eval.eval_expr("let =").unwrap_err();
    assert!(!err.is_empty());
}

#[test]
fn test_rhai_evaluate_without_name_uses_default_label() {
    let eval = RhaiEvaluator::default();
    let err = eval
        .evaluate("@@@", &HashMap::new(), &mut HashMap::new(), None)
        .unwrap_err();
    assert!(err.contains("rhaidef '?'"));
}

#[test]
fn test_rhai_evaluate_preserves_store_and_returns_empty_for_unit() {
    let eval = RhaiEvaluator::new();
    let mut store = HashMap::new();
    store.insert("count".to_string(), Dynamic::from(1_i64));
    let result = eval
        .evaluate("count += 1; ()", &HashMap::new(), &mut store, Some("bump"))
        .unwrap();
    assert_eq!(result, "");
    assert_eq!(store["count"].clone().cast::<i64>(), 2);
}

#[test]
fn test_rhai_scope_variables_fill_missing_store_keys_only() {
    let eval = RhaiEvaluator::new();
    let mut store = HashMap::new();
    store.insert("name".to_string(), Dynamic::from("store".to_string()));
    let vars = HashMap::from([
        ("prefix".to_string(), "hi ".to_string()),
        ("name".to_string(), "vars".to_string()),
    ]);
    let result = eval
        .evaluate("prefix + name", &vars, &mut store, Some("greet"))
        .unwrap();
    assert_eq!(result, "hi store");
}

#[test]
fn test_dynamic_to_string_covers_common_dynamic_kinds() {
    assert_eq!(dynamic_to_string(Dynamic::from("abc".to_string())), "abc");
    assert_eq!(dynamic_to_string(Dynamic::from(12_i64)), "12");
    assert_eq!(dynamic_to_string(Dynamic::UNIT), "");
}
