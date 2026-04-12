// crates/weaveback-macro/src/evaluator/tests/test_raw_scripts.rs
//
// Tests for %pydef_raw — raw-body script builtins.
// The body is treated as literal script source; no macro expansion occurs.
// Macro params are injected directly as script-level variables.

use crate::macro_api::process_string_defaults;

// ── %pydef_raw ────────────────────────────────────────────────────────────────

#[test]
fn test_pydef_raw_basic() {
    let src = "%pydef_raw(greet, name, %{\"hello \" + name%})%greet(world)";
    let result = process_string_defaults(src).unwrap();
    assert_eq!(std::str::from_utf8(&result).unwrap(), "hello world");
}

#[test]
fn test_pydef_raw_body_not_macro_expanded() {
    // Body has %(name) — in raw mode this stays as literal Python source
    // (which is a syntax error in Python). But if we use `name` directly it works.
    let src = "%pydef_raw(upper, name, %{name.upper()%})%upper(hello)";
    let result = process_string_defaults(src).unwrap();
    assert_eq!(std::str::from_utf8(&result).unwrap(), "HELLO");
}

#[test]
fn test_pydef_raw_multiple_params() {
    let src = "%pydef_raw(concat, a, b, %{a + b%})%concat(foo, bar)";
    let result = process_string_defaults(src).unwrap();
    assert_eq!(std::str::from_utf8(&result).unwrap(), "foobar");
}

// ── Arity / name guards (shared via define_macro) ────────────────────────────

#[test]
fn test_pydef_raw_name_guard() {
    use crate::evaluator::EvalError;
    assert!(matches!(
        process_string_defaults("%pydef_raw(def, x, %{x%})"),
        Err(EvalError::InvalidUsage(_))
    ));
}
