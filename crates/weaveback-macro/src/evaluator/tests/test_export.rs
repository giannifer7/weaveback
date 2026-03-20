// crates/weaveback-macro/src/evaluator/tests/test_export.rs

#[cfg(test)]
mod tests {
    use crate::evaluator::EvalError;
    use crate::macro_api::process_string_defaults;

    #[test]
    fn test_export_macro_with_frozen_args() {
        let source = r#"
%def(macro_exporting_stuff, base, name, %{
    %def(AFILE, param, %(base)/%(name)%(param).txt)
    %export(AFILE)

    %set(my_var, %{from %(base) import %(name)%})
    %export(my_var)
%})
%macro_exporting_stuff(one, two)
%AFILE(three)
%(my_var)
        "#;
        let result = process_string_defaults(source)
            .expect("Processing failed for export with macro parameters");
        let output = String::from_utf8(result).expect("Output was not valid UTF-8");
        let expected = "one/twothree.txt\nfrom one import two";
        assert_eq!(
            output.trim(),
            expected,
            "Exported macro did not freeze outer variables as expected"
        );
    }

    /// Test that calling %export with an incorrect number of arguments produces an error.
    #[test]
    fn test_export_wrong_number_of_args() {
        let source = "%export(foo, bar)";
        let result = process_string_defaults(source);
        match result {
            Err(EvalError::InvalidUsage(msg)) => {
                assert!(
                    msg.contains("export: exactly 1 arg"),
                    "Unexpected error message: {}",
                    msg
                );
            }
            _ => panic!("Expected an InvalidUsage error when %export is called with two arguments"),
        }
    }
}
