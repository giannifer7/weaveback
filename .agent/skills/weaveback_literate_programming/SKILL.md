---
name: Weaveback Literate Programming
description: Guidelines and patterns for working in codebases that use the weaveback literate programming toolchain.
---

# Weaveback Literate Programming

Weaveback is a literate programming toolchain. Source files are written as
annotated documents (Markdown, AsciiDoc, etc.) containing macro calls and
named code chunks. The `weaveback` command processes them and writes the final
source files.

## Typical invocation

```bash
weaveback source.md --gen src
weaveback --dir docs --ext adoc --include . --gen src
```

The `--dir` mode recursively discovers driver files (those not `%include`d
by another file) and processes each one. No command-line changes are needed
when new files are added.

## Two passes, one command

`weaveback` runs two passes in sequence, in-process:

1. **weaveback-macro** — expands `%macro(...)` calls into an intermediate
   noweb document
2. **weaveback-tangle** — extracts `<[chunk]>` references and writes output files

The separate `weaveback-macro` and `weaveback-tangle` binaries exist for advanced
pipeline use but are not needed for normal work.

## Chunk syntax (defaults)

The comment marker before a delimiter matches the language of the chunk's
content. Use `//` for Rust, C, and similar; use `#` for Python, shell, TOML,
etc. For weaveback source or plain text there is no host language, so omit the
comment marker entirely.

**Rust** (`//` comment marker):

```rust
// <[@file src/lib.rs]>=
pub mod utils;
// <[utils-module]>
// @

// <[utils-module]>=
pub fn helper() {}
// @
```

**Weaveback / plain text** (no comment marker):

```weaveback
<[@file config/default.toml]>=
[server]
port = <[server-port]>
@

<[server-port]>=
8080
@
```

- `<[@file path]>=` — declares a file output chunk; path may start with `~/`
- `<[name]>=` — declares a named chunk
- `<[name]>` inside a chunk body — expands that chunk inline, preserving indentation
- `// @` / `# @` / `@` — ends the current chunk (marker must match what precedes delimiters)
- Comment markers before delimiters are stripped automatically; defaults are
  `#` and `//` but any set can be configured via `--comment-markers`
  (e.g. `--comment-markers "--,;;"` for Lua/Scheme)

Delimiters are configurable: `--open-delim`, `--close-delim`, `--chunk-end`.

## Macro syntax

```
%def(name, param1, ..., body)   — define a macro
%(varname)                      — interpolate a variable
%set(name, value)               — set a variable
%if(cond, then, else)           — conditional
%include(path)                  — include another file
%import(path)                   — include but discard output (load definitions)
%rhaidef(name, params..., body) — Rhai-scripted macro (logic, arithmetic)
%pydef(name, params..., body)   — Python-scripted macro (via monty)
```

Always wrap macro bodies in `%{ ... %}` — required when they contain commas
or parentheses, and good style otherwise. Wrap non-trivial arguments too.
Leading whitespace inside `%{` is preserved, but leading whitespace on bare
arguments is stripped — which makes multi-line calls with comments readable:

```
%def(tag, name, value, %{<%(name)>%(value)</%(name)>%})

%tag( div,         %# element name — leading space stripped
      Hello world) %# value        — leading space stripped
```
Output: `<div>Hello world</div>`

To keep a leading space, use `%{`:
```
%tag(%{ div%}, %{ Hello world%})
```
Output: `< div> Hello world</ div>`

These calling conventions apply to all macro kinds (`%def`, `%rhaidef`, `%pydef`):
named parameters are matched **by name** (any order), positional args must come
before named args (Python-style), an unknown name is an error (catches typos),
extra positional args beyond the declared count are ignored, missing params
default to empty string. Combined with multi-line style and comments
they serve as self-documenting call sites:

```
%def(http_endpoint, method, path, handler, %{
%(method) %(path) → %(handler)
%})

%http_endpoint(
    method  = GET,          %# HTTP verb
    path    = /api/users,   %# route pattern
    handler = list_users)   %# function name
```
Output: `GET /api/users → list_users`

Named and positional arguments can be mixed, but positional args must come
first (same rule as Python). Named args following them bind by name;
a positional after a named arg is an error, as is providing the same param
both positionally and by name.

## Build system integration

`--depfile` writes a Makefile-format depfile after each run; `--stamp`
touches a file on success. Together they let a single build rule cover an
entire directory tree:

```meson
custom_target('gen',
  output  : ['gen.stamp'],
  depfile : 'gen.d',
  command : [weaveback,
             '--dir',    meson.current_source_dir() / 'src',
             '--ext',    'adoc',
             '--include', meson.current_source_dir(),
             '--gen',    meson.current_source_dir() / 'src',
             '--stamp',  '@OUTPUT0@',
             '--depfile', '@DEPFILE@'],
)
```

> List only the stamp in `output`, never the `.d` file — Ninja consumes
> depfiles into its internal database and will rerun forever if the `.d`
> file is also declared as an output.

## Source tracing

`weaveback` records a source map on every run. Given a line (and optionally a
column) in a generated file, `weaveback trace` returns the exact literate source
location — essential when a compiler error points at generated code.

```bash
# Trace line 42 of a generated file
weaveback trace src/foo.rs 42

# Pinpoint a specific token on that line (column is 0-indexed)
weaveback trace src/foo.rs 42 --col 10
```

Reads `weaveback.db` from the current directory. Pass `--db` and `--gen` for
non-default paths.

**Output fields:**

| Field | Meaning |
|-------|---------|
| `src_file` | Literate source file to edit |
| `src_line` | 1-indexed line in that file |
| `kind` | `Literal`, `MacroBody`, `MacroArg`, `VarBinding`, or `Computed` |
| `macro_name` | Macro name (when `kind` is `MacroBody` or `MacroArg`) |
| `param_name` | Parameter name (when `kind` is `MacroArg`) |
| `var_name` | Variable name (when `kind` is `VarBinding`) |
| `def_locations` | `{file, line}` for every `%def`/`%rhaidef`/`%pydef` that defined this macro (when `kind` is `MacroBody`) |
| `set_locations` | `{file, line}` for every `%set` that set this variable (when `kind` is `VarBinding`) |
| `chunk` | Noweb chunk containing this line |

**Reading the result:**

- `Literal`: edit `src_file` at `src_line` directly.
- `MacroBody`: the text is a literal fragment of the macro body. Edit the
  macro definition — `def_locations` says where it was defined.
- `MacroArg`: the text came from an argument at the call site.
  `src_file:src_line` is that call site; `param_name` names the parameter.
- `VarBinding`: the text came from a `%set` call. `set_locations` lists all
  assignment sites; `var_name` names the variable.

When a line contains tokens from different sources, use `--col` to target the
specific token you want to change. Span attribution follows arguments through
nested macro calls — `src_file:src_line` always points to the original literal
text, not to an intermediate call site.

## Apply-back

`weaveback apply-back` is a **first-class editing workflow**: make changes directly
in the generated files — using your IDE, language-aware tools, or just your
editor — then run apply-back to propagate every change back to the literate
source automatically. This is often faster than locating the right spot in the
literate document first, especially for mechanical edits (renaming, constant
updates, formatting changes) across many files.

```bash
# Propagate all gen/ edits back to their literate sources
weaveback apply-back

# Dry run: show what would change without writing
weaveback apply-back --dry-run
```

**How it works (two levels):**

1. **Noweb level**: diffs each gen/ file against the stored baseline (from the last run).
   For each changed line, `noweb_map` identifies the literate source file and line.

2. **Macro level**: for each changed source line, re-evaluates the driver in tracing
   mode to pinpoint the exact token that produced the output:
   - `Literal` / `MacroBodyLiteral`: patched in place automatically
   - `MacroArg`: replaces the argument value at the call site; oracle-verified
   - `MacroBodyWithVars`: attempts structural patch; oracle-verified
   - `VarBinding` / `Computed`: reported but not auto-patched (ambiguous)

**Oracle verification:** for `MacroArg` and `MacroBodyWithVars`, the patched source is
re-evaluated and the relevant output line is checked before writing. A wrong candidate
is rejected — the source is never corrupted by a failed heuristic.

**Fuzzy line matching:** if the expected source line is not at the exact index (e.g. due
to reformatting), a ±15-line window search using a whitespace-normalised regex finds it.

## MCP server

`weaveback mcp` starts an MCP server (stdio transport) exposing three tools for
IDE/agent integration:

| Tool | Description |
|------|-------------|
| `weaveback_trace` | Trace a generated file line to its literate source. Returns `src_file`, `src_line`, `src_col`, `kind`, and (depending on kind) `macro_name`, `param_name`, `var_name`, `def_locations`, `set_locations`. |
| `weaveback_apply_back` | Propagate all gen/ edits back to the literate source. Returns a report of what was patched, skipped, or needs manual attention. |
| `weaveback_apply_fix` | Apply a single targeted source edit and verify it produces the expected output line (oracle-verified). |

**Recommended agent workflow (use this order):**

1. Call `weaveback_trace` with the generated file and line number to get the
   exact `src_file`/`src_line`. No grep required — works across multi-file
   chunks and macro expansions.
2. Read a few lines around `src_line` in the literate source to understand
   the chunk structure and surrounding prose.
3. Call `weaveback_apply_fix` with the replacement text and the exact output
   line you expect. The macro expander re-runs as an oracle; the file is
   written only if the output matches. No full build needed.
4. Re-read the prose section surrounding `src_line` in the literate source.
   Verify it still accurately describes the changed code.
   **This step is not optional.** The literate document is both code and
   documentation; a fix that leaves stale prose is incomplete.
5. Use a plain `Edit` if the surrounding prose is stale. No oracle needed
   for prose that doesn't affect generated output.
6. Run the project build. Only linking and type-checking remain;
   correctness was already oracle-verified in step 3.

**`weaveback_apply_back` is the escape hatch, not the normal path.**
Use it only when gen/ files have already been edited directly (by hand,
by a language tool, or by an IDE) and need to be synced back. In a normal
agent workflow where no gen/ files were edited directly, use trace →
apply_fix instead.

## Gotchas

**Line number drift:** `weaveback_apply_fix` uses source line numbers from
the current state of the literate file. If the file was edited directly before
calling the tool, line numbers reported by `trace` may have shifted. Always
re-read the target line before calling `apply_fix` to confirm it still contains
the expected content.

**Formatter interaction:** when `--formatter` is configured (e.g. `rs=rustfmt`),
the baseline in `weaveback.db` is the _formatted_ output. The oracle also
produces formatted output. Pass the formatted line as `expected_output`, not
the raw macro expansion.

## Guidelines for agents

- The literate document is the **source of truth**, but editing gen/ files
  directly is a **supported workflow** — not just a debugging shortcut.
  Make changes in the generated files using whatever tools work best, then
  run `weaveback apply-back` to sync them back. The next `weaveback` run will
  overwrite gen/ from the updated literate source, closing the loop.
- Use the Markdown/AsciiDoc structure to explain *why* the code is
  structured as it is. Chunk names should read as intent, not mechanics.
- When adding a new output file, declare it as a `<[@file ...]>=` chunk
  in the appropriate literate source, then reference named sub-chunks to
  keep each chunk short and focused.
- `weaveback` writes output files only when content changes (content-based
  diffing). Rebuilds that produce identical output leave files untouched,
  keeping build system timestamps stable.
- Use `--formatter rs=rustfmt` (or the equivalent for the target language)
  to keep generated code formatted without manual intervention.
- `--dump-expanded` (stderr) shows the macro-expanded intermediate text —
  the first thing to check when a chunk is missing or expands unexpectedly.
