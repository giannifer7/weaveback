# Source tracing

`azadi` records a source map on every run and stores it in `azadi.db`.
Use it to answer *"where did this line in a generated file come from?"*

Tracing is always on — no flag needed.

Two levels of provenance are stored:

- **noweb level** — which literate chunk and source line produced each output line
- **macro level** — which macro call (body or argument) generated each expanded line

## Query commands

```bash
# noweb level: chunk name + source file/line
azadi where <out_file> <line>

# full two-level trace: chunk + macro origin (first byte of line)
azadi trace <out_file> <line>

# sub-line precision: trace the token covering a specific byte column
azadi trace <out_file> <line> --col <col>
```

Both print JSON to stdout. `<out_file>` is the path of the generated file as
seen on disk; `<line>` is 1-indexed, `--col` is 0-indexed byte offset within
that line. They read from `azadi.db` in the current working directory.

## Example

```bash
cd examples/c_enum
azadi status.md --gen .

azadi where src/status.c 6
```

```json
{
  "chunk": "string_cases",
  "expanded_file": "./status.md",
  "expanded_line": 44,
  "indent": "",
  "out_file": "src/status.c",
  "out_line": 6
}
```

```bash
azadi trace src/status.c 6
```

```json
{
  "chunk": "string_cases",
  "expanded_file": "./status.md",
  "expanded_line": 44,
  "indent": "",
  "kind": "MacroBody",
  "macro_name": "enum_val",
  "out_file": "src/status.c",
  "out_line": 6,
  "src_col": 45,
  "src_file": "/path/to/examples/c_enum/status.md",
  "src_line": 31
}
```

`trace` adds `src_file`, `src_line`, `src_col`, `kind`, and `macro_name`,
giving the exact location in the literate source.

### Sub-line precision with `--col`

Without `--col`, `trace` reports the token covering the first byte of the
output line.  When a line contains multiple macro-generated tokens (a common
pattern when macro arguments are themselves macro calls), pass the 0-indexed
byte column to pinpoint the specific token:

```bash
azadi trace src/rompla/config.nim 178 --col 10
```

```json
{
  "kind": "MacroBody",
  "macro_name": "cfg_int",
  "src_file": "/path/to/src/rompla/config.nim.adoc",
  "src_line": 94,
  "src_col": 0
}
```

Span attribution is threaded through argument evaluation: when a macro
argument is itself a macro call (e.g. `%wrap(%inner(world))`), the `world`
token traces back to its original literal position in the source, not to
the `%inner(world)` call site.

### `kind` values

| Value | Meaning |
|-------|---------|
| `Literal` | Text copied verbatim from the source |
| `MacroBody` | Text produced by expanding a macro body |
| `MacroArg` | Text produced from a macro argument value |
| `VarBinding` | Text from a `%set` variable |
| `Computed` | Text produced by a Rhai script or other computed source |

---

## MCP server (`azadi mcp`)

`azadi mcp` starts a [Model Context Protocol](https://modelcontextprotocol.io/)
server over stdin/stdout, exposing the `azadi_trace` tool so IDE extensions and
AI agents can look up source locations without shelling out.

```bash
azadi --gen . mcp
```

The server implements the MCP 2024-11-05 protocol over JSON-RPC 2.0 (one
message per line on stdin/stdout).

### Tool: `azadi_trace`

```json
{
  "name": "azadi_trace",
  "inputSchema": {
    "type": "object",
    "properties": {
      "out_file": { "type": "string" },
      "out_line": { "type": "integer" },
      "out_col":  { "type": "integer", "description": "Byte column within the output line (0-indexed, default 0). Use to pinpoint a specific token when a line contains multiple macro-generated fragments." }
    },
    "required": ["out_file", "out_line"]
  }
}
```

Returns the same JSON as `azadi trace`, encoded as a text content item.

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
