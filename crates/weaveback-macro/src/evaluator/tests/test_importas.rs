// crates/weaveback-macro/src/evaluator/tests/test_importas.rs
//
// Tests for %importas(prefix, path) — imports a file and registers all
// top-level macros it defines under an additional `prefix_name` alias.
// The originals also remain so internal cross-references continue to work.

use crate::evaluator::EvalError;
use crate::macro_api::process_string_defaults;
use std::io::Write;
use tempfile::NamedTempFile;

fn write_tmp(content: &str) -> NamedTempFile {
    let mut f = NamedTempFile::new().unwrap();
    write!(f, "{content}").unwrap();
    f
}

#[test]
fn test_importas_basic_prefix() {
    let lib = write_tmp("%def(greet, name, Hello %(name)!)");
    let path = lib.path().to_str().unwrap();
    let src = format!("%importas(mylib, {path})%mylib_greet(world)");
    let result = process_string_defaults(&src).unwrap();
    assert_eq!(std::str::from_utf8(&result).unwrap(), "Hello world!");
}

#[test]
fn test_importas_prefix_multiple_macros() {
    let lib = write_tmp("%def(foo, x, foo:%(x))%def(bar, x, bar:%(x))");
    let path = lib.path().to_str().unwrap();
    let src = format!("%importas(lib, {path})%lib_foo(a)-%lib_bar(b)");
    let result = process_string_defaults(&src).unwrap();
    assert_eq!(std::str::from_utf8(&result).unwrap(), "foo:a-bar:b");
}

#[test]
fn test_importas_originals_still_accessible() {
    // Originals remain in scope so internal cross-references work.
    let lib = write_tmp("%def(greet, name, Hello %(name)!)");
    let path = lib.path().to_str().unwrap();
    let src = format!("%importas(mylib, {path})%greet(world)");
    let result = process_string_defaults(&src).unwrap();
    assert_eq!(std::str::from_utf8(&result).unwrap(), "Hello world!");
}

#[test]
fn test_importas_internal_cross_references_work() {
    // helper calls inner; importas should not break internal cross-refs.
    let lib = write_tmp("%def(inner, x, [%(x)])%def(outer, x, %outer:%inner(%(x)))");
    let path = lib.path().to_str().unwrap();
    let src = format!("%importas(lib, {path})%lib_outer(hi)");
    let result = process_string_defaults(&src).unwrap();
    assert_eq!(std::str::from_utf8(&result).unwrap(), "%outer:[hi]");
}

#[test]
fn test_importas_does_not_prefix_pre_existing_macros() {
    // A macro defined before the importas should NOT get a prefixed copy.
    let lib = write_tmp("%def(new_mac, x, %(x))");
    let path = lib.path().to_str().unwrap();
    let src = format!(
        "%def(existing, x, %(x))%importas(lib, {path})%lib_new_mac(ok)"
    );
    let result = process_string_defaults(&src).unwrap();
    assert_eq!(std::str::from_utf8(&result).unwrap(), "ok");
    // lib_existing should NOT exist
    let src2 = format!("%def(existing, x, %(x))%importas(lib, {path})%lib_existing(x)");
    assert!(matches!(
        process_string_defaults(&src2),
        Err(EvalError::UndefinedMacro(_))
    ));
}

#[test]
fn test_importas_missing_file_is_error() {
    assert!(matches!(
        process_string_defaults("%importas(lib, /nonexistent/path/file.adoc)"),
        Err(EvalError::IncludeNotFound(_))
    ));
}

#[test]
fn test_importas_missing_args_is_error() {
    assert!(matches!(
        process_string_defaults("%importas(lib)"),
        Err(EvalError::InvalidUsage(_))
    ));
    assert!(matches!(
        process_string_defaults("%importas()"),
        Err(EvalError::InvalidUsage(_))
    ));
}

#[test]
fn test_importas_prefix_is_identifier() {
    // Prefix must be a valid identifier (single_ident_param enforces this).
    let lib = write_tmp("%def(foo, x, %(x))");
    let path = lib.path().to_str().unwrap();
    // Non-identifier prefix should error
    let src = format!("%importas(bad prefix, {path})");
    assert!(process_string_defaults(&src).is_err());
}

#[test]
fn test_importas_discards_text_like_import() {
    // Like %import, text output from the included file is discarded.
    let lib = write_tmp("just text, no macros");
    let path = lib.path().to_str().unwrap();
    let src = format!("%importas(lib, {path})ok");
    let result = process_string_defaults(&src).unwrap();
    assert_eq!(std::str::from_utf8(&result).unwrap(), "ok");
}
