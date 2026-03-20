use crate::macro_api::process_string_defaults;
use std::str;

#[test]
fn test_equal_basic() {
    let result = process_string_defaults("%equal(abc, abc)").unwrap();
    assert_eq!(result, b"abc");

    let result = process_string_defaults("%equal(abc, def)").unwrap();
    assert_eq!(result, b"");
}

#[test]
fn test_equal_whitespace() {
    let result = process_string_defaults("%equal( abc , abc)").unwrap();
    assert_eq!(result, b"");

    let result = process_string_defaults("%equal( abc ,  abc )").unwrap();
    assert_eq!(str::from_utf8(&result).unwrap().trim(), "abc");

    let result = process_string_defaults("%equal(abc  , abc  )").unwrap();
    assert_eq!(str::from_utf8(&result).unwrap().trim(), "abc");
}

#[test]
fn test_equal_with_vars() {
    let result = process_string_defaults(
        r#"
        %def(set_x, val, %(val))
        %equal(%set_x(value), value)
    "#,
    )
    .unwrap();
    assert_eq!(str::from_utf8(&result).unwrap().trim(), "value");
}

#[test]
fn test_equal_errors() {
    assert!(process_string_defaults("%equal()").is_err());
    assert!(process_string_defaults("%equal(single)").is_err());
    assert!(process_string_defaults("%equal(a,b,c)").is_err());
}
