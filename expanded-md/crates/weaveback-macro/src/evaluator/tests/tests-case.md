# Evaluator tests — case conversion
:toc: left

link:tests.adoc[← back to test index]

Tests for the `case_conversion` module and the case-modifier builtins
(`%capitalize`, `%decapitalize`, `%convert_case`, `%to_snake_case`,
`%to_camel_case`, `%to_pascal_case`, `%to_screaming_case`).

## Case conversion (`test_case_conversion.rs`)


```rust
// <[test case conversion]>=
// crates/weaveback-macro/src/evaluator/tests/test_case_conversion.rs

use crate::evaluator::case_conversion::{Case, convert_case, convert_case_str};

#[cfg(test)]
mod basic_conversions {
    use super::*;

    #[test]
    fn test_empty_input() {
        assert_eq!(convert_case("", Case::Snake), "");
        assert_eq!(convert_case("", Case::Camel), "");
        assert_eq!(convert_case("", Case::Pascal), "");
        assert_eq!(convert_case("", Case::ScreamingKebab), "");
    }

    #[test]
    fn test_single_character() {
        assert_eq!(convert_case("a", Case::Snake), "a");
        assert_eq!(convert_case("A", Case::Snake), "a");
        assert_eq!(convert_case("1", Case::Snake), "1");
        assert_eq!(convert_case("a", Case::ScreamingKebab), "A");
    }

    #[test]
    fn test_simple_two_word() {
        let input = "hello_world";
        assert_eq!(convert_case(input, Case::Snake), "hello_world");
        assert_eq!(convert_case(input, Case::Screaming), "HELLO_WORLD");
        assert_eq!(convert_case(input, Case::Kebab), "hello-world");
        assert_eq!(convert_case(input, Case::ScreamingKebab), "HELLO-WORLD");
        assert_eq!(convert_case(input, Case::Camel), "helloWorld");
        assert_eq!(convert_case(input, Case::Pascal), "HelloWorld");
        assert_eq!(convert_case(input, Case::Ada), "Hello_World");
    }
}

#[cfg(test)]
mod delimiter_handling {
    use super::*;

    #[test]
    fn test_leading_delimiters() {
        assert_eq!(convert_case("_hello", Case::Snake), "hello");
        assert_eq!(convert_case("-world", Case::Kebab), "world");
        assert_eq!(convert_case("__test", Case::Pascal), "Test");
        assert_eq!(convert_case("-test", Case::ScreamingKebab), "TEST");
    }

    #[test]
    fn test_trailing_delimiters() {
        assert_eq!(convert_case("hello_", Case::Snake), "hello");
        assert_eq!(convert_case("world-", Case::Kebab), "world");
        assert_eq!(convert_case("test__", Case::ScreamingKebab), "TEST");
    }

    #[test]
    fn test_multiple_delimiters() {
        assert_eq!(convert_case("hello___world", Case::Snake), "hello_world");
        assert_eq!(convert_case("hello--world", Case::Kebab), "hello-world");
        assert_eq!(convert_case("hello_-_world", Case::Camel), "helloWorld");
        assert_eq!(
            convert_case("hello--world", Case::ScreamingKebab),
            "HELLO-WORLD"
        );
    }

    #[test]
    fn test_mixed_delimiters() {
        assert_eq!(
            convert_case("hello_world-test", Case::Snake),
            "hello_world_test"
        );
        assert_eq!(
            convert_case("hello-world_test", Case::Kebab),
            "hello-world-test"
        );
        assert_eq!(
            convert_case("hello_world-test", Case::ScreamingKebab),
            "HELLO-WORLD-TEST"
        );
    }
}

#[cfg(test)]
mod number_handling {
    use super::*;

    #[test]
    fn test_numbers_in_words() {
        assert_eq!(convert_case("user123name", Case::Snake), "user_123_name");
        assert_eq!(
            convert_case("user123name", Case::ScreamingKebab),
            "USER-123-NAME"
        );
        assert_eq!(convert_case("user123name", Case::Camel), "user123Name");
    }

    #[test]
    fn test_leading_numbers() {
        assert_eq!(convert_case("123name", Case::Snake), "123_name");
        assert_eq!(convert_case("123name", Case::ScreamingKebab), "123-NAME");
        assert_eq!(convert_case("123name", Case::Pascal), "123Name");
    }

    #[test]
    fn test_trailing_numbers() {
        assert_eq!(convert_case("name123", Case::Snake), "name_123");
        assert_eq!(convert_case("name123", Case::ScreamingKebab), "NAME-123");
        assert_eq!(convert_case("name123", Case::Camel), "name123");
    }
}

#[cfg(test)]
mod acronym_handling {
    use super::*;

    #[test]
    fn test_simple_acronyms() {
        assert_eq!(convert_case("parseXML", Case::Snake), "parse_xml");
        assert_eq!(convert_case("parseXML", Case::ScreamingKebab), "PARSE-XML");
        assert_eq!(convert_case("parseXML", Case::Camel), "parseXml");
    }

    #[test]
    fn test_leading_acronyms() {
        assert_eq!(convert_case("XMLParser", Case::Snake), "xml_parser");
        assert_eq!(
            convert_case("XMLParser", Case::ScreamingKebab),
            "XML-PARSER"
        );
        assert_eq!(convert_case("XMLParser", Case::Camel), "xmlParser");
    }
}

#[cfg(test)]
mod screaming_kebab_specific {
    use super::*;

    #[test]
    fn test_to_screaming_kebab_from_various_cases() {
        assert_eq!(
            convert_case("hello-world", Case::ScreamingKebab),
            "HELLO-WORLD"
        );
        assert_eq!(
            convert_case("helloWorld", Case::ScreamingKebab),
            "HELLO-WORLD"
        );
        assert_eq!(
            convert_case("HELLO_WORLD", Case::ScreamingKebab),
            "HELLO-WORLD"
        );
        assert_eq!(
            convert_case("HelloWorld", Case::ScreamingKebab),
            "HELLO-WORLD"
        );
        assert_eq!(
            convert_case("hello_world", Case::ScreamingKebab),
            "HELLO-WORLD"
        );
        assert_eq!(
            convert_case("HELLO-WORLD", Case::ScreamingKebab),
            "HELLO-WORLD"
        );
    }

    #[test]
    fn test_from_screaming_kebab() {
        let input = "HELLO-WORLD-TEST";

        assert_eq!(convert_case(input, Case::Snake), "hello_world_test");
        assert_eq!(convert_case(input, Case::Camel), "helloWorldTest");
        assert_eq!(convert_case(input, Case::Pascal), "HelloWorldTest");
        assert_eq!(convert_case(input, Case::Kebab), "hello-world-test");
        assert_eq!(convert_case(input, Case::Ada), "Hello_World_Test");
        assert_eq!(convert_case(input, Case::Screaming), "HELLO_WORLD_TEST");
        assert_eq!(convert_case(input, Case::Lower), "helloworldtest");
        assert_eq!(convert_case(input, Case::Upper), "HELLOWORLDTEST");
    }

    #[test]
    fn test_screaming_kebab_edge_cases() {
        assert_eq!(convert_case("A", Case::ScreamingKebab), "A");
        assert_eq!(convert_case("a", Case::ScreamingKebab), "A");
        assert_eq!(convert_case("-", Case::ScreamingKebab), "");
        assert_eq!(convert_case("--", Case::ScreamingKebab), "");
        assert_eq!(convert_case("-a-", Case::ScreamingKebab), "A");
        assert_eq!(convert_case("a-", Case::ScreamingKebab), "A");
        assert_eq!(convert_case("-a", Case::ScreamingKebab), "A");
    }
}

#[cfg(test)]
mod string_case_conversion {
    use super::*;

    #[test]
    fn test_valid_string_conversions() {
        assert_eq!(
            convert_case_str("hello_world", "snake").unwrap(),
            "hello_world"
        );
        assert_eq!(
            convert_case_str("hello_world", "PASCAL").unwrap(),
            "HelloWorld"
        );
        assert_eq!(
            convert_case_str("hello_world", "screaming-kebab").unwrap(),
            "HELLO-WORLD"
        );
    }

    #[test]
    fn test_case_insensitive_parsing() {
        assert_eq!(
            convert_case_str("hello_world", "SCREAMING_SNAKE").unwrap(),
            convert_case_str("hello_world", "screaming_snake").unwrap()
        );
    }

    #[test]
    fn test_invalid_case_strings() {
        assert!(convert_case_str("hello", "invalid_case").is_err());
        assert!(convert_case_str("hello", "").is_err());
        assert!(convert_case_str("hello", "unknown").is_err());
    }

    #[test]
    fn test_alternative_names() {
        assert_eq!(
            convert_case_str("test", "screaming_snake").unwrap(),
            convert_case_str("test", "SCREAMING_SNAKE_CASE").unwrap()
        );
        assert_eq!(
            convert_case_str("test", "kebab").unwrap(),
            convert_case_str("test", "kebab-case").unwrap()
        );
    }
}
// @
```


## Case modifier builtins (`test_case_modifiers.rs`)


```rust
// <[test case modifiers]>=
// crates/weaveback-macro/src/evaluator/tests/test_case_modifiers.rs

use crate::macro_api::process_string_defaults;

#[test]
fn test_builtin_capitalize() {
    let result = process_string_defaults(r#"%capitalize(hello)"#).unwrap();
    assert_eq!(String::from_utf8(result).unwrap().trim(), "Hello");
}

#[test]
fn test_builtin_decapitalize() {
    let result = process_string_defaults(r#"%decapitalize(HELLO)"#).unwrap();
    assert_eq!(String::from_utf8(result).unwrap().trim(), "hELLO");
}

#[test]
fn test_builtin_convert_case() {
    let result = process_string_defaults(r#"%convert_case(helloWorld, snake)"#).unwrap();
    assert_eq!(String::from_utf8(result).unwrap().trim(), "hello_world");

    let result = process_string_defaults(r#"%convert_case(helloWorld, invalid)"#);
    assert!(result.is_err());

    let result = process_string_defaults(r#"%convert_case(text)"#);
    assert!(result.is_err());
}

#[test]
fn test_builtin_to_snake_case() {
    let result = process_string_defaults(r#"%to_snake_case(helloWorld)"#).unwrap();
    assert_eq!(String::from_utf8(result).unwrap().trim(), "hello_world");

    let result = process_string_defaults(r#"%to_snake_case(HelloWorld)"#).unwrap();
    assert_eq!(String::from_utf8(result).unwrap().trim(), "hello_world");

    let result = process_string_defaults(r#"%to_snake_case(hello123World)"#).unwrap();
    assert_eq!(String::from_utf8(result).unwrap().trim(), "hello_123_world");

    let result = process_string_defaults(r#"%to_snake_case(hello_world)"#).unwrap();
    assert_eq!(String::from_utf8(result).unwrap().trim(), "hello_world");
}

#[test]
fn test_builtin_to_camel_case() {
    let result = process_string_defaults(r#"%to_camel_case(hello_world)"#).unwrap();
    assert_eq!(String::from_utf8(result).unwrap().trim(), "helloWorld");

    let result = process_string_defaults(r#"%to_camel_case(HelloWorld)"#).unwrap();
    assert_eq!(String::from_utf8(result).unwrap().trim(), "helloWorld");

    let result = process_string_defaults(r#"%to_camel_case(hello_123_world)"#).unwrap();
    assert_eq!(String::from_utf8(result).unwrap().trim(), "hello123World");
}

#[test]
fn test_builtin_to_pascal_case() {
    let result = process_string_defaults(r#"%to_pascal_case(hello_world)"#).unwrap();
    assert_eq!(String::from_utf8(result).unwrap().trim(), "HelloWorld");

    let result = process_string_defaults(r#"%to_pascal_case(helloWorld)"#).unwrap();
    assert_eq!(String::from_utf8(result).unwrap().trim(), "HelloWorld");

    let result = process_string_defaults(r#"%to_pascal_case(hello_123_world)"#).unwrap();
    assert_eq!(String::from_utf8(result).unwrap().trim(), "Hello123World");
}

#[test]
fn test_builtin_to_screaming_case() {
    let result = process_string_defaults(r#"%to_screaming_case(helloWorld)"#).unwrap();
    assert_eq!(String::from_utf8(result).unwrap().trim(), "HELLO_WORLD");

    let result = process_string_defaults(r#"%to_screaming_case(hello123World)"#).unwrap();
    assert_eq!(String::from_utf8(result).unwrap().trim(), "HELLO_123_WORLD");

    let result = process_string_defaults(r#"%to_screaming_case(HELLO_WORLD)"#).unwrap();
    assert_eq!(String::from_utf8(result).unwrap().trim(), "HELLO_WORLD");
}

#[test]
fn test_case_conversion_edge_cases() {
    let result = process_string_defaults(r#"%to_snake_case()"#).unwrap();
    assert_eq!(String::from_utf8(result).unwrap().trim(), "");

    let result = process_string_defaults(r#"%to_pascal_case(a)"#).unwrap();
    assert_eq!(String::from_utf8(result).unwrap().trim(), "A");

    let result = process_string_defaults(r#"%to_snake_case(hello___world)"#).unwrap();
    assert_eq!(String::from_utf8(result).unwrap().trim(), "hello_world");

    let result = process_string_defaults(r#"%to_camel_case(XML_HTTP_REQUEST)"#).unwrap();
    assert_eq!(String::from_utf8(result).unwrap().trim(), "xmlHttpRequest");
}
// @
```

