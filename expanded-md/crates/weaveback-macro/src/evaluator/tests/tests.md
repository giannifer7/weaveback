---
title: |-
  Evaluator tests
toc: left
---
# Evaluator tests

The evaluator test suite covers every built-in macro, the Python
scripting back-end, the output-sink tracing infrastructure, and the public
eval API.  Tests live under `src/evaluator/tests/` and are gated by
`#[cfg(test)]` in `mod.rs`.

## Focused test pages

* link:tests-macros.adoc[tests-macros.adoc] — `%def`, `%set`, `%env`, variable substitution, test helpers, builtin edge cases
* link:tests-control.adoc[tests-control.adoc] — `%if`, `%include`, `%import`, `%export`, `%eval`, `%here`
* link:tests-case.adoc[tests-case.adoc] — case conversion module + case-modifier builtins
* link:tests-scripting.adoc[tests-scripting.adoc] — Python (`%pydef`, `%pyset`, …) scripting + engine unit tests
* link:tests-output.adoc[tests-output.adoc] — output sinks, eval API, macro API, SKILL.md examples
* link:tests-core.adoc[tests-core.adoc] — `Evaluator` core API, `EvaluatorState`, `SourceManager`, `modify_source`

## Test organisation

<table>
  <tr><th>Module</th><th>Coverage</th></tr>
  <tr><td>`test_utils`</td><td>Shared helpers: `config_in_temp_dir`, `evaluator_in_temp_dir`</td></tr>
  <tr><td>`test_macros`</td><td>`%def` basic call, parameters, nested, scope isolation</td></tr>
  <tr><td>`test_def`</td><td>`%def` error paths: missing args, numeric names, duplicate params, `=`-style params</td></tr>
  <tr><td>`test_var`</td><td>Variable substitution through `%def` parameter binding</td></tr>
  <tr><td>`test_set`</td><td>`%set` builtin: sets a variable in the current scope</td></tr>
  <tr><td>`test_if`</td><td>`%if` conditionals: truthy/falsy strings, nested, macro conditions</td></tr>
  <tr><td>`test_include`</td><td>`%include`: basic, macros, missing file, circular detection, symlinks, scope,<br>
`open_includes` cleanup on error (regression for bug #6)</td></tr>
  <tr><td>`test_import`</td><td>`%import`: definitions-only include; text output discarded</td></tr>
  <tr><td>`test_export`</td><td>`%export`: frozen arguments, wrong-arity error</td></tr>
  <tr><td>`test_eval`</td><td>`%eval`: dynamic macro dispatch by name, nested macros, empty args</td></tr>
  <tr><td>`test_here`</td><td>`%here`: source-file patching via `modify_source`</td></tr>
  <tr><td>`test_case_conversion`</td><td>`Case` enum, `convert_case`, `convert_case_str`: all nine styles, delimiters,<br>
numbers, acronyms, SCREAMING-KEBAB edge cases</td></tr>
  <tr><td>`test_case_modifiers`</td><td>`%capitalize`, `%decapitalize`, `%convert_case`, `%to_snake_case`,<br>
`%to_camel_case`, `%to_pascal_case`, `%to_screaming_case`</td></tr>
  <tr><td>`test_pydef`</td><td>`%pydef` arithmetic, multi-param, greet, error propagation;<br>
`%pyset`/`%pyget` store operations, param shadowing store key</td></tr>
  <tr><td>`test_output`</td><td>`PlainOutput` parity with `evaluate()`; `SpyOutput` (test-only sink);<br>
`TracingOutput` per-line spans; `into_macro_map_entries`</td></tr>
  <tr><td>`test_eval_api`</td><td>`eval_string_with_defaults`, `eval_file_with_config`, `eval_files_with_config`:<br>
basic, include, error, nested macros, sigil, shared macros</td></tr>
  <tr><td>`test_macro_api`</td><td>`process_string`, `process_file`, `process_files_from_config`, `discover_includes_in_file` at the macro_api layer</td></tr>
  <tr><td>`test_skill_examples`</td><td>Exact examples from `SKILL.md`: positional/named params, arity edge cases,<br>
dynamic vs lexical scoping, `%export` freeze</td></tr>
  <tr><td>`test_predicates`</td><td>`%eq`, `%neq`, `%not`: equality, inequality, logical negation, arity errors,<br>
canonical boolean output, integration with `%if`</td></tr>
  <tr><td>`test_raw_scripts`</td><td>Verbatim `%[ ... %]` blocks inside `%pydef`: literal script bodies,<br>
param injection, and contrast with macro-aware `%{ ... %}` blocks</td></tr>
  <tr><td>`test_warnings`</td><td>Warning infrastructure: `%export` at global scope, `%if()` with no args,<br>
`take_warnings()` drains the list, `%export` inside macro does not warn</td></tr>
</table>

## File structure

```rust
// <[@file weaveback-macro/src/evaluator/tests/mod.rs]>=
// weaveback-macro/src/evaluator/tests/mod.rs
// I'd Really Rather You Didn't edit this generated file.

// <[tests mod]>

// @
```

```rust
// <[@file weaveback-macro/src/evaluator/tests/test_utils.rs]>=
// weaveback-macro/src/evaluator/tests/test_utils.rs
// I'd Really Rather You Didn't edit this generated file.

// <[test utils]>

// @
```

```rust
// <[@file weaveback-macro/src/evaluator/tests/test_macros.rs]>=
// weaveback-macro/src/evaluator/tests/test_macros.rs
// I'd Really Rather You Didn't edit this generated file.

// <[test macros]>

// @
```

```rust
// <[@file weaveback-macro/src/evaluator/tests/test_def.rs]>=
// weaveback-macro/src/evaluator/tests/test_def.rs
// I'd Really Rather You Didn't edit this generated file.

// <[test def]>

// @
```

```rust
// <[@file weaveback-macro/src/evaluator/tests/test_var.rs]>=
// weaveback-macro/src/evaluator/tests/test_var.rs
// I'd Really Rather You Didn't edit this generated file.

// <[test var]>

// @
```

```rust
// <[@file weaveback-macro/src/evaluator/tests/test_set.rs]>=
// weaveback-macro/src/evaluator/tests/test_set.rs
// I'd Really Rather You Didn't edit this generated file.

// <[test set]>

// @
```

```rust
// <[@file weaveback-macro/src/evaluator/tests/test_if.rs]>=
// weaveback-macro/src/evaluator/tests/test_if.rs
// I'd Really Rather You Didn't edit this generated file.

// <[test if]>

// @
```

```rust
// <[@file weaveback-macro/src/evaluator/tests/test_include.rs]>=
// weaveback-macro/src/evaluator/tests/test_include.rs
// I'd Really Rather You Didn't edit this generated file.

// <[test include]>

// @
```

```rust
// <[@file weaveback-macro/src/evaluator/tests/test_import.rs]>=
// weaveback-macro/src/evaluator/tests/test_import.rs
// I'd Really Rather You Didn't edit this generated file.

// <[test import]>

// @
```

```rust
// <[@file weaveback-macro/src/evaluator/tests/test_export.rs]>=
// weaveback-macro/src/evaluator/tests/test_export.rs
// I'd Really Rather You Didn't edit this generated file.

// <[test export]>

// @
```

```rust
// <[@file weaveback-macro/src/evaluator/tests/test_eval.rs]>=
// weaveback-macro/src/evaluator/tests/test_eval.rs
// I'd Really Rather You Didn't edit this generated file.

// <[test eval]>

// @
```

```rust
// <[@file weaveback-macro/src/evaluator/tests/test_here.rs]>=
// weaveback-macro/src/evaluator/tests/test_here.rs
// I'd Really Rather You Didn't edit this generated file.

// <[test here]>

// @
```

```rust
// <[@file weaveback-macro/src/evaluator/tests/test_case_conversion.rs]>=
// weaveback-macro/src/evaluator/tests/test_case_conversion.rs
// I'd Really Rather You Didn't edit this generated file.

// <[test case conversion]>

// @
```

```rust
// <[@file weaveback-macro/src/evaluator/tests/test_case_modifiers.rs]>=
// weaveback-macro/src/evaluator/tests/test_case_modifiers.rs
// I'd Really Rather You Didn't edit this generated file.

// <[test case modifiers]>

// @
```

```rust
// <[@file weaveback-macro/src/evaluator/tests/test_pydef.rs]>=
// weaveback-macro/src/evaluator/tests/test_pydef.rs
// I'd Really Rather You Didn't edit this generated file.

// <[test pydef]>

// @
```

```rust
// <[@file weaveback-macro/src/evaluator/tests/test_output.rs]>=
// weaveback-macro/src/evaluator/tests/test_output.rs
// I'd Really Rather You Didn't edit this generated file.

// <[test output]>

// @
```

```rust
// <[@file weaveback-macro/src/evaluator/tests/test_eval_api.rs]>=
// weaveback-macro/src/evaluator/tests/test_eval_api.rs
// I'd Really Rather You Didn't edit this generated file.

// <[test eval api]>

// @
```

```rust
// <[@file weaveback-macro/src/evaluator/tests/test_macro_api.rs]>=
// weaveback-macro/src/evaluator/tests/test_macro_api.rs
// I'd Really Rather You Didn't edit this generated file.

// <[test macro api]>

// @
```

```rust
// <[@file weaveback-macro/src/evaluator/tests/test_skill_examples.rs]>=
// weaveback-macro/src/evaluator/tests/test_skill_examples.rs
// I'd Really Rather You Didn't edit this generated file.

// <[test skill examples]>

// @
```

```rust
// <[@file weaveback-macro/src/evaluator/tests/test_lexer_parser.rs]>=
// weaveback-macro/src/evaluator/tests/test_lexer_parser.rs
// I'd Really Rather You Didn't edit this generated file.

// <[test lexer parser]>

// @
```

```rust
// <[@file weaveback-macro/src/evaluator/tests/test_core.rs]>=
// weaveback-macro/src/evaluator/tests/test_core.rs
// I'd Really Rather You Didn't edit this generated file.

// <[test core]>

// @
```

```rust
// <[@file weaveback-macro/src/evaluator/tests/test_state.rs]>=
// weaveback-macro/src/evaluator/tests/test_state.rs
// I'd Really Rather You Didn't edit this generated file.

// <[test state]>

// @
```

```rust
// <[@file weaveback-macro/src/evaluator/tests/test_source_utils.rs]>=
// weaveback-macro/src/evaluator/tests/test_source_utils.rs
// I'd Really Rather You Didn't edit this generated file.

// <[test source utils]>

// @
```

```rust
// <[@file weaveback-macro/src/evaluator/tests/test_env.rs]>=
// weaveback-macro/src/evaluator/tests/test_env.rs
// I'd Really Rather You Didn't edit this generated file.

// <[test env]>

// @
```

```rust
// <[@file weaveback-macro/src/evaluator/tests/test_builtins_misc.rs]>=
// weaveback-macro/src/evaluator/tests/test_builtins_misc.rs
// I'd Really Rather You Didn't edit this generated file.

// <[test builtins misc]>

// @
```

```rust
// <[@file weaveback-macro/src/evaluator/tests/test_monty_eval.rs]>=
// weaveback-macro/src/evaluator/tests/test_monty_eval.rs
// I'd Really Rather You Didn't edit this generated file.

// <[test monty eval]>

// @
```


## `tests/mod.rs` — module registry

```rust
// <[tests mod]>=
// crates/weaveback-macro/src/evaluator/tests/mod.rs
mod test_builtins_misc;
mod test_case_conversion;
mod test_case_modifiers;
mod test_core;
mod test_def;
mod test_env;
mod test_eval;
mod test_export;
mod test_here;
mod test_if;
mod test_import;
mod test_include;
mod test_macro_api;
mod test_macros;
mod test_monty_eval;
mod test_pydef;
mod test_set;
mod test_skill_examples;
mod test_source_utils;
mod test_state;
mod test_utils;
mod test_var;
mod test_output;
mod test_lexer_parser;
mod test_eval_api;
mod test_predicates;
mod test_raw_scripts;
mod test_warnings;
// @
```


## `tests/test_lexer_parser.rs` — lex/parse error paths

```rust
// <[test lexer parser]>=
// crates/weaveback-macro/src/evaluator/tests/test_lexer_parser.rs
use crate::evaluator::lexer_parser::lex_parse_content;

#[test]
fn lex_parse_content_plain_text_succeeds() {
    let ast = lex_parse_content("hello world", '%', 0).unwrap();
    let _ = ast;
}

#[test]
fn lex_parse_content_empty_string_succeeds() {
    let ast = lex_parse_content("", '%', 0).unwrap();
    let _ = ast;
}

#[test]
fn lex_parse_content_unclosed_var_is_lex_error() {
    // %(foo without closing ) triggers a lex error
    let err = lex_parse_content("%(foo", '%', 0).unwrap_err();
    assert!(err.contains("Lexer errors"), "expected lex error, got: {err}");
}

#[test]
fn lex_parse_content_unclosed_arg_list_is_lex_error() {
    // %foo( without closing ) triggers "Unclosed macro argument list"
    let err = lex_parse_content("%foo(bar", '%', 0).unwrap_err();
    assert!(err.contains("Lexer errors"), "expected lex error, got: {err}");
}

#[test]
fn lex_parse_content_macro_call_succeeds() {
    let ast = lex_parse_content("%foo(bar, baz)", '%', 0).unwrap();
    let _ = ast;
}
// @
```

