use crate::evaluator::EvalError;
use crate::macro_api::process_string_defaults;

#[test]
fn test_eval_simple_macro_call() {
    let result = process_string_defaults(
        r#"
        %def(greet, name, %{
            Hello, %(name)!
        %})
        %eval(greet, World)
        "#,
    )
    .unwrap();

    assert_eq!(String::from_utf8(result).unwrap().trim(), "Hello, World!");
}

#[test]
fn test_eval_macro_call_with_multiple_arguments() {
    let result = process_string_defaults(
        r#"
        %def(greet, name, greeting, %{
            %(greeting), %(name)!
        %})
        %eval(greet, World, Hello)
        "#,
    )
    .unwrap();

    assert_eq!(String::from_utf8(result).unwrap().trim(), "Hello, World!");
}

#[test]
fn test_eval_macro_call_with_nested_macros() {
    let result = process_string_defaults(
        r#"
        %def(get_name, %{
            World
        %})
        %def(greet, name, %{
            Hello, %(name)!
        %})
        %eval(greet, %get_name())
        "#,
    )
    .unwrap();

    assert_eq!(
        String::from_utf8(result).unwrap().trim(),
        "Hello, \n            World\n        !"
    );
}

#[test]
fn test_eval_macro_call_with_conditional_logic() {
    let result = process_string_defaults(
        r#"
        %def(greet, name, %{
            %if(%(name), %{
                Hello, %(name)!
            %}, %{
                Hello, stranger!
            %})
        %})
        %eval(greet, World)
        "#,
    )
    .unwrap();

    assert_eq!(String::from_utf8(result).unwrap().trim(), "Hello, World!");
}

#[test]
fn test_eval_macro_call_with_empty_arguments() {
    let result = process_string_defaults(
        r#"
        %def(greet, name, %{
            Hello, %(name)!
        %})
        %eval(greet, )
        "#,
    );
    assert!(
        matches!(
            result,
            Err(EvalError::UnboundParameter { ref macro_name, ref param_name })
            if macro_name == "greet" && param_name == "name"
        ),
        "expected UnboundParameter for greet/name, got: {result:?}"
    );
}

#[test]
fn test_eval_macro_call_with_whitespace_in_arguments() {
    let result = process_string_defaults(
        r#"
        %def(greet, name, %{
            Hello, %(name)!
        %})
        %eval(greet,   World  )
        "#,
    )
    .unwrap();

    assert_eq!(String::from_utf8(result).unwrap().trim(), "Hello, World  !");
}

#[test]
fn test_eval_macro_call_with_dynamic_macro_name() {
    let result = process_string_defaults(
        r#"
        %def(greet, name, %{
            Hello, %(name)!
        %})
        %def(get_macro_name, %{
            greet
        %})
        %eval(%get_macro_name(), World)
        "#,
    )
    .unwrap();

    assert_eq!(String::from_utf8(result).unwrap().trim(), "Hello, World!");
}
