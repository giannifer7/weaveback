use crate::macro_api::process_string_defaults;

#[test]
fn test_eval_simple_macro_call() {
    // Test evaluating a simple macro call
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
    // Test evaluating a macro call with multiple arguments
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
    // Test evaluating a macro call with nested macros
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
    // Test evaluating a macro call with conditional logic
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
    // Test evaluating a macro call with empty arguments
    let result = process_string_defaults(
        r#"
        %def(greet, name, %{
            Hello, %(name)!
        %})
        %eval(greet, )
        "#,
    )
    .unwrap();

    assert_eq!(String::from_utf8(result).unwrap().trim(), "Hello, !");
}

#[test]
fn test_eval_macro_call_with_whitespace_in_arguments() {
    // Test evaluating a macro call with whitespace in arguments
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
    // Test evaluating a macro call with a dynamic macro name
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
