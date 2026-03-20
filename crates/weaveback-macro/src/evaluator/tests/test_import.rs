#[cfg(test)]
mod tests {
    use crate::macro_api::process_string_defaults;
    use std::io::Write;
    use tempfile::NamedTempFile;

    /// Test that `%import` imports definitions only.
    ///
    /// The temporary file contains:
    ///
    ///     This text should be discarded.
    ///     %def(included_macro, x, included: %(x)!)
    ///     More text that should be discarded.
    ///
    /// The source uses `%import` to process that file (so its text is not output)
    /// and then calls a macro that uses the included definition:
    ///
    ///     %def(macro_using_includes, param, %{
    ///         %import(TEMP_PATH)
    ///         %included_macro(%(param))
    ///     %})
    ///     %macro_using_includes(test)
    ///
    /// The expected output is:
    ///
    ///     included: test
    ///
    /// (Note: The definition for `%included_macro` is imported only within the scope of
    /// `%macro_using_includes`; it is not leaked outside.)
    #[test]
    fn test_import_includes_definitions_only() {
        // Create a temporary file that contains both text and a macro definition.
        let mut tmp = NamedTempFile::new().expect("Failed to create temporary file");
        // Write some text that should normally appear if included normally.
        writeln!(tmp, "This text should be discarded.").expect("Failed to write to temp file");
        // Write a macro definition that should update the evaluator’s state.
        writeln!(tmp, "%def(included_macro, x, included: %(x)!)")
            .expect("Failed to write macro definition to temp file");
        // Write additional text that should be discarded.
        writeln!(tmp, "More text that should be discarded.").expect("Failed to write to temp file");
        let tmp_path = tmp
            .path()
            .to_str()
            .expect("Temporary file path is not valid UTF-8");

        // Build a source that uses %import to load definitions from the temp file.
        // Then it defines a macro that calls %included_macro with its own parameter.
        let source = format!(
            r#"
%def(macro_using_includes, param, %{{
    %import({tmp_path})
    %included_macro(%(param))
%}})
%macro_using_includes(test)
"#
        );

        // Process the source.
        let result = process_string_defaults(&source);
        match result {
            Ok(output) => {
                let output_str = String::from_utf8(output).expect("Output was not valid UTF-8");
                // We expect that only the definition is used and no text from the file appears.
                let expected = "included: test!";
                assert_eq!(
                    output_str.trim(),
                    expected,
                    "Output did not match expected result. Got: {:?}",
                    output_str
                );
            }
            Err(e) => {
                panic!("Processing failed with error: {:?}", e);
            }
        }
    }
}
