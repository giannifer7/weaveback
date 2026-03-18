# Source tracing

`azadi` records a source map on every run and stores it in `azadi.db`.
Use it to answer *"where did this line in a generated file come from?"*

Tracing is always on — no flag needed.

## Commands

```bash
# Trace a line to its literate source
azadi trace <out_file> <line>

# Pinpoint a specific token by column (1-indexed character position)
azadi trace <out_file> <line> --col <col>
```

Prints JSON to stdout. All line and column numbers are 1-indexed character
positions. `<out_file>` is the path of the generated file as seen on disk.
Reads `azadi.db` from the current working directory.

## Example

```bash
cd examples/c_enum
azadi status.md --gen .
azadi trace src/status.c 6
```

```json
{
  "chunk": "string_cases",
  "kind": "MacroBody",
  "macro_name": "enum_val",
  "src_file": "/path/to/examples/c_enum/status.md",
  "src_line": 31,
  "out_file": "src/status.c",
  "out_line": 6
}
```

When a line contains tokens from different sources, pass `--col` to target
the specific token you want to change:

```bash
azadi trace src/config.nim 178 --col 10
```

```json
{
  "kind": "MacroArg",
  "macro_name": "cfg_int",
  "param_name": "default_val",
  "src_file": "/path/to/config.nim.adoc",
  "src_line": 22
}
```

## Output fields

| Field | Meaning |
|-------|---------|
| `src_file` | Literate source file to edit |
| `src_line` | 1-indexed line in that file |
| `src_col` | 1-indexed character position within `src_line` |
| `kind` | `Literal`, `MacroBody`, `MacroArg`, `VarBinding`, or `Computed` |
| `macro_name` | Macro name (when `kind` is `MacroBody` or `MacroArg`) |
| `param_name` | Parameter name (when `kind` is `MacroArg`) |
| `var_name` | Variable name (when `kind` is `VarBinding`) |
| `def_locations` | `{file, line}` for every `%def`/`%rhaidef`/`%pydef` that defined this macro (when `kind` is `MacroBody`) |
| `set_locations` | `{file, line}` for every `%set` that set this variable (when `kind` is `VarBinding`) |
| `chunk` | Noweb chunk containing this line |

## Reading the result

- **`Literal`**: edit `src_file` at `src_line` directly.
- **`MacroBody`**: the text is a literal fragment of a macro body.
  `def_locations` says where the macro was defined.
- **`MacroArg`**: the text came from an argument at the call site.
  `src_file:src_line` is that call site; `param_name` names the parameter.
- **`VarBinding`**: the text came from a `%set` call. `set_locations` lists
  all assignment sites; `var_name` names the variable.

Span attribution follows arguments through nested macro calls —
`src_file:src_line` always points to the original literal text, not to
an intermediate call site.

---

## MCP server (`azadi mcp`)

`azadi mcp` starts a [Model Context Protocol](https://modelcontextprotocol.io/)
server over stdin/stdout, exposing tracing and apply-back tools so IDE
extensions and AI agents can work with the literate source without shelling out.

```bash
azadi --gen . mcp
```

The server implements the MCP 2024-11-05 protocol over JSON-RPC 2.0 (one
message per line on stdin/stdout).

### Tools

| Tool | Description |
|------|-------------|
| `azadi_trace` | Trace a generated file line to its literate source. Accepts `out_file`, `out_line`, and optional `out_col`. All 1-indexed character positions. |
| `azadi_apply_fix` | **Preferred tool for source edits.** Replace a line or range in the literate source and oracle-verify it produces the expected output before writing. Accepts `src_file`, `src_line`, optional `src_line_end` (range, defaults to `src_line`), `new_src_line` or `new_src_lines` array, `out_file`, `out_line`, `expected_output`. All line numbers are 1-indexed. |
| `azadi_apply_back` | Bulk baseline-reconciliation tool. Use only when gen/ files have already been edited by hand. Diffs each modified file against its stored baseline, traces changed lines back to the literate source, and patches in place. Accepts optional `files` array and `dry_run` flag. |

### Recommended agent workflow

1. **Locate the source**: call `azadi_trace` with the generated file and line number.
2. **Read the source**: open `src_file` at `src_line` (and surrounding context).
3. **Apply the fix**: call `azadi_apply_fix` with the replacement text and the
   exact output line you expect. The macro expander re-runs as an oracle —
   the file is written only if the expected output is produced.

`azadi_apply_fix` works for both single-line replacements (`src_line` only) and
multi-line ranges (`src_line` + `src_line_end` + `new_src_lines` array).
No full rebuild is needed to verify the edit.

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
