use crate::evaluator::{EvalConfig, EvalError, Evaluator};
use crate::macro_api::process_string;
use tempfile::TempDir;

fn eval_default(src: &str) -> Result<String, EvalError> {
    let mut eval = Evaluator::new(EvalConfig::default());
    process_string(src, None, &mut eval).map(|b| String::from_utf8(b).unwrap())
}

#[test]
fn test_equal_wrong_arity_reports_error() {
    let err = eval_default("%equal(one)").unwrap_err();
    assert!(matches!(err, EvalError::InvalidUsage(_)));
    assert!(err.to_string().contains("equal: exactly 2 args"));
}

#[test]
fn test_equal_mismatch_returns_empty() {
    let result = eval_default("%equal(one,two)").unwrap();
    assert_eq!(result, "");
}

#[test]
fn test_include_and_import_empty_filename_return_empty() {
    assert_eq!(eval_default("%include()").unwrap(), "");
    assert_eq!(eval_default("%import()").unwrap(), "");
}

#[test]
fn test_convert_case_wrong_arity_reports_error() {
    let err = eval_default("%convert_case(one)").unwrap_err();
    assert!(matches!(err, EvalError::InvalidUsage(_)));
    assert!(err.to_string().contains("convert_case: exactly 2 args"));
}

#[test]
fn test_convert_case_empty_input_returns_empty() {
    let result = eval_default("%convert_case(, snake)").unwrap();
    assert_eq!(result, "");
}

#[test]
fn test_single_arg_case_builtins_empty_input_return_empty() {
    assert_eq!(eval_default("%capitalize()").unwrap(), "");
    assert_eq!(eval_default("%decapitalize()").unwrap(), "");
    assert_eq!(eval_default("%to_snake_case()").unwrap(), "");
}

#[test]
fn test_store_getters_require_key() {
    let rhai_err = eval_default("%rhaiget()").unwrap_err();
    let py_err = eval_default("%pyget()").unwrap_err();
    assert!(rhai_err.to_string().contains("rhaiget: requires a key"));
    assert!(py_err.to_string().contains("pyget: requires a key"));
}

#[test]
fn test_store_setters_require_two_args() {
    for src in ["%rhaiset(one)", "%rhaiexpr(one)", "%pyset(one)"] {
        let err = eval_default(src).unwrap_err();
        assert!(matches!(err, EvalError::InvalidUsage(_)));
    }
}

#[test]
fn test_import_in_discovery_mode_records_and_swallows_output() {
    let temp = TempDir::new().unwrap();
    let include_path = temp.path().join("inc.txt");
    std::fs::write(&include_path, "%def(x, y)").unwrap();

    let mut eval = Evaluator::new(EvalConfig {
        include_paths: vec![temp.path().to_path_buf()],
        discovery_mode: true,
        ..EvalConfig::default()
    });

    let result = process_string("%import(inc.txt)", None, &mut eval).unwrap();
    assert_eq!(String::from_utf8(result).unwrap(), "");
    assert_eq!(eval.take_discovered_includes(), vec![include_path]);
}
