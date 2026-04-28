---
title: |-
  Macro language reference
---
# Macro language reference

The macro expander is invoked with `%` by default. Change it with `--sigil`.

## Syntax basics

### Argument whitespace

Leading whitespace in every argument is stripped, so you can align calls
without spaces leaking into the output:

```text
%def(tag, name, value, %{<%(name)>%(value)</%(name)>%})
%tag( div,
      Hello world)
```


Output: `<div>Hello world</div>`

To include a literal leading space, wrap in a block:

```text
%tag(%{ div%}, %{ Hello world%})
```


### Named arguments

Any argument may be given a name with `identifier = value`. Named args bind by
name regardless of position and must come after all positional args:

```text
%def(greet, name, msg, %{Hello, %(name)! %(msg)%})
%greet(name = Alice, msg = %{Good morning%})
```


### Comments

| Syntax | Scope |
| --- | --- |
| `%# …` | to end of line |
| `%// …` | to end of line |
| `%-- …` | to end of line |
| `%/* … %*/` | block, nestable |

### Multi-line blocks

`%{ … %}` delimits a block that may span lines and contain commas and
parentheses without triggering argument splitting. An optional tag makes
matching pairs easier to spot:

```text
%def(page, title, body, %page{
<!DOCTYPE html>
<html><head><title>%(title)</title></head>
<body>%(body)</body></html>
%page})
```


---


## Built-in macros

###  `%def` — define a macro

```text
%def(name, param1, param2, ..., body)
```


```text
%def(greet, name, %{Hello, %(name)!%})
%greet(World)
```


Output: `Hello, World!`

Macro bodies may call other macros and contain nested `%def` calls.
Nested definitions are scoped to the invocation; use `%export` to promote them.

`%def` creates a constant binding in the current frame. Use `%redef` when
rebinding is intentional.

###  `%redef` — explicitly rebind a macro

```text
%redef(name, param1, param2, ..., body)
```


Replaces an existing rebindable binding in the current frame, or creates one
if absent. It may not overwrite a `%def` binding.

### Calling conventions

These rules apply to all macro kinds:

* Positional args fill declared params left-to-right; extras are an error.
* Named args (`param = value`) bind by name; an unknown name is an error.
* Positional args must come before named args.
* Binding the same param both ways is an error.
* Missing args are an error by default. `--no-strict-params` restores the old
  empty-string fallback.
* `%set(...)` is not allowed in argument position.

```text
%def(endpoint, method, path, handler, %{%(method) %(path) → %(handler)%})
%endpoint(GET, path = /users, handler = list_users)
```


Output: `GET /users → list_users`

###  `%set` — set a variable

```text
%set(version, 1.0.0)
Version: %(version)
```


Output: `Version: 1.0.0`

Undefined variables are an error by default. `--no-strict-vars` restores the
old empty-string fallback for `%(name)`.

###  `%if` — conditional

Empty string is falsy; any non-empty string is truthy.

```text
%set(debug, yes)
%if(%(debug), [DEBUG MODE], )
```


Output: `[DEBUG MODE]`

###  `%include` / `%import`

`%include` expands the file inline. `%import` expands it but discards the
output for loading macro definitions only.

```text
%import(macros/common.txt)
%my_macro(arg)
```


###  `%env` — read an environment variable

Requires `--allow-env`; raises an error without it.

```bash
weaveback --allow-env notes.md --gen src
```


```text
Prefix: %env(MY_PREFIX)_
```


### Case conversion

`%capitalize`, `%decapitalize`, `%to_snake_case`, `%to_camel_case`,
`%to_pascal_case`, `%to_screaming_case`

```text
%to_snake_case(MyFancyName)
```


Output: `my_fancy_name`

###  `%eval` — indirect macro call

```text
%eval(%(macro_name), arg1, arg2)
```


###  `%export` — promote to parent scope

```text
%def(init, %{
  %set(x, 10)
  %export(x)
%})
%init()
x is: %(x)
```


###  `%here` — in-place expansion

Evaluates its argument and writes the result back into the source file at the
call site. Useful for one-time code generation.

---


### Block forms

* `%{ ... %}` is a quoted argument block: one argument, still macro-active.
* `%[ ... %]` is a verbatim block: macro parsing is disabled inside.

##  `%pydef` — Python-scripted macros

```text
%pydef(name, param1, ..., body)
```


The body is evaluated by [monty](https://github.com/pydantic/monty), a
pure-Rust sandboxed Python interpreter compiled into the binary. No Python
runtime required. Only declared parameters and `%pyset` store entries are
available inside the script.

> [!NOTE]
> monty supports a subset of Python: arithmetic, string ops, `re`, basic control flow. No third-party libraries, no file I/O, no `print`.


```text
%pydef(double, x, %{str(int(x) * 2)%})
%double(21)
```


Output: `42`

###  `%pyset` / `%pyget` — Python store

```text
%pyset(key, value)   — write a string into the store
%pyget(key)          — read from the store (empty string if absent)
```


Write-back is explicit. Capture the return value with `%pyset`:

```text
%pyset(total, 0)
%pydef(add, n, %{str(int(total) + int(n))%})
%pyset(total, %add(10))
%pyset(total, %add(20))
Total: %pyget(total)
```


Output: `Total: 30`

---


## Macro redefinition and the X macro pattern

A `%redef` with the same name replaces the previous rebindable definition.
This enables the [X macro](https://en.wikipedia.org/wiki/X_macro) idiom:
define a list macro that calls a configurable inner macro `X` for each entry,
then rebind `X` before each use.

```text
%def(Colors,
  %X(Red)
  %X(Green)
  %X(Blue)
)

%redef(X, value, %{%(value),%})
typedef enum { %Colors() } Color;

%redef(X, value, %{[%(value)] = "%(value)",%})
const char *color_names[] = { %Colors() };
```


`X` need not be defined before `Colors` is defined, only before `%Colors()` is
called. Adding an entry to `Colors` automatically propagates to every
projection.

## Semantic tracing

Code generated by macros is fully traceable. Weaveback maintains a two-level
source map: generated line to intermediate noweb line to original literate
token. This allows tools like `wb-query trace` and the MCP server to pinpoint
the exact macro definition or call-site argument that produced a given line of
code.

See [architecture.adoc](architecture.adoc#_semantic_language_server_integration_weaveback_lsp)
for more on how this integrates with language servers.

Output:

```c
typedef enum { Red, Green, Blue, } Color;
const char *color_names[] = { [Red] = "Red", [Green] = "Green", [Blue] = "Blue", };
```

