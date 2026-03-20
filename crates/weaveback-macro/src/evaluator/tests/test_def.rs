// crates/weaveback-macro/src/evaluator/tests/test_def.rs

use crate::evaluator::EvalError;
use crate::macro_api::process_string_defaults;

#[test]
fn test_def_macro_other_errors() {
    // Test missing arguments
    let result = process_string_defaults("%def()");
    assert!(
        matches!(result, Err(EvalError::InvalidUsage(_))),
        "Expected InvalidUsage error for empty def"
    );

    // Test single argument
    let result = process_string_defaults("%def(foo)");
    assert!(
        matches!(result, Err(EvalError::InvalidUsage(_))),
        "Expected InvalidUsage error for def with only name"
    );

    // Test numeric name
    let result = process_string_defaults("%def(123, body)");
    assert!(
        matches!(result, Err(EvalError::InvalidUsage(_))),
        "Expected InvalidUsage error for numeric macro name"
    );

    // Test numeric parameter
    let result = process_string_defaults("%def(foo, 123, body)");
    assert!(
        matches!(result, Err(EvalError::InvalidUsage(_))),
        "Expected InvalidUsage error for numeric parameter"
    );

    // Test parameter with equals
    let result = process_string_defaults("%def(foo, param=value, body)");
    assert!(
        matches!(result, Err(EvalError::InvalidUsage(_))),
        "Expected InvalidUsage error for parameter with equals"
    );
}

#[test]
fn test_def_macro_basic() {
    let result = process_string_defaults("%def(foo, bar) [%foo()]").unwrap();
    assert_eq!(result, b" [bar]");
}

#[test]
fn test_def_macro_with_params() {
    let result = process_string_defaults(
        "%def(greet, name, message, Hello, %(name)! %(message))\n%greet(Alice, Have a nice day)",
    )
    .unwrap();
    assert_eq!(
        std::str::from_utf8(&result).unwrap(),
        "\nAlice! Have a nice day"
    );
}

#[test]
fn test_def_macro_empty_body() {
    let result = process_string_defaults("%def(foo, bar,)\n%foo()").unwrap();
    assert_eq!(std::str::from_utf8(&result).unwrap(), "\n");
}

#[test]
fn test_def_macro_comma_errors() {
    let result = process_string_defaults("%def(foo bar baz, body)");
    assert!(matches!(result, Err(EvalError::InvalidUsage(_))));

    let result = process_string_defaults("%def(, foo, bar)");
    assert!(matches!(result, Err(EvalError::InvalidUsage(_))));

    let result = process_string_defaults("%def(foo,, bar, baz)");
    assert!(matches!(result, Err(EvalError::InvalidUsage(_))));
}

#[test]
fn test_def_macro_with_comments() {
    let result = process_string_defaults(
        "%def(greet, %/* greeting macro %*/\n\
         name, %// person to greet\n\
         msg, %/* message to show %*/\n\
         Hello %(name)! %(msg)\n\
         )\n\
         %greet(Alice, Good morning)",
    )
    .unwrap();
    assert_eq!(
        std::str::from_utf8(&result).unwrap(),
        "\nHello Alice! Good morning\n"
    );
}

#[test]
fn test_def_macro_spaces() {
    let result = process_string_defaults("%def( foo, bar, baz, output)\n%foo(bar, baz)").unwrap();
    assert_eq!(std::str::from_utf8(&result).unwrap(), "\noutput");
}

#[test]
fn test_def_macro_nested() {
    let result = process_string_defaults(
        "%def(bold, text, **%(text)**)
         %def(greet, name, Hello %bold(dear %(name))!)
         %greet(World)",
    )
    .unwrap();
    assert_eq!(
        std::str::from_utf8(&result).unwrap(),
        "\n         \n         Hello **dear World**!"
    );
}
