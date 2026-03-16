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

# full two-level trace: chunk + macro origin
azadi trace <out_file> <line>
```

Both print JSON to stdout. `<out_file>` is the path of the generated file as
seen on disk; `<line>` is 1-indexed. They read from `azadi.db` in the current
working directory.

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
      "out_line": { "type": "integer" }
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
