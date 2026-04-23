---
title: |-
  CLI reference
---
# CLI reference

The old combined `weaveback` binary is gone. The public CLI is now split into
four focused tools.

## Command map

<table>
  <tr><th>Tool</th><th>Responsibility</th></tr>
  <tr><td>`wb-tangle`</td><td>Build-side operations: single-pass tangling, multi-pass `weaveback.toml`<br>
runs, and `apply-back` reconciliation from `gen/` to literate source.</td></tr>
  <tr><td>`wb-query`</td><td>Read/query-side operations: `where`, `trace`, `attribute`, `impact`,<br>
`graph`, `search`, `lint`, `coverage`, wrapped `cargo`, `tags`, plus<br>
semantic `lsp` lookups and `tag` metadata generation.</td></tr>
  <tr><td>`wb-serve`</td><td>Local docs server with live reload, inline editing, and AI-assisted views.</td></tr>
  <tr><td>`wb-mcp`</td><td>MCP server for editor and agent integrations.</td></tr>
</table>

## Migration from the old combined CLI

| Old command | New command |
| --- | --- |
| `weaveback file.adoc --gen gen` | `wb-tangle file.adoc --gen gen` |
| `weaveback --dir src --gen gen` | `wb-tangle --dir src --gen gen` |
| `weaveback tangle` | `wb-tangle` |
| `weaveback apply-back` | `wb-tangle apply-back` |
| `weaveback where ...` | `wb-query where ...` |
| `weaveback trace ...` | `wb-query trace ...` |
| `weaveback attribute ...` | `wb-query attribute ...` |
| `weaveback cargo ...` | `wb-query cargo ...` |
| `weaveback coverage ...` | `wb-query coverage ...` |
| `weaveback graph ...` | `wb-query graph ...` |
| `weaveback impact ...` | `wb-query impact ...` |
| `weaveback search ...` | `wb-query search ...` |
| `weaveback lint ...` | `wb-query lint ...` |
| `weaveback tag ...` | `wb-query tag ...` |
| `weaveback tags ...` | `wb-query tags ...` |
| `weaveback lsp ...` | `wb-query lsp ...` |
| `weaveback serve ...` | `wb-serve ...` |
| `weaveback mcp` | `wb-mcp` |

## Common workflows

### Build and regenerate outputs

```bash
wb-tangle --dir src --ext adoc --include . --gen src
wb-tangle
```


### Reconcile edits made in `gen/`

```bash
wb-tangle apply-back
wb-tangle --gen path/to/gen apply-back --dry-run
```


### Trace generated code back to literate source

```bash
wb-query where gen/out.rs 120
wb-query trace gen/out.rs 120 8 --include .
wb-query attribute gen/out.rs:120:8 --include .
```


### Coverage and diagnostics

```bash
wb-query cargo clippy --all-targets -- -D warnings
wb-query coverage --summary lcov.info
```


### Docs server and MCP

```bash
wb-serve --watch
wb-mcp
```


## Detailed references

For full flag-by-flag details, read the tool-specific pages:

* [`wb-tangle`](../crates/wb-tangle/src/main.adoc)
* [`wb-query`](../crates/wb-query/src/main.adoc)
* [`wb-serve`](../crates/wb-serve/src/main.adoc)
* [`wb-mcp`](../crates/wb-mcp/src/main.adoc)
* [`cli-spec/macros.adoc`](../cli-spec/macros.adoc) for the shared option
  families used by the generated CLI code
