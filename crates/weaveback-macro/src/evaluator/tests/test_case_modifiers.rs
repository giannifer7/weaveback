// crates/weaveback-macro/src/evaluator/tests/test_case_modifiers.rs

use crate::macro_api::process_string_defaults;

#[test]
fn test_builtin_capitalize() {
    // Test the %capitalize macro with a direct string input
    let result = process_string_defaults(r#"%capitalize(hello)"#).unwrap();

    // Verify that the first letter is capitalized
    assert_eq!(String::from_utf8(result).unwrap().trim(), "Hello");
}

#[test]
fn test_builtin_decapitalize() {
    // Test the %decapitalize macro with a direct string input
    let result = process_string_defaults(r#"%decapitalize(HELLO)"#).unwrap();

    // Verify that the first letter is lowercased
    assert_eq!(String::from_utf8(result).unwrap().trim(), "hELLO");
}

#[test]
fn test_builtin_convert_case() {
    // Test basic conversion to snake case
    let result = process_string_defaults(r#"%convert_case(helloWorld, snake)"#).unwrap();
    assert_eq!(String::from_utf8(result).unwrap().trim(), "hello_world");

    // Test with unrecognized case style
    let result = process_string_defaults(r#"%convert_case(helloWorld, invalid)"#);
    assert!(result.is_err());

    // Test with missing arguments
    let result = process_string_defaults(r#"%convert_case(text)"#);
    assert!(result.is_err());
}

#[test]
fn test_builtin_to_snake_case() {
    // Test camelCase to snake_case
    let result = process_string_defaults(r#"%to_snake_case(helloWorld)"#).unwrap();
    assert_eq!(String::from_utf8(result).unwrap().trim(), "hello_world");

    // Test PascalCase to snake_case
    let result = process_string_defaults(r#"%to_snake_case(HelloWorld)"#).unwrap();
    assert_eq!(String::from_utf8(result).unwrap().trim(), "hello_world");

    // Test with numbers
    let result = process_string_defaults(r#"%to_snake_case(hello123World)"#).unwrap();
    assert_eq!(String::from_utf8(result).unwrap().trim(), "hello_123_world");

    // Test with existing underscores
    let result = process_string_defaults(r#"%to_snake_case(hello_world)"#).unwrap();
    assert_eq!(String::from_utf8(result).unwrap().trim(), "hello_world");
}

#[test]
fn test_builtin_to_camel_case() {
    // Test snake_case to camelCase
    let result = process_string_defaults(r#"%to_camel_case(hello_world)"#).unwrap();
    assert_eq!(String::from_utf8(result).unwrap().trim(), "helloWorld");

    // Test PascalCase to camelCase
    let result = process_string_defaults(r#"%to_camel_case(HelloWorld)"#).unwrap();
    assert_eq!(String::from_utf8(result).unwrap().trim(), "helloWorld");

    // Test with numbers
    let result = process_string_defaults(r#"%to_camel_case(hello_123_world)"#).unwrap();
    assert_eq!(String::from_utf8(result).unwrap().trim(), "hello123World");
}

#[test]
fn test_builtin_to_pascal_case() {
    // Test snake_case to PascalCase
    let result = process_string_defaults(r#"%to_pascal_case(hello_world)"#).unwrap();
    assert_eq!(String::from_utf8(result).unwrap().trim(), "HelloWorld");

    // Test camelCase to PascalCase
    let result = process_string_defaults(r#"%to_pascal_case(helloWorld)"#).unwrap();
    assert_eq!(String::from_utf8(result).unwrap().trim(), "HelloWorld");

    // Test with numbers
    let result = process_string_defaults(r#"%to_pascal_case(hello_123_world)"#).unwrap();
    assert_eq!(String::from_utf8(result).unwrap().trim(), "Hello123World");
}

#[test]
fn test_builtin_to_screaming_case() {
    // Test camelCase to SCREAMING_SNAKE_CASE
    let result = process_string_defaults(r#"%to_screaming_case(helloWorld)"#).unwrap();
    assert_eq!(String::from_utf8(result).unwrap().trim(), "HELLO_WORLD");

    // Test with numbers
    let result = process_string_defaults(r#"%to_screaming_case(hello123World)"#).unwrap();
    assert_eq!(String::from_utf8(result).unwrap().trim(), "HELLO_123_WORLD");

    // Test already uppercase
    let result = process_string_defaults(r#"%to_screaming_case(HELLO_WORLD)"#).unwrap();
    assert_eq!(String::from_utf8(result).unwrap().trim(), "HELLO_WORLD");
}

#[test]
fn test_case_conversion_edge_cases() {
    // Test empty string
    let result = process_string_defaults(r#"%to_snake_case()"#).unwrap();
    assert_eq!(String::from_utf8(result).unwrap().trim(), "");

    // Test single character
    let result = process_string_defaults(r#"%to_pascal_case(a)"#).unwrap();
    assert_eq!(String::from_utf8(result).unwrap().trim(), "A");

    // Test multiple consecutive delimiters
    let result = process_string_defaults(r#"%to_snake_case(hello___world)"#).unwrap();
    assert_eq!(String::from_utf8(result).unwrap().trim(), "hello_world");

    // Test acronyms
    let result = process_string_defaults(r#"%to_camel_case(XML_HTTP_REQUEST)"#).unwrap();
    assert_eq!(String::from_utf8(result).unwrap().trim(), "xmlHttpRequest");
}
