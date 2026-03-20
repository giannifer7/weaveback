#[cfg(test)]
mod tests {
    use crate::macro_api::process_string_defaults;

    #[test]
    fn test_builtin_set() {
        // The %set builtin should set variable "foo" to "bar".
        // Then, the expression "%(foo)" should expand to "bar".
        let source = "%set(foo, bar)%(foo)";
        let result =
            process_string_defaults(source).expect("Failed to process string with %set builtin");
        let output = String::from_utf8(result).expect("Output was not valid UTF-8");
        assert_eq!(output.trim(), "bar");
    }
}
