# weaveback-tangle: Conceptual Map

This document provides a high-level "zoom out" of how `weaveback-tangle` processes literate source files into generated code. While the individual `.rs` files handle the mechanics, this map explains the **intent** and the **whys**.

## The Tangle Pipeline


<!-- graph: tangle-pipeline -->
```d2

direction: right

input: {
  label: "Literate Source (.adoc)"
  shape: page
}

parser: {
  label: "Block Parser"
  tooltip: "Extracts noweb chunks and metadata"
}

db: {
  label: "Noweb Database"
  shape: cylinder
  tooltip: "Stores chunk relationships and source maps"
}

expander: {
  label: "Recursive Expander"
  tooltip: "Resolves <[chunk]> references"
}

writer: {
  label: "Safe Atomic Writer"
  tooltip: "Writes only on change to preserve timestamps"
}

output: {
  label: "Generated Source (.rs)"
  shape: page
}

input -> parser: "Scan for chunks"
parser -> db: "Populate registry"
db -> expander: "Fetch definition"
expander -> writer: "Flattened text"
writer -> output: "Atomic sync"

expander -> expander: "Detect cycles" {
  style: {
    stroke-dash: 3
  }
}

```


## Why this architecture?

### 1. Why a intermediate database?
Instead of expanding chunks in-memory during parsing, we populate a **Noweb Database** first.
*   **Decoupling**: Parsing and Expanding are separate concerns.
*   **Source Tracing**: The DB stores the exact file and line number for every fragment of text. This allows `wb-query trace` to work even through complex nested expansions.
*   **Incrementalism**: In the future, the DB will allow us to only re-tangle files whose chunks have actually changed.

### 2. Why Atomic Writing?
The `SafeWriter` (see xref:safe_writer.adoc[safe_writer.adoc]) never overwrites a file if the content is identical. 
*   **Build Stability**: Compilers like `rustc` won't recompile a module just because `weaveback` ran; they only recompile if the timestamp changes.
*   **Safety**: We write to a `.tmp` file first and use an atomic rename to prevent half-written files on crash.

### 3. Why Two Passes?
`weaveback` (the top-level tool) runs `weaveback-macro` before `weaveback-tangle`. 
*   **Macro Power**: This allows you to use logic (for example Python via monty) to generate code chunks dynamically.
*   **Separation of Concerns**: Tangle only cares about `<<noweb>>` syntax; it doesn't need to know about `%macros`.

## Where to go next?
*   To see how we parse blocks: link:block_parser.adoc[block_parser.adoc]
*   To see the expansion logic: link:noweb.adoc[noweb.adoc]
*   To see the database schema: link:db.adoc[db.adoc]
