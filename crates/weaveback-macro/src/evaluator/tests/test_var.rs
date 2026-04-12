use crate::evaluator::{EvalConfig, EvalError, Evaluator};
use crate::macro_api::{process_string, process_string_defaults};

#[test]
fn test_simple_variable_substitution() {
    let result = process_string_defaults(
        r#"
        %def(greet, name, %{
            Hello, %(name)!
        %})
        %greet(World)
        "#,
    )
    .unwrap();

    assert_eq!(String::from_utf8(result).unwrap().trim(), "Hello, World!");
}

#[test]
fn test_nested_variable_substitution() {
    let result = process_string_defaults(
        r#"
        %def(greet, name, %{
            Hello, %(name)!
        %})
        %def(greet_twice, name, %{
            %greet(%(name))
            %greet(%(name))
        %})
        %greet_twice(World)
        "#,
    )
    .unwrap();

    assert_eq!(
        String::from_utf8(result).unwrap().trim(),
        "Hello, World!\n        \n            \n            Hello, World!"
    );
}

#[test]
fn test_variable_substitution_with_whitespace() {
    let result = process_string_defaults(
        r#"
        %def(greet, name, %{
            Hello, %(name)!
        %})
        %greet(  World  )
        "#,
    )
    .unwrap();

    assert_eq!(String::from_utf8(result).unwrap().trim(), "Hello, World  !");
}

#[test]
fn test_variable_substitution_with_empty_string_when_not_strict_about_params() {
    let config = EvalConfig {
        strict_unbound_params: false,
        ..EvalConfig::default()
    };
    let mut evaluator = Evaluator::new(config);
    let result = process_string(
        r#"
        %def(greet, name, %{
            Hello, %(name)!
        %})
        %greet()
        "#,
        None,
        &mut evaluator,
    )
    .unwrap();

    assert_eq!(String::from_utf8(result).unwrap().trim(), "Hello, !");
}

#[test]
fn test_variable_substitution_with_multiple_arguments() {
    let result = process_string_defaults(
        r#"
        %def(greet, name, greeting, %{
            %(greeting), %(name)!
        %})
        %greet(World, Hello)
        "#,
    )
    .unwrap();

    assert_eq!(String::from_utf8(result).unwrap().trim(), "Hello, World!");
}

#[test]
fn test_variable_substitution_with_macro_in_argument() {
    let result = process_string_defaults(
        r#"
        %def(get_name, %{
            World
        %})
        %def(greet, name, %{
            Hello, %(name)!
        %})
        %greet(%get_name())
        "#,
    )
    .unwrap();

    assert_eq!(
        String::from_utf8(result).unwrap().trim(),
        "Hello, \n            World\n        !"
    );
}

#[test]
fn test_variable_substitution_with_conditional_logic() {
    let result = process_string_defaults(
        r#"
        %def(greet, name, %{
            %if(%(name), %{
                Hello, %(name)!
            %}, %{
                Hello, stranger!
            %})
        %})
        %greet(World)
        "#,
    )
    .unwrap();

    assert_eq!(String::from_utf8(result).unwrap().trim(), "Hello, World!");
}

#[test]
fn test_strict_undefined_variable_is_error() {
    let result = process_string_defaults("before%(missing)after");
    assert!(
        matches!(result, Err(EvalError::UndefinedVariable(ref name)) if name == "missing"),
        "expected UndefinedVariable(\"missing\"), got: {result:?}"
    );
}

#[test]
fn test_no_strict_undefined_variable_is_empty() {
    let config = EvalConfig {
        strict_undefined_vars: false,
        ..EvalConfig::default()
    };
    let mut evaluator = Evaluator::new(config);
    let result = process_string("before%(missing)after", None, &mut evaluator);
    assert_eq!(String::from_utf8(result.unwrap()).unwrap(), "beforeafter");
}

#[test]
fn test_variable_substitution_with_conditional_logic_empty_when_not_strict_about_params() {
    let config = EvalConfig {
        strict_unbound_params: false,
        ..EvalConfig::default()
    };
    let mut evaluator = Evaluator::new(config);
    let result = process_string(
        r#"
        %def(greet, name, %{
            %if(%(name), %{
                Hello, %(name)!
            %}, %{
                Hello, stranger!
            %})
        %})
        %greet()
        "#,
        None,
        &mut evaluator,
    )
    .unwrap();

    assert_eq!(
        String::from_utf8(result).unwrap().trim(),
        "Hello, stranger!"
    );
}
