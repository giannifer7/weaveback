# Azadi architecture

Azadi is a literate programming toolchain. Source files are written as
annotated documents (Markdown, AsciiDoc, etc.) that contain both prose and
named code chunks. The `azadi` command processes them and writes the final
source files.

## Transformation pipeline

```
Literate source (.md / .adoc / …)
        │
        ▼
  azadi-macros          expands %def / %set / %if / %rhaidef / %pydef calls
        │               into an intermediate noweb document
        ▼
  azadi-noweb           extracts <[chunk]> references, assembles them,
        │               optionally runs a formatter (e.g. rustfmt)
        ▼
  gen/ (output files)   written only when content changes
```

The two passes run in-process when you invoke the combined `azadi` binary.
The separate `azadi-macros` and `azadi-noweb` binaries exist for pipeline
use but are not needed for normal work.

## Source of truth

The literate document is the **only** source of truth. Files under `gen/`
are derived artefacts — editing them directly is always wrong because the
next `azadi` run will overwrite them (or refuse to, and tell you why; see
below).

## Directory layout

```
project/
├── src/                   literate source files
├── gen/                   generated output files  ← do not edit
└── azadi.db               source-map and modification-baseline database
```

`azadi.db` is a SQLite database (WAL mode) written by the tool after each
run. It stores the modification baseline for every generated file (for
external-edit detection), source maps for tracing, and snapshots of the
literate sources.

Because the database uses WAL mode, concurrent builds (`ninja -j4`) and a
running MCP server never contend: readers never block writers and writers
never block readers. Each azadi process accumulates its writes in an
in-memory database and flushes everything to `azadi.db` in a single
transaction at the end of the run.

The file is a standard SQLite database and can be inspected directly:

```bash
sqlite3 azadi.db .tables
sqlite3 azadi.db "SELECT out_file, out_line, src_file, src_line FROM noweb_map LIMIT 10"
```

Commit `gen/` to version control; add `azadi.db` to `.gitignore`.

## Content-based writes

`azadi-noweb` compares the freshly generated content against what is already
in `gen/` before writing. If they are identical the file is left untouched,
keeping build-system timestamps stable and avoiding unnecessary recompilation.

## What happens when you edit a generated file

Azadi protects generated files from accidental overwriting. After each
successful run it stores the bytes of every file it wrote as a baseline in
`azadi.db` (the `gen_baselines` table). On the next run, before writing, it
compares the current `gen/` file against that baseline:

- **File unchanged since last run** — azadi overwrites it with the new
  content as usual.
- **File modified externally** — azadi stops with a `ModifiedExternally`
  error and leaves the file untouched. The message names the file so you
  can decide what to do:
  - To accept the regenerated version: restore the file from version
    control (or delete it) and rerun azadi.
  - To keep your manual change: run `azadi apply-back` (see below) to
    propagate the edit back into the literate source, then rerun azadi.

In CI, start from a clean checkout (no `azadi.db`) so no baseline exists and
no conflict can arise.

## Propagating gen/ edits back to the source (`apply-back`)

`azadi apply-back` is the batch inverse of `azadi`: it reads the diff between
modified `gen/` files and their stored baselines, uses `noweb_map` to trace
each changed output line back to the literate source chunk and line that
produced it, and patches the literate source in place.

```bash
azadi apply-back                # process all modified gen/ files
azadi apply-back src/foo.c      # process one file
azadi apply-back --dry-run      # show what would change without writing
```

**What it can and cannot handle:**

- **Literal chunk content** (no macros in the chunk body) — patched
  automatically.
- **Size-preserving edits** (same number of lines changed) — handled
  line-by-line.
- **Added or deleted lines** — reported and skipped; edit the literate
  source manually for those.
- **Macro-generated content** (`%def`, `%rhaidef` bodies) — reported as a
  conflict and skipped. The source map points through the noweb level only;
  macro-level back-propagation is handled surgically via `azadi_apply_fix`
  in the MCP server (see below).

After applying, `apply-back` updates the baselines in `azadi.db` so the next
`azadi` run proceeds without a `ModifiedExternally` error.

`apply-back` is a **bulk reconciliation** tool: use it when `gen/` files have
already been edited by hand. For AI-assisted or interactive edits, prefer the
MCP `azadi_apply_fix` tool instead.

## Source tracing (`azadi trace`)

`azadi` records a full source map on every run. Use it to answer
*"where in the literate source did this output line come from?"*

```bash
azadi trace <out_file> <line>
azadi trace <out_file> <line> --col <col>   # 1-indexed character position
```

Both line and column numbers are **1-indexed character positions**
(multi-byte UTF-8 characters count as one position). The trace result
includes `kind`, `src_file`, `src_line`, and — when `--col` narrows to a
single token — the exact macro name, parameter name, or variable name that
produced that token.

See `docs/tracing.md` for the full output schema and examples.

## MCP server (`azadi mcp`)

`azadi mcp` exposes tracing and surgical source-editing over the
[Model Context Protocol](https://modelcontextprotocol.io/), so IDE
extensions and AI agents can work with literate sources without shelling out
or doing a full rebuild.

```bash
azadi --db azadi.db --gen src mcp
```

### Tools

| Tool | Description |
|------|-------------|
| `azadi_trace` | Trace a generated file line/column to its literate source. |
| `azadi_apply_fix` | **Preferred edit tool.** Replace a line or range in the literate source and oracle-verify it produces the expected output before writing. Supports single-line (`src_line`) and multi-line (`src_line` + `src_line_end` + `new_src_lines`) replacements. |
| `azadi_apply_back` | Bulk baseline-reconciliation. Use only when `gen/` files have already been edited by hand. |

### How `azadi_apply_fix` works

1. The agent calls `azadi_trace` to find `src_file:src_line`.
2. The agent reads the source context and constructs the replacement.
3. The agent calls `azadi_apply_fix` with:
   - `src_file`, `src_line` (and optionally `src_line_end` for a range)
   - `new_src_line` (single line) or `new_src_lines` (array)
   - `out_file`, `out_line`, `expected_output` — the oracle check
4. azadi re-expands the affected macro/chunk in memory. If the result at
   `out_line` matches `expected_output`, the literate source is patched and
   the baseline updated. Otherwise the call fails with a diff and no files
   are touched.

This oracle loop gives strong correctness guarantees without a full rebuild.

### Claude Code / Claude Desktop configuration

Add a `.mcp.json` in your project root:

```json
{
  "mcpServers": {
    "azadi": {
      "command": "azadi",
      "args": ["--db", "azadi.db", "--gen", "src", "mcp"]
    }
  }
}
```

Adjust `--gen` to match your project's generated-file directory.

## Build-system integration

`--depfile` writes a Makefile-format depfile after each run; `--stamp`
touches a file on success. Together they let a single build rule cover an
entire directory tree:

```meson
custom_target('gen',
  output  : ['gen.stamp'],
  depfile : 'gen.d',
  command : [azadi,
             '--dir',    meson.current_source_dir() / 'src',
             '--ext',    'adoc',
             '--include', meson.current_source_dir(),
             '--gen',    meson.current_source_dir() / 'gen',
             '--stamp',  '@OUTPUT0@',
             '--depfile', '@DEPFILE@'],
)
```

> List only the stamp in `output`, never the `.d` file — Ninja consumes
> depfiles into its internal database and will rerun forever if the `.d`
> file is also declared as an output.

## Formatter hooks

`--formatter EXT=COMMAND` runs a formatter on each generated file with the
matching extension before it is compared and written. Example:

```
azadi --formatter rs=rustfmt src/main.adoc --gen gen
```

The formatter receives a temporary copy (via `NamedTempFile`); the formatted
result is then used for content comparison and written to `gen/`.
