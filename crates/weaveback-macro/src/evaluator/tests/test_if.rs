use crate::macro_api::process_string_defaults;

#[test]
fn test_if_condition_true() {
    let result = process_string_defaults(
        r#"
        %if(true, %{
            This should be printed.
        %})
        "#,
    )
    .unwrap();

    assert_eq!(
        String::from_utf8(result).unwrap().trim(),
        "This should be printed."
    );
}

#[test]
fn test_if_condition_false() {
    let result = process_string_defaults(
        r#"
        %if(  , %{
            This should NOT be printed.
        %})
        "#,
    )
    .unwrap();

    assert_eq!(String::from_utf8(result).unwrap().trim(), "");
}

#[test]
fn test_if_else_condition_true() {
    let result = process_string_defaults(
        r#"
        %if(true, %{
            This should be printed.
        %}, %{
            This should NOT be printed.
        %})
        "#,
    )
    .unwrap();

    assert_eq!(
        String::from_utf8(result).unwrap().trim(),
        "This should be printed."
    );
}

#[test]
fn test_if_else_condition_false() {
    let result = process_string_defaults(
        r#"
        %if(, %{
            This should NOT be printed.
        %}, %{
            This should be printed.
        %})
        "#,
    )
    .unwrap();

    assert_eq!(
        String::from_utf8(result).unwrap().trim(),
        "This should be printed."
    );
}

#[test]
fn test_nested_if_conditions() {
    let result = process_string_defaults(
        r#"
        %if(true, %{
            %if(true, %{
                Nested condition is true.
            %})
        %})
        "#,
    )
    .unwrap();

    assert_eq!(
        String::from_utf8(result).unwrap().trim(),
        "Nested condition is true."
    );
}

#[test]
fn test_if_with_macro_condition() {
    let result = process_string_defaults(
        r#"
        %def(empty,)
        %if(%empty(), , %{condition is false.%})
        "#,
    )
    .unwrap();

    assert_eq!(
        String::from_utf8(result).unwrap().trim(),
        "condition is false."
    );
}
