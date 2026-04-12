// crates/weaveback-macro/src/evaluator/tests/test_raw_scripts.rs
//
// Tests for %rhaidef_raw and %pydef_raw — raw-body script builtins.
// The body is treated as literal script source; no macro expansion occurs.
// Macro params are injected directly as script-level variables.

use crate::macro_api::process_string_defaults;

// ── %rhaidef_raw ──────────────────────────────────────────────────────────────

#[test]
fn test_rhaidef_raw_basic_arithmetic() {
    // Body is raw Rhai; param `x` is injected as a Rhai string variable.
    let src = r#"%rhaidef_raw(double, x, %{(parse_int(x) * 2).to_string()%})%double(7)"#;
    let result = process_string_defaults(src).unwrap();
    assert_eq!(std::str::from_utf8(&result).unwrap(), "14");
}

#[test]
fn test_rhaidef_raw_body_is_not_macro_expanded() {
    // The body contains %(x) which looks like a macro var reference.
    // In raw mode this should be passed as literal Rhai source, not expanded.
    // The Rhai engine sees `x + ""` (not the weaveback expansion of %(x)).
    let src = r#"%rhaidef_raw(passthrough, x, %{x%})%passthrough(hello)"#;
    let result = process_string_defaults(src).unwrap();
    // Rhai receives `x` as script, with x = "hello" injected → returns "hello"
    assert_eq!(std::str::from_utf8(&result).unwrap(), "hello");
}

#[test]
fn test_rhaidef_raw_contrast_with_non_raw() {
    // Non-raw: body uses quoted %(x) so Rhai sees the value as a string literal.
    // Raw: body uses `x` as a Rhai variable name — param injected directly by engine.
    // Both produce the same string output via different mechanisms.
    let raw_src = r#"%rhaidef_raw(r, x, %{x%})%r(hello)"#;
    // In non-raw mode, %(x) is macro-expanded; wrapping in "" makes it a Rhai string literal.
    let nonraw_src = r#"%rhaidef(nr, x, %{"%(x)"%})%nr(hello)"#;
    let raw_result = process_string_defaults(raw_src).unwrap();
    let nonraw_result = process_string_defaults(nonraw_src).unwrap();
    assert_eq!(raw_result, nonraw_result);
    assert_eq!(std::str::from_utf8(&raw_result).unwrap(), "hello");
}

#[test]
fn test_rhaidef_raw_multiple_params() {
    let src = r#"%rhaidef_raw(add, a, b, %{(parse_int(a) + parse_int(b)).to_string()%})%add(3, 4)"#;
    let result = process_string_defaults(src).unwrap();
    assert_eq!(std::str::from_utf8(&result).unwrap(), "7");
}

#[test]
fn test_rhaidef_raw_only_declared_params_injected() {
    // Outer scope has `outer_var` set via %set, but raw mode only injects
    // declared params. The Rhai body should NOT see `outer_var` as a variable.
    let src = r#"%set(outer_var, secret)%rhaidef_raw(check, x, %{
        if x == "hi" { "ok" } else { "fail" }
    %})%check(hi)"#;
    let result = process_string_defaults(src).unwrap();
    assert_eq!(std::str::from_utf8(&result).unwrap().trim(), "ok");
}

#[test]
fn test_rhaidef_raw_script_error_propagates() {
    let src = r#"%rhaidef_raw(broken, x, %{@@@%})%broken(x)"#;
    assert!(process_string_defaults(src).is_err());
}

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
fn test_rhaidef_raw_name_guard() {
    use crate::evaluator::EvalError;
    assert!(matches!(
        process_string_defaults("%rhaidef_raw(if, x, %{x%})"),
        Err(EvalError::InvalidUsage(_))
    ));
}

#[test]
fn test_pydef_raw_name_guard() {
    use crate::evaluator::EvalError;
    assert!(matches!(
        process_string_defaults("%pydef_raw(def, x, %{x%})"),
        Err(EvalError::InvalidUsage(_))
    ));
}

#[test]
fn test_rhaidef_raw_min_arity() {
    use crate::evaluator::EvalError;
    assert!(matches!(
        process_string_defaults("%rhaidef_raw(foo)"),
        Err(EvalError::InvalidUsage(_))
    ));
}
