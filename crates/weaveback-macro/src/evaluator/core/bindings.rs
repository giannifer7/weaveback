// weaveback-macro/src/evaluator/core/bindings.rs
// I'd Really Rather You Didn't edit this generated file.

#[derive(Clone, Copy)]
struct PositionalBinding<'a> {
    param_name: &'a str,
    param_node: &'a ASTNode,
}

#[derive(Clone)]
struct NamedBinding<'a> {
    arg_name: String,
    param_node: &'a ASTNode,
}

struct BindingPlan<'a> {
    positional: Vec<PositionalBinding<'a>>,
    named: Vec<NamedBinding<'a>>,
    unbound: Vec<&'a str>,
}

