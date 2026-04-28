---
title: |-
  Weaveback Macro Language — Design and State
toc: left
---
# Weaveback Macro Language — Design and State

This document records the design decisions behind the macro language and the
current implementation state. It is structured as a *keep / change / remove*
analysis per topic, followed by a status summary.

The goal is a language a coding model, or a human author, can use reliably
without constantly simulating weird edge cases.

## Target semantic core

A language that is:

* **locally predictable** — expressions mean what they look like where they
  are written
* **uniform** — builtins and user macros obey the same call rules
* **strict about mistakes** — wrong arity, typos, and bad forms are errors
  or warnings, not silent empty output
* **explicit about side effects** — mutations are visible at the call site
* **small enough to fit in working memory** — the full model can be described
  in a page

Desired properties:

* strict eager expansion
* arguments evaluate in **caller scope**
* one scope model
* one capture model with `%alias` only
* uniform call validation across all forms
* strict-by-default or warn-by-default diagnostics
* script bodies clearly fenced off from macro syntax

---


## 1. Scope and state

### Keep

* Stack of scope frames
* Shadowing lookup, inner scope wins
* Local `%set`
* Global frame never popped

### Implemented

Argument evaluation now happens in the **caller scope**, before the callee
frame is pushed.

Old behaviour:
`evaluate(arg)` ran inside the callee's new empty frame.
`%set(x, v)` inside an argument wrote to the callee frame, which was
immediately discarded. Side effects in argument lists were silently swallowed.

Current behaviour:

1. All arguments are evaluated in the caller's current scope.
2. A new callee frame is pushed.
3. The already-evaluated strings are bound to formal parameters.

### Demoted — still present, not in core story

Persistent script stores with `py_store` are an advanced effect
system, clearly separated from the main scope model. They bypass all frame
discipline and are not part of the core semantics.

### Coding-model rule

Expressions are evaluated where they are written, then passed as strings.

---


## 2. Capture semantics

### Keep

`%alias(new, src, k=v, …)` as explicit partial application and selective
free-variable pinning. The semantics are understandable and useful.

### Implemented

Single capture model:

* Ordinary macros use **dynamic lookup** at call time.
* `%alias(…, k=v)` is the **only** capture mechanism.
* `%export(macro)` copies the macro definition upward **as-is**, with no
  automatic free-variable freezing.

`freeze_macro_definition` has been removed. Implicit half-closure semantics
on `%export` no longer exist.

### Coding-model rule

A macro does not secretly gain a frozen environment unless
`%alias(…, k=v)` explicitly says so.

---


## 3. Argument handling

### Keep

* Positional args must precede named args.
* Unknown named arg raises an error.
* Duplicate binding, positional plus named, raises an error.

### Implemented

**Extra positional args are an error.**
A positional arity mismatch is almost always a bug; silently ignoring it hides it.

### Implemented

**Missing, unbound, parameters** are now strict by default:

* unbound formal parameters raise `UnboundParameter`
* `--no-strict-params` restores the old empty-string fallback

Possible future addition, explicit optional parameters with inline defaults:

```text
%def(fmt, level, tag, msg = "(none)", body)
```


### Coding-model rule

Wrong arity is a bug, not a tolerated shape.

---


## 4. Builtins vs. user-defined macros

### Keep

Reserved builtin namespace. Builtin names cannot be called as user macros.

### Implemented

`%def` and `%alias` attempts that use a builtin name are immediately rejected
with an error. The name guard runs inside the shared `define_macro` helper,
so it covers `%pydef` as well.

### Coding-model rule

All calls obey the same argument-shape rules regardless of target.

---


## 5. Macro redefinition

Macro redefinition is a **first-class semantic operation**, not an incidental
hazard. It supports two canonical patterns:

X-macro pattern:
A schema is defined once; successive passes redefine the `X` visitor to map
that schema into different outputs.

Context-dependent meaning:
Macro identifiers have no fixed meaning. Their expansion is determined by the
current pass, allowing the same source to be interpreted in multiple ways.

```text
%def(FIELDS, %{
  %X(name, string)
  %X(age, int)
%})

%def(X, name, type, %(name): %(type),)          <- pass 1: struct fields
%FIELDS()

%def(X, name, type, new_%(name): impl Into<%(type)>)  <- pass 2: ctor params
%FIELDS()
```


`%def` is now constant-in-frame and `%redef` is the explicit rebinding path.
There is no `@replace` marker.

### Accidental collision

If independent libraries really need separate naming, solve that at the
library-definition layer rather than relying on a second import form.

### Coding-model rule

Macro identifiers name roles, not values. Redefine to change the role.

---


## 6. Diagnostics — silent failures

### Keep

Empty string as a first-class macro result.

### Implemented

Non-fatal warnings, accumulated in `Vec<String>` and drained via `take_warnings()`:

* `%if()` called with no arguments, always expands to empty
* `%export` at global scope, no parent frame, so the call is a no-op

Errors:

* Attempt to `%def` or `%alias` a builtin name
* Extra positional arguments
* `%env(NAME)` when `--allow-env` is not set

### Implemented

* Undefined variable reference `%(typo)` is `UndefinedVariable` by default.
  `--no-strict-vars` restores the old empty fallback.
* Unbound parameters are `UnboundParameter` by default.
  `--no-strict-params` restores the old empty fallback.

### Useful future additions

* `%defined(name)` returns `1` if name is defined, empty otherwise.
* `%default(x, fallback)` returns `x` if non-empty, else `fallback`.

These let authors express *intended* emptiness instead of relying on silent
empty-on-undefined.

### Coding-model rule

Silence means success, not hidden fallback.

---


## 7. Script integration

### Keep

`%pydef` as the escape hatch for computed logic.

### Implemented

`%pydef(name, [params,] body)`:

* Defines a Python-backed macro.
* Declared parameters are injected directly as script-level variables.
* Use `%[ ... %]` or `%tag[ ... %tag]` when the body should remain literal.
* Use `%{ ... %}` when macro preprocessing of the script source is intentional.

### Coding-model rule

Inside a script body, script syntax means script syntax.
Use verbatim blocks when the body should stay literal; reserve macro-aware
blocks for code that intentionally preprocesses the script source.

---


## 8. `%here`

### Keep

The capability, source-file patching on first run, may be useful.

### Open

* Recast as an **explicit workflow primitive**, not a regular expression-forming
  builtin. Options: separate subcommand, required mode flag, or rename to
  something obviously effectful such as `%patch_here`.
* **Error** if more than one `%here` appears in a file, instead of silently
  running only the first.
* Surface the one-shot behaviour prominently in the docs.

### Coding-model rule

`%here` is a source-rewrite operation, not a normal expression-forming macro.
Generated code should not contain it.

---


## 9. Includes and imports

### Keep

`%include` and `%import` as-is.

---


## 10. Boolean predicates

### Keep

* `%if`, lazy branches evaluated only when reached

### Implemented

Canonical predicate builtins returning `1` or empty:

| Builtin | Semantics |
| --- | --- |
| `%eq(a, b)` | `1` if `a == b` byte-exact, else empty |
| `%neq(a, b)` | `1` if `a != b`, else empty |
| `%not(x)` | `1` if `x` is empty, else empty; accepts 0 or 1 args |

`%if` treats the empty string as false and any non-empty string as true.
`%not()` with no arguments returns `1`, no arg means empty means false, negation means true.

### Coding-model rule

The boolean model is: empty string is false, non-empty string is true.
Canonical predicates return `1` or empty. Models do not have to remember
which of the operands is returned.

---


## 11. Discovery mode

### Keep

The dependency-discovery feature.

### Open

Present as two separate operations in the API and documentation:

* `evaluate()`, normal expansion
* `discover_dependencies()`, collect `%include` and `%import` paths without
  expanding

### Coding-model rule

The same source text should not appear to have two radically different meanings
depending on a hidden runtime flag.

---


## Recommended authoring rules for generated macros

For immediate use in agent and coding-model prompts:

1. Never rely on undefined variables expanding to empty.
2. Never pass side-effecting expressions as arguments.
3. Use `%alias(…, k=v)` for specialization; do not rely on exported macro capture.
4. Keep `%import` at top level whenever possible.
5. Avoid `%here` in generated sources.
6. Prefer canonical predicates `%eq`, `%not`, and `%neq`.
7. Treat wrong arity as a bug, not a shape.
8. Do not redefine builtins or use builtin names for user macros.
9. Keep Python code in verbatim blocks when it should remain literal.
10. Prefer local `%set`; avoid cross-call hidden state via script stores.
11. Redefine macros freely to change the current pass or context, X-macro or context-dependent meaning.

---


## Recommended macro-use profile

### Green path — encouraged in generated code

* `%def`, `%alias(…, k=v)` for explicit specialization
* Macro redefinition for X-macro and context-dependent-meaning patterns
* `%set` local only
* `%include` and `%import` at top level only
* `%if`, `%eq`, `%not`, `%neq`
* String case transforms
* Named arguments for multi-parameter macros

### Yellow path — advanced, use carefully

* `%export` for variables only
* `%pydef` with verbatim blocks for computed logic
* `%env` only when explicitly enabled
* `%pydef` non-raw when body preprocessing is intentional

### Red path — avoid in generated code

* `%here`
* Script stores for cross-call accumulation
* Dynamic free-variable lookup across deep include chains
* Relying on silent empty undefined variables
* Side effects in argument position
* Exporting macros with implicit capture

---


## Implementation status

### Done

* Caller-scope argument evaluation
* `%export` as plain upward copy; `freeze_macro_definition` removed
* `%alias(…, k=v)` as sole capture mechanism
* Error on `%def` and `%alias` of a builtin name
* Error on extra positional arguments
* Warning on `%if()` with no arguments
* Warning on `%export` at global scope
* `%eq`, `%neq`, `%not` canonical predicates
* verbatim blocks `%[ ... %]` and `%name[ ... %name]`
* Warning infrastructure with `take_warnings()` on `Evaluator`

### Decided — not implementing

* Warning on macro redefinition. Redefinition is a first-class semantic
  operation, X-macro pattern and context-dependent meaning. No `@replace`
  annotation needed.

### Open

* Missing, unbound, parameter diagnostic
* Undefined variable reference diagnostic, likely too noisy
* `%defined(name)` and `%default(x, fallback)` helpers
* `%here` workflow reclassification
* Multiple-`%here` error
* Linter rule for naming-convention enforcement
* Discovery mode API separation
