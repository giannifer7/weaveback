use crate::macro_api::process_string_defaults;

#[test]
fn test_simple_variable_substitution() {
    // Test simple variable substitution
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
    // Test nested variable substitution
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
    // Test variable substitution with whitespace
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
fn test_variable_substitution_with_empty_string() {
    // Test variable substitution with an empty string
    let result = process_string_defaults(
        r#"
        %def(greet, name, %{
            Hello, %(name)!
        %})
        %greet()
        "#,
    )
    .unwrap();

    assert_eq!(String::from_utf8(result).unwrap().trim(), "Hello, !");
}

#[test]
fn test_variable_substitution_with_multiple_arguments() {
    // Test variable substitution with multiple arguments
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
    // Test variable substitution with a macro as an argument
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
    // Test variable substitution with conditional logic
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
fn test_variable_substitution_with_conditional_logic_empty() {
    // Test variable substitution with conditional logic and an empty string
    let result = process_string_defaults(
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
    )
    .unwrap();

    assert_eq!(
        String::from_utf8(result).unwrap().trim(),
        "Hello, stranger!"
    );
}
