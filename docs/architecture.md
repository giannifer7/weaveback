---
title: |-
  Weaveback architecture
toc: left
---
# Weaveback architecture

Weaveback is a literate programming toolchain. Source files are written as
annotated documents (Markdown, AsciiDoc, etc.) that contain both prose and
named code chunks. The split CLI tools process them and write the final source
files.

## Transformation pipeline

```text
Literate source (.md / .adoc / …)
        │
        ▼
  weaveback-macro          expands %def / %set / %if / %pydef calls
        │               into an intermediate noweb document
        ▼
  weaveback-tangle           extracts <[chunk]> references, assembles them,
        │               optionally runs a formatter (e.g. rustfmt)
        ▼
  gen/ (output files)   written only when content changes
        │
        ▲
  weaveback-lsp            (Semantic Bridge)
        │               interfaces with rust-analyzer, nimlsp, pyright
        ▼
  Semantic Insights        mapped back to literate sources (Hover, Definition)
```


See [tangle module map](../crates/weaveback-tangle/src/weaveback_tangle.adoc) for the
weaveback-tangle internals.

Normal work now uses the split front ends:

* `wb-tangle` for builds, tangling, and apply-back
* `wb-query` for tracing, search, diagnostics, coverage, tagging, and LSP
* `wb-serve` for the docs server
* `wb-mcp` for MCP integration

## Source of truth

The literate document is the *only* source of truth. Files under `gen/`
are derived artefacts — editing them directly is always wrong because the
next `wb-tangle` run will overwrite them (or refuse to, and tell you why; see
below).

## Directory layout

```text
project/
├── src/                   literate source files
├── gen/                   generated output files  ← do not edit
├── crates/
│   ├── wb-tangle/         build/tangle/apply-back front end
│   ├── wb-query/          query/trace/coverage/tag/lsp front end
│   ├── wb-serve/          docs server front end
│   ├── wb-mcp/            MCP front end
│   ├── weaveback-macro/   macro expansion engine
│   ├── weaveback-tangle/  noweb chunk extractor and database
│   ├── weaveback-docgen/  documentation generator
│   ├── weaveback-core/    shared constants and path resolution
│   └── weaveback-lsp/     language server protocol client
└── weaveback.db           source-map and modification-baseline database
```


`weaveback.db` is a SQLite database (WAL mode) written by the tool after each
run. It stores the modification baseline for every generated file (for
external-edit detection), source maps for tracing, and snapshots of the
literate sources. The schema and API are documented in
[db.adoc](../crates/weaveback-tangle/src/db.adoc).

Because the database uses WAL mode, concurrent builds (`ninja -j4`) and a
running MCP server never contend: readers never block writers and writers
never block readers. Each weaveback process accumulates its writes in an
in-memory database and flushes everything to `weaveback.db` in a single
transaction at the end of the run.

The file is a standard SQLite database and can be inspected directly:

```bash
sqlite3 weaveback.db .tables
sqlite3 weaveback.db "SELECT out_file, out_line, src_file, src_line FROM noweb_map LIMIT 10"
```


Commit `gen/` to version control; add `weaveback.db` to `.gitignore`.

## Path normalization (`PathResolver`)

Weaveback uses a centralized `PathResolver` (implemented in
[`weaveback-core`](../crates/weaveback-core/src-wvb/weaveback_core.wvb)) to
ensure consistency between how humans, agents, and language servers see the
project directory.

The resolver handles:
* **Absolute paths**: converting `/home/user/project/gen/src/main.rs` to `src/main.rs`.
* **Workspace prefixes**: handling `crates/weaveback/src/main.rs` by stripping the
  member name if it matches the current context.
* **Auto-detection**: resolving `./` and other relative prefixes to match the
  database's canonical relative format.

This robust resolution is critical for the **Semantic Bridge**, as language
servers often return absolute URIs while Weaveback's database stores paths
relative to the generation directory (`--gen`).

## Content-based writes

[`SafeFileWriter`](../crates/weaveback-tangle/src/safe_writer.adoc) compares
the freshly generated content against what is already in `gen/` before writing.
If they are identical the file is left untouched, keeping build-system
timestamps stable and avoiding unnecessary recompilation.

## What happens when you edit a generated file

[`SafeFileWriter`](../crates/weaveback-tangle/src/safe_writer.adoc) protects
generated files from accidental overwriting. After each successful run it stores
the bytes of every file it wrote as a baseline in `weaveback.db` (the
[`gen_baselines`](../crates/weaveback-tangle/src/db.adoc#_gen_baselines)
table). On the next run, before writing, it compares the current `gen/` file
against that baseline:

* *File unchanged since last run* — `wb-tangle` overwrites it with the new
  content as usual.
* *File modified externally* — `wb-tangle` stops with a `ModifiedExternally`
  error and leaves the file untouched. The message names the file so you
  can decide what to do:
** To accept the regenerated version: restore the file from version
   control (or delete it) and rerun `wb-tangle`.
** To keep your manual change: run `wb-tangle apply-back` (see below) to
   propagate the edit back into the literate source, then rerun `wb-tangle`.

In CI, start from a clean checkout (no `weaveback.db`) so no baseline exists and
no conflict can arise.

## Propagating gen/ edits back to the source (`apply-back`)

`wb-tangle apply-back` is the batch inverse of `wb-tangle`: it reads the diff
between modified `gen/` files and their stored baselines, traces each changed
output line back through `noweb_map` and macro tracing, and then tries to
reconstruct the best literate-source edit that would reproduce the desired
generated result.

```bash
wb-tangle apply-back                # process all modified gen/ files
wb-tangle apply-back src/foo.c      # process one file
wb-tangle apply-back --dry-run      # show what would change without writing
```


*What it can and cannot handle:*

* *Literal chunk content* (no macros in the chunk body) — patched directly.
* *Simple noweb-mapped edits* — patched directly when attribution is
  unambiguous.
* *Macro-generated content* (`MacroBodyWithVars`, `MacroArg`) — handled through
  bounded candidate search plus oracle verification. weaveback searches nearby
  source lines and macro call sites, reruns the macro expander in memory, and
  keeps only candidates that reproduce the edited generated output.
* *Added or deleted lines* — reported and skipped; edit the literate source
  manually for those.
* *Conflicts or unverifiable candidates* — reported for manual resolution.
  `weaveback_apply_fix` in the MCP server remains the surgical edit path for
  explicit, oracle-checked fixes.

After applying, `apply-back` updates the baselines in `weaveback.db` so the next
`weaveback` run proceeds without a `ModifiedExternally` error.

`apply-back` is a *bulk reconciliation* tool: use it when `gen/` files have
already been edited by hand. For AI-assisted or interactive edits, prefer the
MCP `weaveback_apply_fix` tool instead.

## Source tracing (`wb-query trace`)

Weaveback records a full source map on every run. Use it to answer
_"where in the literate source did this output line come from?"_

```bash
wb-query trace <out_file> <line>
wb-query trace <out_file> <line> --col <col>   # 1-indexed character position
```


Both line and column numbers are *1-indexed character positions*
(multi-byte UTF-8 characters count as one position). The trace result
includes `kind`, `src_file`, `src_line`, and — when `--col` narrows to a
single token — the exact macro name, parameter name, or variable name that
produced that token.

See [docs/tracing.adoc](tracing.adoc) for the full output schema and examples.

## Semantic language-server integration (`wb-query lsp`)

`wb-query lsp` provides semantic navigation by bridging language-specific
LSPs with Weaveback's literate source maps. This allows
developers and coding agents to perform semantic queries on generated code
and have the results mapped directly back to the original source.

Supported languages (auto-detected):
* **Rust** (`rust-analyzer`)
* **Nim** (`nimlsp`)
* **Python** (`pyright-langserver`)

```bash
# Go to definition of symbol at line 100, col 8
wb-query lsp definition gen/src/main.rs 100 8

# Find all references to symbol at line 50, col 12
wb-query lsp references gen/src/main.rs 50 12

# Manual override for a custom server
wb-query lsp --lsp-cmd "pylsp" definition gen/main.py 10 5
```


### How it works

1. **LSP Query**: Weaveback spawns or reuses a background language server
   and sends a standard LSP request (e.g. `textDocument/definition`) for the
   requested position in the generated file.
2. **Semantic Resolution**: The LSP server resolves the query using its full
   semantic knowledge of the project.
3. **Transitive Mapping**: Weaveback takes the resulting generated-file
   position and calls `perform_trace` to map it back to the original literate
   source line and character column.

### Enriched documentation (`docs-ai`)

The `docs-ai` recipe in the `justfile` uses the LSP integration during
documentation generation to build a precise symbol graph. This captures
relationships like call sites and trait implementations that a simple structural
parser (like `syn`) cannot resolve, and injects them as semantic links in the
generated HTML.

## MCP server (`wb-mcp`)

`wb-mcp` exposes tracing and surgical source-editing over the
https://modelcontextprotocol.io/[Model Context Protocol], so IDE
extensions and AI agents can work with literate sources without shelling out
or doing a full rebuild.

```bash
wb-mcp --db weaveback.db --gen src
```


### Tools

<table>
  <tr><th>Tool</th><th>Description</th></tr>
  <tr><td>`weaveback_trace`</td><td>Trace a generated file line/column to its literate source.</td></tr>
  <tr><td>`weaveback_apply_fix`</td><td>*Preferred edit tool.* Replace a line or range in the literate source and<br>
oracle-verify it produces the expected output before writing. Supports<br>
single-line (`src_line`) and multi-line (`src_line` + `src_line_end` +<br>
`new_src_lines`) replacements.</td></tr>
  <tr><td>`weaveback_apply_back`</td><td>Bulk baseline-reconciliation. Use only when `gen/` files have already been<br>
edited by hand.</td></tr>
  <tr><td>`weaveback_lsp_hover`</td><td>Get type information and documentation for a symbol, mapped to literate source.</td></tr>
  <tr><td>`weaveback_lsp_diagnostics`</td><td>Get compiler errors and warnings, mapped to original literate source lines.</td></tr>
  <tr><td>`weaveback_lsp_symbols`</td><td>List semantic symbols (functions, structs) in a file with source locations.</td></tr>
</table>

### How `weaveback_apply_fix` works

1. The agent calls `weaveback_trace` to find `src_file:src_line`.
2. The agent reads the source context and constructs the replacement.
3. The agent calls `weaveback_apply_fix` with:
   * `src_file`, `src_line` (and optionally `src_line_end` for a range)
   * `new_src_line` (single line) or `new_src_lines` (array)
   * `out_file`, `out_line`, `expected_output` — the oracle check
4. weaveback re-expands the affected macro/chunk in memory. If the result at
   `out_line` matches `expected_output`, the literate source is patched and
   the baseline updated. Otherwise the call fails with a diff and no files
   are touched.

This oracle loop gives strong correctness guarantees without a full rebuild.

### Claude Code / Claude Desktop configuration

Add a `.mcp.json` in your project root:

```json
{
  "mcpServers": {
    "weaveback": {
      "command": "wb-mcp",
      "args": ["--db", "weaveback.db", "--gen", "src"]
    }
  }
}
```


Adjust `--gen` to match your project's generated-file directory.

## Live documentation server (`wb-serve`)

`wb-serve` starts a local HTTP server that serves the rendered HTML
documentation from `docs/html/`, pushes live-reload events to connected
browsers via Server-Sent Events, and — when you are editing the project's own
literate sources — lets you edit named code chunks directly in the browser.

### Intended workflow

```bash
# Terminal 1 — start the server once
wb-serve

# Terminal 2 — normal edit loop
$EDITOR crates/foo/src/bar.adoc
just tangle && just docs       # wb-serve detects the HTML changes and
                               # reloads the browser tab automatically
```


The server is intentionally *orthogonal* to the normal tangle and docgen
passes. In its default mode it serves existing HTML and watches for changes so
connected browsers can reload. In `--watch` mode it also triggers rebuilds when
`.adoc` or theme sources change. The important boundary is that the browser
editor still writes literate source; the normal build pipeline remains the
authority that regenerates code and docs.

`wb-serve` also adds a floating "✏ Edit source" button to every rendered
page. Clicking it opens the corresponding `.adoc` file at line 1 in
`$VISUAL` / `$EDITOR`.

### HTTP endpoints

<table>
  <tr><th>Path</th><th>Method</th><th>Description</th></tr>
  <tr><td>`/__events`</td><td>GET</td><td>Long-lived Server-Sent Events stream. The server sends an `event: reload`<br>
message whenever a file under `docs/html/` changes. The browser&#39;s<br>
`EventSource` reconnects automatically after a two-second delay on error.</td></tr>
  <tr><td>`/__open`</td><td>GET</td><td>Opens the `.adoc` source file for the current page in `$VISUAL` / `$EDITOR`.<br>
Query params: `file` (adoc path relative to project root), `line` (1-indexed).</td></tr>
  <tr><td>`/__chunk`</td><td>GET</td><td>Returns the current body of a named chunk and its line bounds, for<br>
pre-filling the inline editor. +<br>
Query params: `file`, `name`, `nth` (default 0). +<br>
Response: `{ &quot;ok&quot;: true, &quot;body&quot;: &quot;...&quot;, &quot;def_start&quot;: N, &quot;def_end&quot;: M }`.</td></tr>
  <tr><td>`/__apply`</td><td>POST</td><td>Applies a chunk body edit to the literate source file. +<br>
JSON body: `{ &quot;file&quot;, &quot;name&quot;, &quot;nth&quot;, &quot;old_body&quot;, &quot;new_body&quot; }`. +<br>
Verifies `old_body` matches the file on disk, runs an in-memory tangle<br>
oracle, writes the file on success, returns `{ &quot;ok&quot;: true }` or an error.</td></tr>
</table>

All `/__*` endpoints include `Access-Control-Allow-Origin: *`.

### AI assistant (`/__ai`)

`wb-serve` includes an AI assistant panel that provides context-aware
help. When a question is asked about a chunk, the server builds a
comprehensive context object:
* **Source body**: the raw literate source of the chunk.
* **Design notes**: all prose from the section containing the chunk.
* **Dependencies**: the bodies of all chunks transitively referenced.

This context is forwarded to the configured AI backend. Supported backends
include:
* **Claude** (via local CLI or Anthropic API)
* **Google Gemini** (API)
* **Ollama** (local models like `llama3`)
* **OpenAI** (and OpenAI-compatible APIs)

### Chunk ID annotation

`weaveback-docgen` injects `data-chunk-id="file|name|nth"` attributes on
every `<div class="listingblock">` element whose `<code>` block opens with a
weaveback chunk-open marker (`<[name]>=` or `<<name>>=`). The annotation
pass runs as part of `just docs` and is idempotent.

The browser JS reads these attributes on load, attaches a "✎ Edit" button to
each annotated block (visible on hover), and shows the inline editor panel
when clicked.

### Inline editor

The editor panel appears at the bottom-right of the browser window:

* Fetches the current chunk body via `GET /__chunk`.
* Displays it in the embedded CodeMirror editor, pre-filled with the body
  (header and close marker lines are excluded).
* `Save` (or `Ctrl+S` / `Cmd+S`) posts to `/__apply` with `old_body` for
  optimistic-concurrency checking.
* On `body_mismatch` (file changed since the panel was opened) or
  `tangle_failed` (the oracle rejected the edit), the error is shown in the
  status line without touching the file.
* On success the panel shows "Saved — waiting for rebuild…"; `just tangle &&
  just docs` regenerates the HTML and the SSE live-reload fires automatically.

### The `/__apply` oracle

Before writing the modified `.adoc` file, the server runs an in-memory tangle
check on the *entire directory* of `.adoc` files (not just the one being
edited). This catches recursion errors, undefined chunk references, and
structural problems in the same tangle unit. It does *not* run formatters or
write `gen/` files — it only verifies that chunk expansion succeeds without
error.

The oracle uses the chunk-syntax configuration passed to `wb-serve`:

```bash
# Defaults match the weaveback project's own conventions
wb-serve

# Override for projects using different delimiters
wb-serve --open-delim "<<" --close-delim ">>" --chunk-end "@" \
                --comment-markers "//"
```


### What the inline editor can and cannot do

*Can edit:* named chunk bodies — the lines between the chunk-open header and
the chunk-close marker. These are the authoritative definitions that tangle
expands.

*Cannot edit through the browser:* assembly chunks (`@file` roots), chunk
expansion projections (blocks that reference a chunk without defining it),
or arbitrary prose code blocks. These are shown in the HTML but not annotated
with `data-chunk-id` because their content is either derived or structural.

After saving, you still need to run `just tangle && just docs` to regenerate
the code and HTML. The browser reloads automatically once `just docs`
finishes.

## Weaveback's own implementation

Both crates are written as literate AsciiDoc sources.
The generated `.rs` files live next to their `.adoc` counterparts; `just tangle`
regenerates them, and `just docs` renders all `.adoc` files to `docs/html/`.

.weaveback-tangle literate sources
| Document | Generates |
| --- | --- |
| [weaveback-tangle crate index](../crates/weaveback-tangle/src/weaveback_tangle.adoc) | `crates/weaveback-tangle/src/lib.rs` |
| [chunk parser and expander](../crates/weaveback-tangle/src/noweb.adoc) | `crates/weaveback-tangle/src/noweb.rs` |
| [safe file writer](../crates/weaveback-tangle/src/safe_writer.adoc) | `crates/weaveback-tangle/src/safe_writer.rs` |
| [persistent database](../crates/weaveback-tangle/src/db.adoc) | `crates/weaveback-tangle/src/db.rs` |
| [CLI binary](../crates/weaveback-tangle/src/cli.adoc) | `crates/weaveback-tangle/src/main.rs` |
| [tests](../crates/weaveback-tangle/src/tests/tests.adoc) | `crates/weaveback-tangle/src/tests/` (4 files) |

.weaveback-lsp literate sources
| Document | Generates |
| --- | --- |
| [weaveback-lsp crate index](../crates/weaveback-lsp/src/weaveback_lsp.adoc) | `crates/weaveback-lsp/src/lib.rs` |

.weaveback-core literate sources
| Document | Generates |
| --- | --- |
| [weaveback-core crate index](../crates/weaveback-core/src-wvb/weaveback_core.wvb) | `crates/weaveback-core/src/lib.rs` |

.weaveback-macro literate sources
<table>
  <tr><th>Document</th><th>Generates</th></tr>
  <tr><td>[weaveback-macro crate index](../crates/weaveback-macro/src/weaveback_macro.adoc)</td><td>`crates/weaveback-macro/src/lib.rs`</td></tr>
  <tr><td>[shared types](../crates/weaveback-macro/src/types.adoc)</td><td>`crates/weaveback-macro/src/types.rs`</td></tr>
  <tr><td>[line index](../crates/weaveback-macro/src-wvb/line_index.wvb)</td><td>`crates/weaveback-macro/src/line_index.rs`</td></tr>
  <tr><td>[macro_api](../crates/weaveback-macro/src/macro_api.adoc)</td><td>`crates/weaveback-macro/src/macro_api.rs`</td></tr>
  <tr><td>[CLI binary](../crates/weaveback-macro/src/bin/cli.adoc)</td><td>`crates/weaveback-macro/src/bin/weaveback-macro.rs`</td></tr>
  <tr><td>[weaveback-macro parser](../crates/weaveback-macro/src/parser/parser.adoc)</td><td>`crates/weaveback-macro/src/parser/mod.rs`</td></tr>
  <tr><td>[weaveback-macro lexer](../crates/weaveback-macro/src/lexer/lexer.adoc)</td><td>`crates/weaveback-macro/src/lexer/mod.rs` +<br>
`crates/weaveback-macro/src/lexer/tests.rs`</td></tr>
  <tr><td>[weaveback-macro AST](../crates/weaveback-macro/src/ast/ast.adoc)</td><td>`crates/weaveback-macro/src/ast/mod.rs` +<br>
`crates/weaveback-macro/src/ast/serialization.rs` +<br>
`crates/weaveback-macro/src/ast/tests.rs`</td></tr>
  <tr><td>[weaveback-macro evaluator (index](../crates/weaveback-macro/src/evaluator/evaluator.adoc))</td><td>`crates/weaveback-macro/src/evaluator/mod.rs` +<br>
`crates/weaveback-macro/src/evaluator/errors.rs` +<br>
`crates/weaveback-macro/src/evaluator/lexer_parser.rs`</td></tr>
  <tr><td>[evaluator state](../crates/weaveback-macro/src/evaluator/state.adoc)</td><td>`crates/weaveback-macro/src/evaluator/state.rs`</td></tr>
  <tr><td>[evaluator output sinks](../crates/weaveback-macro/src/evaluator/output.adoc)</td><td>`crates/weaveback-macro/src/evaluator/output.rs`</td></tr>
  <tr><td>[evaluator core](../crates/weaveback-macro/src/evaluator/core.adoc)</td><td>`crates/weaveback-macro/src/evaluator/core.rs`</td></tr>
  <tr><td>[evaluator builtins](../crates/weaveback-macro/src/evaluator/builtins.adoc)</td><td>`crates/weaveback-macro/src/evaluator/builtins.rs` +<br>
`crates/weaveback-macro/src/evaluator/case_conversion.rs` +<br>
`crates/weaveback-macro/src/evaluator/source_utils.rs`</td></tr>
  <tr><td>[evaluator script back-ends](../crates/weaveback-macro/src/evaluator/scripting.adoc)</td><td>`crates/weaveback-macro/src/evaluator/monty_eval.rs`</td></tr>
  <tr><td>[evaluator public API](../crates/weaveback-macro/src/evaluator/eval_api.adoc)</td><td>`crates/weaveback-macro/src/evaluator/eval_api.rs`</td></tr>
  <tr><td>[evaluator tests](../crates/weaveback-macro/src/evaluator/tests.adoc)</td><td>`crates/weaveback-macro/src/evaluator/test_utils.rs` +<br>
`crates/weaveback-macro/src/evaluator/tests/` (21 files)</td></tr>
</table>

## For Coding Agents

Weaveback's architecture is specifically designed to support autonomous coding
agents. Traditional repositories are difficult for agents because the "source
of truth" (the code) is often detached from the "intent" (the docs).

By using Weaveback, agents gain several advantages:

1. **Contextual Precision**: The `weaveback_chunk_context` tool provides an
   agent with both the code and the developer's design notes in a single
   structured request.
2. **Verified Editing**: Agents do not have to "hope" their generation is
   correct. The `weaveback_apply_fix` oracle verifies that the change produces
   the expected output before any file is saved.
3. **Semantic Navigation**: Through the **LSP Bridge**, agents can navigate
   the codebase semantically ("Who calls this function?") while staying
   within the literate source documents.
4. **Automatic Synchronization**: The `apply-back` tool allows agents to
   use existing language-specific refactoring tools (like `rust-analyzer`
   renames) and automatically propagate those results back to the literate
   sources.

## Tree-sitter grammar

`tree-sitter-weaveback/` contains a
https://tree-sitter.github.io/tree-sitter/[tree-sitter] grammar for the
Weaveback macro language. It is a standalone Node.js project (not part of
the Rust workspace) that provides editor syntax highlighting, language
injection into `%pydef` bodies, and `[source,weaveback]` block
highlighting inside AsciiDoc literate documents.

The grammar and all editor-integration files are literate AsciiDoc sources
tangled by `just tangle`.

.tree-sitter-weaveback literate sources
<table>
  <tr><th>Document</th><th>Generates</th></tr>
  <tr><td>[tree-sitter-weaveback index](../tree-sitter-weaveback/tree_sitter_weaveback.adoc)</td><td>(overview and module map only)</td></tr>
  <tr><td>[grammar.adoc](../tree-sitter-weaveback/grammar.adoc)</td><td>`tree-sitter-weaveback/grammar.js`</td></tr>
  <tr><td>[queries.adoc](../tree-sitter-weaveback/queries.adoc)</td><td>`tree-sitter-weaveback/queries/highlights.scm` +<br>
`tree-sitter-weaveback/queries/injections.scm` +<br>
`tree-sitter-weaveback/editors/helix/asciidoc-injections.scm`</td></tr>
  <tr><td>[editors.adoc](../tree-sitter-weaveback/editors.adoc)</td><td>`tree-sitter-weaveback/editors/helix/languages.toml` +<br>
`tree-sitter-weaveback/editors/helix/install.py` +<br>
`tree-sitter-weaveback/editors/neovim/weaveback.lua` +<br>
`tree-sitter-weaveback/editors/neovim/asciidoc.lua` +<br>
`tree-sitter-weaveback/editors/neovim/install.py`</td></tr>
  <tr><td>[manifest.adoc](../tree-sitter-weaveback/manifest.adoc)</td><td>`tree-sitter-weaveback/package.json`, `tree-sitter.json`</td></tr>
</table>

## Build-system integration

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
             '--gen',    meson.current_source_dir() / 'gen',
             '--stamp',  '@OUTPUT0@',
             '--depfile', '@DEPFILE@'],
)
```


> [!NOTE]
> List only the stamp in `output`, never the `.d` file — Ninja consumes
depfiles into its internal database and will rerun forever if the `.d`
file is also declared as an output.


## Formatter hooks

`--formatter EXT=COMMAND` runs a formatter on each generated file with the
matching extension before it is compared and written
(implemented in [`SafeFileWriter::run_formatter`](../crates/weaveback-tangle/src/safe_writer.adoc)).
Example:

```bash
weaveback --formatter rs=rustfmt src/main.adoc --gen gen
```


The formatter receives a temporary copy (via `NamedTempFile`); the formatted
result is then used for content comparison and written to `gen/`.
