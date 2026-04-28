// weaveback-api/src/process/macro_prelude.rs
// I'd Really Rather You Didn't edit this generated file.

use std::path::PathBuf;

use weaveback_macro::evaluator::Evaluator;
use weaveback_macro::macro_api::process_string;

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

