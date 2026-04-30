// weaveback-macro/src/evaluator/builtins/control.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;

pub(in crate::evaluator::builtins) fn builtin_if(eval: &mut Evaluator, node: &ASTNode) -> EvalResult<String> {
    let parts = &node.parts;
    if parts.is_empty() {
        eval.push_warning("%if() called with no arguments — always expands to \"\"".to_string());
        return Ok("".into());
    }
    let cond = eval.evaluate(&parts[0])?;
    if !cond.is_empty() {
        if parts.len() > 1 {
            eval.evaluate(&parts[1])
        } else {
            Ok("".into())
        }
    } else {
        if parts.len() > 2 {
            eval.evaluate(&parts[2])
        } else {
            Ok("".into())
        }
    }
}

