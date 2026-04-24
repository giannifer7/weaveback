---
title: |-
  Chunk syntax (weaveback-tangle)
---
# Chunk syntax (weaveback-tangle)

Comment markers (`#` or `//` by default) are stripped before delimiters are
recognised, so chunks blend into any language's comment syntax.

```rust
// <[@file src/hello.rs]>=
fn main() {
    // <[greeting]>
}
// @

// <[greeting]>=
println!("Hello, world!");
// @
```


## Declarations and references

| Syntax | Meaning |
| --- | --- |
| `# <[@file path]>=` | Declare a file output chunk |
| `# <[name]>=` | Declare a named chunk |
| `# <[name]>` | Reference (expand) a chunk inline; indentation is preserved |
| `# @` | End the current chunk |

The path in `@file` may begin with `~/` to write to the home directory.

## Modifiers

Modifiers go *before* the chunk name, inside the delimiters.

### `@replace`

Discards all prior definitions of the chunk and starts fresh:

```rust
// <[@replace @file src/main.rs]>=
… new content …
// @
```


### `@reversed`

On a *reference* line: expands the referenced chunk's accumulated definitions
in reverse order (last-defined first). Useful for stack / LIFO patterns:

```rust
// <[@reversed items]>
```


## Multiple definitions

Chunk definitions accumulate — a chunk may be defined in multiple places and
all definitions are concatenated in order when the chunk is referenced (unless
`@replace` or `@reversed` is used).

## Semantic navigation

When using language servers (LSP), weaveback can bridge the gap between chunk
boundaries and semantic definitions. For example, a symbol defined in one chunk
and used in another can be resolved semantically, taking you directly to the
original source chunk of the definition.

See [architecture.adoc](architecture.adoc#_semantic_language_server_integration_wb_query_lsp)
for the full LSP and MCP tool documentation.
