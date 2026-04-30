// weaveback-macro/src/evaluator/core/types.rs
// I'd Really Rather You Didn't edit this generated file.

pub struct Evaluator {
    state: EvaluatorState,
    builtins: HashMap<String, BuiltinFn>,
    monty_evaluator: MontyEvaluator,
    py_store: HashMap<String, String>,
}

