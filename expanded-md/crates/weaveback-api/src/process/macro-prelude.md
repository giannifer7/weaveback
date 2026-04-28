# Single-Pass Macro Prelude Helpers

## Macro Prelude Helpers

```rust
// <[process-macro-prelude]>=
pub(super) fn evaluate_macro_preludes(
    evaluator: &mut Evaluator,
    preludes: &[PathBuf],
) -> Result<(), String> {
    for prelude in preludes {
        let content = std::fs::read_to_string(prelude)
            .map_err(|e| format!("{}: {e}", prelude.display()))?;
        process_string(&content, Some(prelude), evaluator)
            .map_err(|e| format!("{}: {e}", prelude.display()))?;
    }
    Ok(())
}
// @
```

