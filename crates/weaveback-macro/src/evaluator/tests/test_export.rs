// crates/weaveback-macro/src/evaluator/tests/test_export.rs

#[cfg(test)]
mod tests {
    use crate::evaluator::EvalError;
    use crate::macro_api::process_string_defaults;

    #[test]
    fn test_export_plain_copy() {
        // %export now does a plain upward copy — no automatic free-variable freezing.
        // Variables resolved in the macro body (e.g. via %set) are exported as
        // their evaluated string values.  Macros are copied as-is: free variables
        // in the body will be looked up dynamically at the call site.
        let source = r#"
%def(maker, base, name, %{
    %def(AFILE, param, %(base)/%(name)%(param).txt)
    %export(AFILE)

    %set(my_var, %{from %(base) import %(name)%})
    %export(my_var)
%})
%maker(one, two)
%AFILE(three)
%(my_var)
        "#;
        let mut eval = crate::evaluator::Evaluator::new(crate::evaluator::EvalConfig {
            strict_undefined_vars: false,
            ..Default::default()
        });
        let result = crate::macro_api::process_string(source, None, &mut eval)
            .expect("Processing failed");
        let output = String::from_utf8(result).expect("Output was not valid UTF-8");
        // my_var: its value was evaluated inside maker's body where base/name
        // are in scope — exported as the concrete string "from one import two".
        // AFILE: plain export means no freeze; %(base) and %(name) are unbound
        // at the global call site so they expand to "".
        let expected = "/three.txt\nfrom one import two";
        assert_eq!(output.trim(), expected, "Unexpected export output");
    }

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
