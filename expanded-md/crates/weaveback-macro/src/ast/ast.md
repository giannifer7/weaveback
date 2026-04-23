---
title: |-
  weaveback-macro AST
description: |-
  Literate source for crates/weaveback-macro/src/ast/
toc: left
toclevels: 3
---
# weaveback-macro AST

The AST phase sits between the parser and the evaluator — it is the
_normalization_ (or _lowering_) step.  The parser produces a `ParseNode`
arena (a flat `Vec` indexed by integer, built by the push-down state
machine) that is structurally correct but not yet clean: it still contains
comment nodes and unresolved whitespace artefacts.  The evaluator needs an
owned, recursive `ASTNode` tree where those details have been stripped and
named parameters have been resolved.  `build_ast` bridges the two
representations in a single tree walk.

The module also contains the serialization layer used to dump the AST to
a line-oriented text format for external consumers (Python evaluator,
diagnostic tools).

## Design rationale

### Why arena → owned tree?

The parser's arena model is ideal for the push-down parser: nodes are
appended cheaply, back-references are just indices.  But the evaluator
benefits from owned children — no lifetime ties to the source string, no
index arithmetic, direct recursive descent.  `clean_node` converts each
`ParseNode` (by index) into an `ASTNode` (owned), recursing through
children and dropping comments on the way.

### Why two-pass `analyze_param`?

A macro argument can be either positional (`value`) or named
(`name = value`).  Detecting a named parameter requires recognising the
pattern `[spaces/comments] Ident [spaces/comments] Equal [...]` at the
start of the children list — and then deciding where the _value_ begins.
A forward scan through a three-state DFA accumulates the result; a second
loop then converts children starting at the computed `start_idx`.  This
avoids backtracking while making all legal transitions explicit.

### `ParamState` — making the DFA explicit

The first pass of `analyze_param` is a three-state machine:

[cols="1,2,2",options="header"]
|===
| State | Transitions in | Carries

| `Start`
| initial state
| —

| `SeenName`
| `Start` + `Ident` token
| the `name: Token` and its `name_idx: usize`

| `SeenEqual`
| `SeenName` + `Equal` token
| the `name: Token`
|===

The `SeenName` state has two exits: `Equal` advances to `SeenEqual`; any
other non-skip token `break`s the scan and the parameter is classified as
_positional_.  This means `foo bar` is positional (with `foo` as its
first non-skippable token), never named, even though `foo` looks like a
name — only `foo = ...` is named.

Any non-skip, non-transition token causes an immediate `break`.  The final
state after the scan — combined with whether `first_good_after_equal` was
set — fully determines the output case without any boolean flags.

Positions (`first_not_skippable`, `first_good_after_equal`) remain
`Option<usize>` rather than `i32` sentinels: no casting, no sentinel bugs,
idiomatic Rust.

### `process_ast` as the single safe entry point

`strip_space_before_comments` mutates the parser's arena before `build_ast`
converts it to a tree.  Calling `build_ast` without the preceding strip
produces correct but polluted output.  `Parser::process_ast` (in
`parser/mod.rs`) is the one place in the codebase that sequences both calls
in the right order.  `Parser::build_ast` is also exposed for tests that
want to inspect the un-stripped parse tree.  Any external caller that
wants clean output must go through `process_ast`.

### Why strip whitespace before comments?

In idiomatic weaveback source, comments appear on their own line:

----
some text %%// strip me
----

The space before `%%//` is syntactically part of the preceding `Text` or
`Space` node.  If left in, it pollutes the generated output.
`strip_space_before_comments` removes preceding `Space` nodes entirely
and trims trailing spaces off preceding `Text` nodes.  Block comments are
treated identically when they are followed by a newline (i.e. they fill
the rest of the line).

The backward walk scans over _all_ consecutive `Space` nodes before a
comment, not just the one immediately adjacent.  A sequence such as
`Text("hello") / Space / Space / %%// comment` would otherwise leave a
dangling `Space` node in the output.

`is_skippable` centralises the three-way `Space | LineComment | BlockComment`
predicate so both `strip_space_before_comments` and `analyze_param` use
the same definition.

### Why BFS serialization?

The AST is serialized to a line-per-node text format for external
evaluators.  BFS guarantees that a node at output index `i` always has its
children at a contiguous range `next_idx..next_idx+n`, where `next_idx`
is computed by accumulating child counts during the traversal.  DFS would
place children non-contiguously, forcing consumers to store and scan back-
references.

## File structure

Three files are generated from this document.


```rust
// <[@file weaveback-macro/src/ast/mod.rs]>=
// weaveback-macro/src/ast/mod.rs
// I'd Really Rather You Didn't edit this generated file.

// crates/weaveback-macro/src/ast/mod.rs — generated from ast.adoc
// <[ast preamble]>

// <[ast error]>

// <[ast build]>

// <[ast analyze param]>

// <[ast clean node]>

// <[ast strip spaces]>

// @
```



```rust
// <[@file weaveback-macro/src/ast/serialization.rs]>=
// weaveback-macro/src/ast/serialization.rs
// I'd Really Rather You Didn't edit this generated file.

// crates/weaveback-macro/src/ast/serialization.rs — generated from ast.adoc
// <[ast serialization preamble]>

// <[ast serialize token]>

// <[ast serialize nodes]>

// <[ast write ast]>

// <[ast read input]>

// <[ast dump]>

// <[ast serialization tests]>

// @
```



```rust
// <[@file weaveback-macro/src/ast/tests.rs]>=
// weaveback-macro/src/ast/tests.rs
// I'd Really Rather You Didn't edit this generated file.

// <[ast tests]>

// @
```


## Module preamble


```rust
// <[ast preamble]>=
use crate::parser::Parser;
use crate::types::{ASTNode, NodeKind, Token};
use thiserror::Error;
pub mod serialization;

pub use serialization::{dump_macro_ast, serialize_ast_nodes};

/// Three-state DFA used by `analyze_param` to classify a parameter node.
#[derive(Debug)]
enum ParamState {
    /// Initial state — no significant token seen yet.
    Start,
    /// Saw `Ident`; waiting for `=` or a non-skip token that ends the scan.
    SeenName { name: Token, name_idx: usize },
    /// Saw `Ident =`; waiting for the first non-skip value token.
    SeenEqual { name: Token },
}

#[cfg(test)]
mod tests;

/// Returns `true` for node kinds that are transparent in both parameter
/// scanning (`analyze_param`) and whitespace stripping
/// (`strip_space_before_comments`): `Space`, `LineComment`, `BlockComment`.
#[inline]
fn is_skippable(kind: NodeKind) -> bool {
    matches!(kind, NodeKind::Space | NodeKind::LineComment | NodeKind::BlockComment)
}

#[inline]
fn normalized_end_pos(token: Token, end_pos: usize) -> usize {
    end_pos.max(token.end())
}
// @
```


## Error type

`ASTError` covers the three failure modes: the parser produced nothing
(`Parser`), an index lookup missed (`NodeNotFound`), and a generic
catch-all for other processing problems.  `From<String>` lets `?` on
`String`-returning helpers propagate into `ASTError::Other` without noise.


```rust
// <[ast error]>=
#[derive(Error, Debug)]
pub enum ASTError {
    #[error("Parser error: {0}")]
    Parser(String),
    #[error("Node not found: {0}")]
    NodeNotFound(usize),
    #[error("Processing error: {0}")]
    Other(String),
}

impl From<String> for ASTError {
    fn from(error: String) -> Self {
        ASTError::Other(error)
    }
}
// @
```


## Public entry point: `build_ast`

The single public function that callers use.  It asks the parser for its
root index (which may be absent if the source was empty), then delegates
to `clean_node`.  A `None` result from `clean_node` — which can only
happen if the root itself is a comment node, an unlikely but legal state —
is reported as an error rather than silently returning an empty tree.


```rust
// <[ast build]>=
/// Main entry point that unwraps the Option
pub fn build_ast(parser: &Parser) -> Result<ASTNode, ASTError> {
    let root_idx = parser
        .get_root_index()
        .ok_or_else(|| ASTError::Parser("Empty parse tree".into()))?;

    clean_node(parser, root_idx)?
        .ok_or_else(|| ASTError::Parser("Root node was skipped".into()))
}
// @
```


## Parameter analysis: `analyze_param`

The most complex function in the module.  It handles the three structural
cases of a macro parameter:

[cols="1,3",options="header"]
|===
| Pattern | Result

| `[spaces] Ident [spaces] = value...`
| Named param — `name` token set, parts start after `=`

| `[spaces] Ident [spaces] =` (nothing after `=`)
| Named param with blank value — `name` set, `parts` empty

| anything else
| Positional param — `name` is `None`, parts start from first non-skippable node
|===

The first pass (lines labelled "First pass") scans children, skipping
`Space`, `LineComment`, and `BlockComment` nodes.  It sets four state
variables:

* `param_name` — set to the first `Ident` token seen before any `=`
* `name_index` — its position in the children array
* `seen_equal` — set when `=` is found after the first `Ident`
* `first_good_after_equal` — position of the first non-skippable node after `=`
* `first_not_skippable` — position of the first non-skippable node overall

The `break` at line `if seen_equal { ... } break` is intentional: once the
pattern `Ident = first-value-item` is fully determined, further scanning
would only add complexity.  Everything from `start_idx` onwards is
processed by `clean_node` in the second pass.


```rust
// <[ast analyze param]>=
/// Analyse a parameter node: classify as positional or named and collect parts.
fn analyze_param(parser: &Parser, node_idx: usize) -> Result<Option<ASTNode>, ASTError> {
    let node = parser
        .get_node(node_idx)
        .ok_or(ASTError::NodeNotFound(node_idx))?;

    let mut state = ParamState::Start;
    let mut first_not_skippable: Option<usize> = None;
    let mut first_good_after_equal: Option<usize> = None;

    // First pass: walk children through a three-state DFA.
    'scan: for (i, &part_idx) in node.parts.iter().enumerate() {
        let part = parser
            .get_node(part_idx)
            .ok_or(ASTError::NodeNotFound(part_idx))?;

        if is_skippable(part.kind) {
            continue;
        }

        first_not_skippable.get_or_insert(i);

        match &state {
            ParamState::Start => {
                if part.kind == NodeKind::Ident {
                    state = ParamState::SeenName { name: part.token, name_idx: i };
                    // keep scanning — an `=` may follow
                } else {
                    break 'scan; // positional: first non-skip is not an Ident
                }
            }
            ParamState::SeenName { name, .. } => {
                if part.kind == NodeKind::Equal {
                    let name = *name; // Token is Copy
                    state = ParamState::SeenEqual { name };
                    // keep scanning — a value item may follow
                } else {
                    break 'scan; // positional: Ident not followed by =
                }
            }
            ParamState::SeenEqual { .. } => {
                first_good_after_equal = Some(i);
                break 'scan; // named param; value starts here
            }
        }
    }

    // Determine start index and param name from the final DFA state.
    let (start_idx, param_name) = match state {
        ParamState::Start => match first_not_skippable {
            None => {
                // Completely empty param. Whether this should be kept depends on
                // macro context: interior empties are meaningful
                // (`%%if(cond, , false_branch)`), while trailing empties created
                // by optional trailing commas should be dropped later by the
                // enclosing Macro node.
                return Ok(Some(ASTNode {
                    kind: NodeKind::Param,
                    src: node.src,
                    token: node.token,
                    end_pos: normalized_end_pos(node.token, node.end_pos),
                    parts: vec![],
                    name: None,
                }));
            }
            Some(i) => (i, None), // positional: starts from first non-skip
        },
        ParamState::SeenName { name_idx, .. } => (name_idx, None),
        ParamState::SeenEqual { name } => match first_good_after_equal {
            None => {
                // Named param with blank value: `foo =`.
                return Ok(Some(ASTNode {
                    kind: NodeKind::Param,
                    src: node.src,
                    token: node.token,
                    end_pos: normalized_end_pos(node.token, node.end_pos),
                    parts: vec![],
                    name: Some(name),
                }));
            }
            Some(i) => (i, Some(name)),
        },
    };

    // Second pass: collect and clean the value parts.
    let mut value_parts = Vec::new();
    for &part_idx in &node.parts[start_idx..] {
        if let Some(part_node) = clean_node(parser, part_idx)? {
            value_parts.push(part_node);
        }
    }

    Ok(Some(ASTNode {
        kind: NodeKind::Param,
        src: node.src,
        token: node.token,
        end_pos: normalized_end_pos(node.token, node.end_pos),
        parts: value_parts,
        name: param_name,
    }))
}
// @
```


## Tree conversion: `clean_node`

The recursive engine of `build_ast`.  It returns `None` for comment nodes
(they are stripped from the AST entirely) and delegates `Param` nodes to
`analyze_param`.  All other nodes have their children recursively cleaned
and are reconstructed as `ASTNode` values.  The `name` field is always
`None` here — it is only populated by `analyze_param`.

A `debug_assert!` enforces that leaf node kinds (`Equal`, `Ident`, `Text`,
`Space`) arrive with no children.  The assertion fires only in debug builds
and catches parser regressions immediately rather than producing a
silently-wrong AST.


```rust
// <[ast clean node]>=
/// Recursively convert a `ParseNode` arena entry to an owned `ASTNode` tree.
///
/// Returns `None` for comment nodes (stripped from the AST entirely) and
/// delegates `Param` nodes to `analyze_param`.
fn clean_node(parser: &Parser, node_idx: usize) -> Result<Option<ASTNode>, ASTError> {
    let node = parser
        .get_node(node_idx)
        .ok_or(ASTError::NodeNotFound(node_idx))?;

    // Strip comments entirely.
    if matches!(node.kind, NodeKind::LineComment | NodeKind::BlockComment) {
        return Ok(None);
    }

    // Parameter nodes require name/value analysis.
    if node.kind == NodeKind::Param {
        return analyze_param(parser, node_idx);
    }

    // Structural invariant: leaf node kinds should never have children.
    // A violation here indicates a parser bug, not user input.
    debug_assert!(
        !matches!(
            node.kind,
            NodeKind::Equal | NodeKind::Ident | NodeKind::Text | NodeKind::Space
        ) || node.parts.is_empty(),
        "leaf {:?} node at index {} should have no children, found {}",
        node.kind,
        node_idx,
        node.parts.len()
    );

    // Recurse into children.
    let mut child_nodes = Vec::new();
    for &child_idx in &node.parts {
        if let Some(child) = clean_node(parser, child_idx)? {
            child_nodes.push(child);
        }
    }

    Ok(Some(ASTNode {
        kind: node.kind,
        src: node.src,
        token: node.token,
        end_pos: normalized_end_pos(node.token, node.end_pos),
        parts: if node.kind == NodeKind::Macro {
            trim_trailing_empty_params(child_nodes)
        } else {
            child_nodes
        },
        name: None,
    }))
}

fn is_empty_positional_param(node: &ASTNode) -> bool {
    node.kind == NodeKind::Param && node.name.is_none() && node.parts.is_empty()
}

fn trim_trailing_empty_params(mut parts: Vec<ASTNode>) -> Vec<ASTNode> {
    while parts.last().is_some_and(is_empty_positional_param) {
        parts.pop();
    }
    parts
}
// @
```


## Whitespace stripping

### `strip_space_before_comments`

Walks the children of `node_idx` and removes whitespace that immediately
precedes comments.  Two sub-cases:

* **`Space` node before comment** — the `Space` node is removed from
  `parts` by recording its index in `to_remove` and splicing it out after
  the analysis loop (to avoid mutating while iterating).
* **`Text` node before comment** — trailing spaces are trimmed in-place
  via `parser.strip_ending_space`, which shortens the token's `length`
  field without reallocating.

Block comments are treated as line-ending only when the byte immediately
after their `end_pos` is `\n`; inline block comments (`%%/* ... %%*/ more`)
are left alone.

The function then recurses into all surviving children.  Note that
children are re-read after the removal loop — the `children` Vec is
cloned _after_ splicing so removed nodes are not visited.

### `is_followed_by_newline`

Tiny helper: checks whether the byte at `node.end_pos` in the raw content
buffer is `\n`.  Kept separate to keep `strip_space_before_comments`
readable.


```rust
// <[ast strip spaces]>=
pub fn strip_space_before_comments(
    content: &[u8],
    parser: &mut Parser,
    node_idx: usize,
) -> Result<(), ASTError> {
    let mut to_remove: Vec<usize> = Vec::new();
    let mut spaces_to_strip: Vec<usize> = Vec::new();

    // Analysis phase: walk forward; when we hit a comment, walk back over
    // all consecutive Space nodes preceding it.
    {
        let node = parser
            .get_node(node_idx)
            .ok_or(ASTError::NodeNotFound(node_idx))?;

        let mut i = 0;
        while i < node.parts.len() {
            let part_idx = node.parts[i];
            let part = parser
                .get_node(part_idx)
                .ok_or(ASTError::NodeNotFound(part_idx))?;

            let is_line_comment = part.kind == NodeKind::LineComment;
            let is_block_comment = part.kind == NodeKind::BlockComment;

            if is_line_comment || is_block_comment {
                let block_comment_newline = if is_block_comment {
                    is_followed_by_newline(content, parser, part_idx)?
                } else {
                    false
                };

                if is_line_comment || block_comment_newline {
                    // Walk back over ALL consecutive Space nodes.
                    let mut j = i;
                    while j > 0 {
                        let prev_idx = node.parts[j - 1];
                        let prev = parser
                            .get_node(prev_idx)
                            .ok_or(ASTError::NodeNotFound(prev_idx))?;
                        if prev.kind == NodeKind::Space {
                            to_remove.push(j - 1);
                            j -= 1;
                        } else {
                            // Not a Space — trim trailing spaces from a
                            // preceding Text node, then stop.
                            if prev.kind == NodeKind::Text {
                                spaces_to_strip.push(prev_idx);
                            }
                            break;
                        }
                    }
                }
            }
            i += 1;
        }
    }

    // Modification phase
    if !to_remove.is_empty() {
        // De-duplicate (a single Space may be adjacent to two comments).
        to_remove.sort_unstable();
        to_remove.dedup();
        let node = parser
            .get_node_mut(node_idx)
            .ok_or(ASTError::NodeNotFound(node_idx))?;
        for &idx in to_remove.iter().rev() {
            node.parts.remove(idx);
        }
    }

    for idx in spaces_to_strip {
        parser.strip_ending_space(content, idx)?;
    }

    // Recurse into children (re-read after modification to skip removed nodes)
    let children: Vec<usize> = parser
        .get_node(node_idx)
        .ok_or(ASTError::NodeNotFound(node_idx))?
        .parts
        .clone();
    for child_idx in children {
        strip_space_before_comments(content, parser, child_idx)?;
    }

    Ok(())
}

fn is_followed_by_newline(
    content: &[u8],
    parser: &Parser,
    node_idx: usize,
) -> Result<bool, ASTError> {
    let node = parser
        .get_node(node_idx)
        .ok_or(ASTError::NodeNotFound(node_idx))?;
    let end_pos = node.end_pos;

    Ok(end_pos < content.len() && content[end_pos] == b'\n')
}
// @
```


## Invariants

After `process_ast` completes the following hold for every `ASTNode` in
the returned tree.  Tests assert subsets of these; `debug_assert!` guards
catch violations in debug builds at the point of construction.

[cols="1,3",options="header"]
|===
| Invariant | Explanation

| No comment nodes
| `clean_node` returns `None` for `LineComment` and `BlockComment` nodes;
callers in `clean_node`'s own recursion skip `None` results.  The root
`build_ast` call surfaces a `None` root as an error.

| `Param.name` iff `SeenEqual`
| `analyze_param` sets `name: Some(token)` exactly when the DFA reaches
`SeenEqual`, i.e. the pattern `Ident =` was found.  Positional params
always have `name: None`.

| Leaf nodes have no children
| `Text`, `Space`, `Ident`, and `Equal` nodes are lexer terminals; they
carry no children in the parse arena and therefore produce `ASTNode`s
with `parts: vec![]`.  Asserted by `debug_assert!` in `clean_node`.

| BFS output is contiguous
| The serialization walk writes nodes in BFS order; each node's children
occupy a contiguous range immediately following all previously written
sibling subtrees.  Consumers can compute child ranges from child counts
alone without storing back-references.
|===

## Serialization (`serialization.rs`)

### Preamble


```rust
// <[ast serialization preamble]>=
use crate::evaluator::{lex_parse_content, EvalError};
use crate::types::{ASTNode, Token};
use std::collections::VecDeque;
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::PathBuf;
// @
```


### Token serialization

Tokens are encoded as a comma-separated tuple: `src,kind,pos,length`.
`src` is a file-index integer assigned by the lexer; `kind` is the
`TokenKind` discriminant cast to `i32` for a stable numeric wire format.


```rust
// <[ast serialize token]>=
fn serialize_token(token: &Token) -> String {
    format!("{},{},{},{}", token.src, token.kind as i32, token.pos, token.length)
}
// @
```


### BFS node serialization

Each node is encoded as a JSON-like array.  The token tuple is expanded
_inline_ by `serialize_token` (which returns a comma-separated string), so
the full wire format is:

----
[node_kind, token_src, token_kind, token_pos, token_length, end_pos, [child_indices...]]
----

The `format!` call has four `{}` slots — `node_kind`, `serialize_token(…)`,
`end_pos`, and `parts` — but `serialize_token` itself emits four
comma-separated fields, yielding seven fields total in the output line.

BFS traversal (via `VecDeque`) ensures that child indices are always a
contiguous range starting at `next_idx`.  The root is always at index 0.
`next_idx` is advanced by `node.parts.len()` for each dequeued node,
mirroring the order in which children are enqueued.


```rust
// <[ast serialize nodes]>=
pub fn serialize_ast_nodes(root: &ASTNode) -> Vec<String> {
    let mut nodes = Vec::new();
    // BFS so that child indices assigned as next_idx..next_idx+n are contiguous
    // and land exactly where each node ends up in the output array.
    let mut queue: VecDeque<&ASTNode> = VecDeque::new();
    let mut next_idx = 1usize; // root is index 0

    // We don't need to write src because we process one file at a time and the caller knows which
    queue.push_back(root);
    while let Some(node) = queue.pop_front() {
        let child_indices: Vec<usize> = (next_idx..next_idx + node.parts.len()).collect();
        next_idx += node.parts.len();

        let parts = if child_indices.is_empty() {
            "[]".to_string()
        } else {
            format!(
                "[{}]",
                child_indices
                    .iter()
                    .map(|i| i.to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            )
        };
        nodes.push(format!(
            "[{},{},{},{}]",
            node.kind as i32,
            serialize_token(&node.token),
            node.end_pos,
            parts,
        ));

        for child in &node.parts {
            queue.push_back(child);
        }
    }

    nodes
}
// @
```


### Writing AST output

`write_ast` is the generic writer; `write_ast_to_file` adds stdout-vs-file
dispatch based on the `-` convention used throughout the codebase.


```rust
// <[ast write ast]>=
pub fn write_ast<W: Write>(header: &str, nodes: &[String], writer: &mut W) -> io::Result<()> {
    writeln!(writer, "{}", header)?;
    for line in nodes {
        writeln!(writer, "{}", line)?;
    }
    Ok(())
}

pub fn write_ast_to_file(header: &str, nodes: &[String], output_path: &PathBuf) -> io::Result<()> {
    if output_path.to_str() == Some("-") {
        let stdout = io::stdout();
        let mut handle = stdout.lock();
        write_ast(header, nodes, &mut handle)
    } else {
        let mut file = File::create(output_path)?;
        write_ast(header, nodes, &mut file)
    }
}
// @
```


### Input reading

`read_input` applies the same `-`-means-stdin convention to the input side.


```rust
// <[ast read input]>=
fn read_input(input: &PathBuf) -> io::Result<String> {
    if input.to_str() == Some("-") {
        let mut buffer = String::new();
        io::stdin().read_to_string(&mut buffer)?;
        Ok(buffer)
    } else {
        std::fs::read_to_string(input)
    }
}
// @
```


### CLI dump entry point

`dump_macro_ast` is the public API called by the binary.  For each input
file it lexes, parses, and serializes the AST, then writes it alongside
the source with a `.ast` extension (or to stdout for `-`).

The header line carries a `# src:0=<path>` annotation so external
consumers can map the `src` field of each token back to the originating
file.


```rust
// <[ast dump]>=
pub fn dump_macro_ast(sigil: char, input_files: &[PathBuf]) -> Result<(), EvalError> {
    for input in input_files {
        let content = read_input(input).map_err(|e| {
            EvalError::Runtime(format!("Failed to read {}: {}", input.display(), e))
        })?;

        let ast = lex_parse_content(&content, sigil, 0)?;
        let nodes = serialize_ast_nodes(&ast);

        let (output, src_name) = if input.to_str() == Some("-") {
            (PathBuf::from("-"), "-".to_string())
        } else {
            (input.with_extension("ast"), input.display().to_string())
        };

        // Header line: maps src indices to source file paths.
        // Format: # src:<index>=<path>  (one per source file; currently always src:0)
        let header = format!("# src:0={}", src_name);

        write_ast_to_file(&header, &nodes, &output).map_err(|e| {
            EvalError::Runtime(format!("Failed to write {}: {}", output.display(), e))
        })?;
    }
    Ok(())
}
// @
```


### Serialization tests


```rust
// <[ast serialization tests]>=
#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ASTNode, NodeKind, Token, TokenKind};
    use tempfile::tempdir;

    fn token(kind: TokenKind, pos: usize, length: usize) -> Token {
        Token { src: 0, kind, pos, length }
    }

    fn sample_ast() -> ASTNode {
        ASTNode {
            kind: NodeKind::Block,
            src: 0,
            token: Token::synthetic(0, 0),
            end_pos: 7,
            name: None,
            parts: vec![
                ASTNode {
                    kind: NodeKind::Text,
                    src: 0,
                    token: token(TokenKind::Text, 0, 3),
                    end_pos: 3,
                    name: None,
                    parts: vec![],
                },
                ASTNode {
                    kind: NodeKind::Macro,
                    src: 0,
                    token: token(TokenKind::Macro, 3, 4),
                    end_pos: 7,
                    name: None,
                    parts: vec![ASTNode {
                        kind: NodeKind::Param,
                        src: 0,
                        token: token(TokenKind::Ident, 4, 2),
                        end_pos: 6,
                        name: Some(token(TokenKind::Ident, 4, 1)),
                        parts: vec![],
                    }],
                },
            ],
        }
    }

    #[test]
    fn serialize_ast_nodes_emits_breadth_first_indices() {
        let lines = serialize_ast_nodes(&sample_ast());
        assert_eq!(lines.len(), 4);
        assert!(lines[0].ends_with("[1,2]]"));
        assert!(lines[1].ends_with("[]]"));
        assert!(lines[2].ends_with("[3]]"));
        assert!(lines[3].ends_with("[]]"));
    }

    #[test]
    fn write_ast_and_write_ast_to_file_emit_expected_content() {
        let nodes = vec!["[10,0,0,0,[]]".to_string(), "[1,0,0,3,[]]".to_string()];
        let mut out = Vec::new();
        write_ast("# src:0=input", &nodes, &mut out).expect("write ast");
        let text = String::from_utf8(out).expect("utf8");
        assert_eq!(text, "# src:0=input\n[10,0,0,0,[]]\n[1,0,0,3,[]]\n");

        let dir = tempdir().expect("tempdir");
        let output = dir.path().join("sample.ast");
        write_ast_to_file("# src:0=input", &nodes, &output).expect("write file");
        assert_eq!(std::fs::read_to_string(output).expect("read file"), text);
    }

    #[test]
    fn dump_macro_ast_writes_ast_file_next_to_input() {
        let dir = tempdir().expect("tempdir");
        let input = dir.path().join("sample.txt");
        std::fs::write(&input, "hello %name(world)").expect("write input");

        dump_macro_ast('%', std::slice::from_ref(&input)).expect("dump ast");

        let output = input.with_extension("ast");
        let text = std::fs::read_to_string(output).expect("read ast");
        assert!(text.starts_with("# src:0="));
        assert!(text.lines().count() > 1);
    }
}
// @
```


## Tests

Key tests and what they guard:

* **`test_node_kind_discriminants`** — regression guard for the numeric
  discriminant values.  `NodeKind::NotUsed = 0` is intentional: Python's
  `IntEnum` starts at 1, so reserving 0 keeps the wire format aligned.
* **`test_serialize_bfs_child_indices`** — verifies BFS ordering.  With
  an old DFS implementation, node B landed at index 4 instead of 2.
* **`test_serialize_token_src_field_present`** — `token.src` must appear
  in the output so external evaluators can trace which source file a node
  came from.
* **`test_strip_*`** — cover the cases of `strip_space_before_comments`:
  Space node removed, Text node trimmed, block comment followed by newline
  (stripped), block comment not followed by newline (kept), multiple
  consecutive Space nodes all removed before a comment, tab trimming, and
  spaces before two consecutive line comments both removed.
* **DFA edge cases** — `test_param_double_equals_value_starts_with_equal`
  (Equal as first value token), `test_param_var_as_first_token_is_positional`,
  `test_param_block_as_first_token_is_positional` — confirm the invariant
  that only `Ident` opens the named-detection path.
* **`test_pipeline_*`** — integration tests through the full
  lex → parse → strip → AST pipeline.
* **`test_strip_is_idempotent`** — running `strip_space_before_comments`
  twice produces the same result as running it once (no double-trim).
* **`test_ast_no_comments_invariant`** — `lex_parse_content` leaves no
  `LineComment` or `BlockComment` nodes anywhere in the AST tree.


```rust
// <[ast tests]>=
// src/ast/tests.rs
use super::*;
use crate::ParseNode;
use crate::parser::Parser;
use crate::types::{NodeKind, Token, TokenKind};

/// Helper to create a basic token
fn t(kind: TokenKind, pos: usize, length: usize) -> Token {
    Token {
        src: 0,
        kind,
        pos,
        length,
    }
}

/// Helper to create a node and add it to parser, returning its index
fn n(parser: &mut Parser, kind: NodeKind, pos: usize, length: usize, parts: Vec<usize>) -> usize {
    parser.add_node(ParseNode {
        kind,
        src: 0,
        token: t(TokenKind::Text, pos, length),
        end_pos: pos + length,
        parts,
    })
}

/// Builder to create sequence of nodes
struct NodeBuilder {
    pos: usize,
    nodes: Vec<(NodeKind, usize, usize)>, // Store (kind, pos, length)
}

impl NodeBuilder {
    fn new() -> Self {
        Self {
            pos: 0,
            nodes: Vec::new(),
        }
    }

    fn space(&mut self, length: usize) -> usize {
        let idx = self.nodes.len();
        self.nodes.push((NodeKind::Space, self.pos, length));
        self.pos += length;
        idx
    }

    fn text(&mut self, length: usize) -> usize {
        let idx = self.nodes.len();
        self.nodes.push((NodeKind::Text, self.pos, length));
        self.pos += length;
        idx
    }

    fn ident(&mut self, length: usize) -> usize {
        let idx = self.nodes.len();
        self.nodes.push((NodeKind::Ident, self.pos, length));
        self.pos += length;
        idx
    }

    fn comment(&mut self, length: usize) -> usize {
        let idx = self.nodes.len();
        self.nodes.push((NodeKind::LineComment, self.pos, length));
        self.pos += length;
        idx
    }

    fn equals(&mut self) -> usize {
        let idx = self.nodes.len();
        self.nodes.push((NodeKind::Equal, self.pos, 1));
        self.pos += 1;
        idx
    }

    fn build_nodes(&self, parser: &mut Parser) -> Vec<usize> {
        let mut indices = Vec::new();
        for &(kind, pos, length) in &self.nodes {
            indices.push(n(parser, kind, pos, length, vec![]));
        }
        indices
    }

    fn param(&self, parser: &mut Parser) -> usize {
        let parts = self.build_nodes(parser);
        n(parser, NodeKind::Param, 0, self.pos, parts)
    }
}

/// Helper to verify AST node structure
fn check_node(node: &ASTNode, expected_kind: NodeKind, expected_parts: usize) {
    assert_eq!(node.kind, expected_kind);
    assert_eq!(node.parts.len(), expected_parts);
}

#[test]
fn test_param_identifier_only() {
    let mut parser = Parser::new();
    let mut builder = NodeBuilder::new();
    builder.space(1);
    builder.ident(3);
    builder.space(1);
    let param_idx = builder.param(&mut parser);
    let result = analyze_param(&parser, param_idx).unwrap().unwrap();
    check_node(&result, NodeKind::Param, 2);
    check_node(&result.parts[0], NodeKind::Ident, 0);
    check_node(&result.parts[1], NodeKind::Space, 0);
}

#[test]
fn test_empty_param() {
    // analyze_param returns Some for empty param; trailing-empty trimming
    // happens at Macro level via trim_trailing_empty_params.
    let mut parser = Parser::new();
    let builder = NodeBuilder::new();
    let param_idx = builder.param(&mut parser);
    let result = analyze_param(&parser, param_idx).unwrap();
    assert!(result.is_some());
    assert!(result.unwrap().parts.is_empty());
}

#[test]
fn test_param_with_comments() {
    let mut parser = Parser::new();
    let mut builder = NodeBuilder::new();
    builder.comment(1);
    builder.ident(3);
    builder.comment(1);
    let param_idx = builder.param(&mut parser);
    let result = analyze_param(&parser, param_idx).unwrap().unwrap();
    check_node(&result, NodeKind::Param, 1);
    check_node(&result.parts[0], NodeKind::Ident, 0);
    assert_eq!(result.parts[0].token.pos, 1);
    assert_eq!(result.parts[0].token.length, 3);
}

#[test]
fn test_param_value_only() {
    let mut parser = Parser::new();
    let mut builder = NodeBuilder::new();
    builder.space(1);
    builder.text(3);
    builder.space(1);
    let param_idx = builder.param(&mut parser);
    let result = analyze_param(&parser, param_idx).unwrap().unwrap();
    check_node(&result, NodeKind::Param, 2);
    assert!(result.name.is_none());
    check_node(&result.parts[0], NodeKind::Text, 0);
    check_node(&result.parts[1], NodeKind::Space, 0);
}

#[test]
fn test_param_name_equals_value() {
    let mut parser = Parser::new();
    let mut builder = NodeBuilder::new();
    builder.ident(3);
    builder.equals();
    builder.text(4);
    let param_idx = builder.param(&mut parser);
    let result = analyze_param(&parser, param_idx).unwrap().unwrap();
    check_node(&result, NodeKind::Param, 1);
    assert!(result.name.is_some());
    check_node(&result.parts[0], NodeKind::Text, 0);
    let name = result.name.unwrap();
    assert_eq!(name.pos, 0);
    assert_eq!(name.length, 3);
}

#[test]
fn test_param_equals_without_ident() {
    let mut parser = Parser::new();
    let mut builder = NodeBuilder::new();
    builder.equals();
    builder.text(4);
    let param_idx = builder.param(&mut parser);
    let result = analyze_param(&parser, param_idx).unwrap().unwrap();
    check_node(&result, NodeKind::Param, 2);
    assert!(result.name.is_none());
    check_node(&result.parts[0], NodeKind::Equal, 0);
    check_node(&result.parts[1], NodeKind::Text, 0);
}

#[test]
fn test_param_equals_with_blank() {
    let mut parser = Parser::new();
    let mut builder = NodeBuilder::new();
    builder.ident(3);
    builder.equals();
    let param_idx = builder.param(&mut parser);
    let result = analyze_param(&parser, param_idx).unwrap().unwrap();
    check_node(&result, NodeKind::Param, 0);
    assert!(result.name.is_some());
    let name = result.name.unwrap();
    assert_eq!(name.pos, 0);
    assert_eq!(name.length, 3);
}

#[test]
fn test_param_equals_only_comment() {
    let mut parser = Parser::new();
    let mut builder = NodeBuilder::new();
    builder.ident(3);
    builder.equals();
    builder.comment(5);
    let param_idx = builder.param(&mut parser);
    let result = analyze_param(&parser, param_idx).unwrap().unwrap();
    check_node(&result, NodeKind::Param, 0);
    assert!(result.name.is_some());
    let name = result.name.unwrap();
    assert_eq!(name.pos, 0);
    assert_eq!(name.length, 3);
}

#[test]
fn test_trailing_comma_empty_param_is_ignored() {
    // analyze_param preserves empty params; trailing-empty trimming
    // at Macro level removes them from the Macro node's parts.
    let mut parser = Parser::new();
    let builder = NodeBuilder::new();
    let param_idx = builder.param(&mut parser);
    let result = analyze_param(&parser, param_idx).unwrap();
    assert!(result.is_some());
}

#[test]
fn test_param_complex_spacing() {
    let mut parser = Parser::new();
    let mut builder = NodeBuilder::new();
    builder.space(2);
    builder.comment(4);
    builder.space(1);
    builder.ident(3);
    builder.space(2);
    builder.comment(5);
    builder.equals();
    builder.space(3);
    builder.comment(4);
    builder.text(4);
    let param_idx = builder.param(&mut parser);
    let result = analyze_param(&parser, param_idx).unwrap().unwrap();
    check_node(&result, NodeKind::Param, 1);
    assert!(result.name.is_some());
    check_node(&result.parts[0], NodeKind::Text, 0);
}

#[test]
fn test_param_multiple_equals() {
    let mut parser = Parser::new();
    let mut builder = NodeBuilder::new();
    builder.ident(3);
    builder.equals();
    builder.text(2);
    builder.equals();
    builder.text(2);
    let param_idx = builder.param(&mut parser);
    let result = analyze_param(&parser, param_idx).unwrap().unwrap();
    assert!(result.name.is_some());
    assert_eq!(result.parts.len(), 3);
    check_node(&result.parts[0], NodeKind::Text, 0);
    check_node(&result.parts[1], NodeKind::Equal, 0);
    check_node(&result.parts[2], NodeKind::Text, 0);
}

#[test]
fn test_param_multiple_idents() {
    let mut parser = Parser::new();
    let mut builder = NodeBuilder::new();
    builder.ident(3);
    builder.space(1);
    builder.ident(3);
    let param_idx = builder.param(&mut parser);
    let result = analyze_param(&parser, param_idx).unwrap().unwrap();
    assert!(result.name.is_none());
    assert_eq!(result.parts.len(), 3);
    check_node(&result.parts[0], NodeKind::Ident, 0);
    check_node(&result.parts[1], NodeKind::Space, 0);
    check_node(&result.parts[2], NodeKind::Ident, 0);
}

#[test]
fn test_param_mixed_content() {
    let mut parser = Parser::new();
    let mut builder = NodeBuilder::new();
    builder.text(2);
    builder.space(1);
    builder.ident(3);
    builder.equals();
    builder.text(2);
    let param_idx = builder.param(&mut parser);
    let result = analyze_param(&parser, param_idx).unwrap().unwrap();
    assert!(result.name.is_none());
    assert_eq!(result.parts.len(), 5);
    check_node(&result.parts[0], NodeKind::Text, 0);
    check_node(&result.parts[1], NodeKind::Space, 0);
    check_node(&result.parts[2], NodeKind::Ident, 0);
    check_node(&result.parts[3], NodeKind::Equal, 0);
    check_node(&result.parts[4], NodeKind::Text, 0);
}

#[test]
fn test_param_complex_nesting() {
    let mut parser = Parser::new();
    let text1_idx     = n(&mut parser, NodeKind::Text,  1, 3, vec![]);
    let var_idx       = n(&mut parser, NodeKind::Var,   4, 5, vec![]);
    let space_idx     = n(&mut parser, NodeKind::Space, 9, 1, vec![]);
    let macro_text_idx = n(&mut parser, NodeKind::Text, 11, 3, vec![]);
    let text2_idx     = n(&mut parser, NodeKind::Text, 18, 2, vec![]);
    let macro_param_idx = n(&mut parser, NodeKind::Param, 11, 3, vec![macro_text_idx]);
    let macro_idx     = n(&mut parser, NodeKind::Macro, 10, 8, vec![macro_param_idx]);
    let block_idx     = n(&mut parser, NodeKind::Block,  0, 20,
                          vec![text1_idx, var_idx, space_idx, macro_idx, text2_idx]);
    let param_idx     = n(&mut parser, NodeKind::Param,  0, 20, vec![block_idx]);
    let result = analyze_param(&parser, param_idx).unwrap().unwrap();
    assert!(result.name.is_none());
    assert_eq!(result.parts.len(), 1);
    let block = &result.parts[0];
    assert_eq!(block.kind, NodeKind::Block);
    assert_eq!(block.parts.len(), 5);
    check_node(&block.parts[0], NodeKind::Text, 0);
    check_node(&block.parts[1], NodeKind::Var, 0);
    check_node(&block.parts[2], NodeKind::Space, 0);
    check_node(&block.parts[3], NodeKind::Macro, 1);
    check_node(&block.parts[4], NodeKind::Text, 0);
}

#[test]
fn test_param_nested_equals() {
    let mut parser = Parser::new();
    let ident_idx  = n(&mut parser, NodeKind::Ident, 0, 3, vec![]);
    let equal1_idx = n(&mut parser, NodeKind::Equal, 3, 1, vec![]);
    let text1_idx  = n(&mut parser, NodeKind::Text,  4, 3, vec![]);
    let equal2_idx = n(&mut parser, NodeKind::Equal, 7, 1, vec![]);
    let text2_idx  = n(&mut parser, NodeKind::Text,  8, 4, vec![]);
    let block_idx  = n(&mut parser, NodeKind::Block, 4, 8,
                       vec![text1_idx, equal2_idx, text2_idx]);
    let param_idx  = n(&mut parser, NodeKind::Param, 0, 12,
                       vec![ident_idx, equal1_idx, block_idx]);
    let result = analyze_param(&parser, param_idx).unwrap().unwrap();
    assert!(result.name.is_some());
    assert_eq!(result.parts.len(), 1);
    let block = &result.parts[0];
    assert_eq!(block.kind, NodeKind::Block);
    assert_eq!(block.parts.len(), 3);
    check_node(&block.parts[0], NodeKind::Text, 0);
    check_node(&block.parts[1], NodeKind::Equal, 0);
    check_node(&block.parts[2], NodeKind::Text, 0);
}

#[test]
fn test_param_with_block() {
    let mut parser = Parser::new();
    let name_idx  = n(&mut parser, NodeKind::Ident, 0, 3, vec![]);
    let equal_idx = n(&mut parser, NodeKind::Equal, 3, 1, vec![]);
    let text1_idx = n(&mut parser, NodeKind::Text,  5, 3, vec![]);
    let space_idx = n(&mut parser, NodeKind::Space, 8, 1, vec![]);
    let text2_idx = n(&mut parser, NodeKind::Text,  9, 4, vec![]);
    let block_idx = n(&mut parser, NodeKind::Block, 4, 10,
                      vec![text1_idx, space_idx, text2_idx]);
    let param_idx = n(&mut parser, NodeKind::Param, 0, 14,
                      vec![name_idx, equal_idx, block_idx]);
    let result = analyze_param(&parser, param_idx).unwrap().unwrap();
    assert!(result.name.is_some());
    assert_eq!(result.parts.len(), 1);
    let block = &result.parts[0];
    assert_eq!(block.kind, NodeKind::Block);
    assert_eq!(block.parts.len(), 3);
    check_node(&block.parts[0], NodeKind::Text, 0);
    check_node(&block.parts[1], NodeKind::Space, 0);
    check_node(&block.parts[2], NodeKind::Text, 0);
}

#[test]
fn test_param_with_var() {
    let mut parser = Parser::new();
    let text1_idx = n(&mut parser, NodeKind::Text,  0, 3, vec![]);
    let space_idx = n(&mut parser, NodeKind::Space, 3, 1, vec![]);
    let var_idx   = n(&mut parser, NodeKind::Var,   4, 5, vec![]);
    let text2_idx = n(&mut parser, NodeKind::Text,  9, 2, vec![]);
    let param_idx = n(&mut parser, NodeKind::Param, 0, 11,
                      vec![text1_idx, space_idx, var_idx, text2_idx]);
    let result = analyze_param(&parser, param_idx).unwrap().unwrap();
    assert!(result.name.is_none());
    assert_eq!(result.parts.len(), 4);
    check_node(&result.parts[0], NodeKind::Text, 0);
    check_node(&result.parts[1], NodeKind::Space, 0);
    check_node(&result.parts[2], NodeKind::Var, 0);
    check_node(&result.parts[3], NodeKind::Text, 0);
}

#[test]
fn test_param_with_nested_macro() {
    let mut parser = Parser::new();
    let name_idx       = n(&mut parser, NodeKind::Ident, 0, 3, vec![]);
    let equal_idx      = n(&mut parser, NodeKind::Equal, 3, 1, vec![]);
    let text_idx       = n(&mut parser, NodeKind::Text,  5, 3, vec![]);
    let macro_param_idx = n(&mut parser, NodeKind::Param, 5, 3, vec![text_idx]);
    let macro_idx      = n(&mut parser, NodeKind::Macro, 4, 8, vec![macro_param_idx]);
    let param_idx      = n(&mut parser, NodeKind::Param, 0, 12,
                           vec![name_idx, equal_idx, macro_idx]);
    let result = analyze_param(&parser, param_idx).unwrap().unwrap();
    assert!(result.name.is_some());
    assert_eq!(result.parts.len(), 1);
    check_node(&result.parts[0], NodeKind::Macro, 1);
}

// ── DFA edge cases ─────────────────────────────────────────────────────

#[test]
fn test_param_double_equals_value_starts_with_equal() {
    // `ident = = text`: second Equal is the first value token.
    // Distinct from test_param_multiple_equals which is `ident = text = text`.
    let mut parser = Parser::new();
    let mut builder = NodeBuilder::new();
    builder.ident(3);  // foo
    builder.equals();  // first =  → SeenEqual
    builder.equals();  // second = → first_good_after_equal, value starts here
    builder.text(3);   // bar
    let param_idx = builder.param(&mut parser);
    let result = analyze_param(&parser, param_idx).unwrap().unwrap();
    assert!(result.name.is_some(), "should be named param");
    assert_eq!(result.name.unwrap().length, 3, "name should be 'foo'");
    // Value part list starts at the second Equal, so parts = [Equal, Text].
    assert_eq!(result.parts.len(), 2);
    check_node(&result.parts[0], NodeKind::Equal, 0);
    check_node(&result.parts[1], NodeKind::Text,  0);
}

#[test]
fn test_param_var_as_first_token_is_positional() {
    // Only Ident can start the named-detection branch; Var must produce positional.
    let mut parser = Parser::new();
    let var_idx   = n(&mut parser, NodeKind::Var,   0, 5, vec![]);
    let text_idx  = n(&mut parser, NodeKind::Text,  5, 3, vec![]);
    let param_idx = n(&mut parser, NodeKind::Param, 0, 8, vec![var_idx, text_idx]);
    let result = analyze_param(&parser, param_idx).unwrap().unwrap();
    assert!(result.name.is_none(), "Var-prefixed param should be positional");
    assert_eq!(result.parts.len(), 2);
    check_node(&result.parts[0], NodeKind::Var,  0);
    check_node(&result.parts[1], NodeKind::Text, 0);
}

#[test]
fn test_param_block_as_first_token_is_positional() {
    // Block before Ident: DFA breaks immediately in Start state.
    // Even though `= text` follows, the whole param is positional.
    let mut parser = Parser::new();
    let inner_idx = n(&mut parser, NodeKind::Text,  1, 3, vec![]);
    let block_idx = n(&mut parser, NodeKind::Block, 0, 5, vec![inner_idx]);
    let equal_idx = n(&mut parser, NodeKind::Equal, 5, 1, vec![]);
    let text_idx  = n(&mut parser, NodeKind::Text,  7, 3, vec![]);
    let param_idx = n(&mut parser, NodeKind::Param, 0, 10,
                      vec![block_idx, equal_idx, text_idx]);
    let result = analyze_param(&parser, param_idx).unwrap().unwrap();
    assert!(result.name.is_none(), "Block-prefixed param should be positional");
    assert_eq!(result.parts.len(), 3);
    check_node(&result.parts[0], NodeKind::Block, 1); // block with its inner Text
    check_node(&result.parts[1], NodeKind::Equal, 0);
    check_node(&result.parts[2], NodeKind::Text,  0);
}

// ── NodeKind discriminants — regression guard ──────────────────────────

#[test]
fn test_node_kind_discriminants() {
    // NotUsed=0 is intentional: Python IntEnum starts at 1 by default,
    // so reserving 0 keeps Rust and Python discriminants aligned.
    assert_eq!(NodeKind::NotUsed as i32, 0);
    assert_eq!(NodeKind::Text as i32, 1);
    assert_eq!(NodeKind::Space as i32, 2);
    assert_eq!(NodeKind::Ident as i32, 3);
    assert_eq!(NodeKind::LineComment as i32, 4);
    assert_eq!(NodeKind::BlockComment as i32, 5);
    assert_eq!(NodeKind::Var as i32, 6);
    assert_eq!(NodeKind::Equal as i32, 7);
    assert_eq!(NodeKind::Param as i32, 8);
    assert_eq!(NodeKind::Macro as i32, 9);
    assert_eq!(NodeKind::Block as i32, 10);
}

// ── serialize_ast_nodes — BFS ordering ────────────────────────────────

#[test]
fn test_serialize_bfs_child_indices() {
    // Tree: Root[A, B], A[C, D], B[], C[], D[]
    // With the old DFS traversal B landed at index 4 instead of 2.
    // BFS guarantees: Root→[1,2], A→[3,4], B/C/D are leaves at 2/3/4.
    let tok = |pos| Token { src: 0, kind: TokenKind::Text, pos, length: 1 };
    let c = ASTNode { kind: NodeKind::Text,  src: 0, token: tok(3), end_pos: 4, parts: vec![], name: None };
    let d = ASTNode { kind: NodeKind::Text,  src: 0, token: tok(4), end_pos: 5, parts: vec![], name: None };
    let b = ASTNode { kind: NodeKind::Text,  src: 0, token: tok(2), end_pos: 3, parts: vec![], name: None };
    let a = ASTNode { kind: NodeKind::Macro, src: 0, token: tok(1), end_pos: 6, parts: vec![c, d], name: None };
    let root = ASTNode { kind: NodeKind::Block, src: 0, token: tok(0), end_pos: 7, parts: vec![a, b], name: None };
    let nodes = serialize_ast_nodes(&root);
    assert_eq!(nodes.len(), 5);
    assert!(nodes[0].contains("[1,2]"),  "root children: {}", nodes[0]);
    assert!(nodes[1].contains("[3,4]"),  "A children: {}",    nodes[1]);
    assert!(nodes[2].ends_with(",[]]"), "B should be leaf: {}", nodes[2]);
    assert!(nodes[3].ends_with(",[]]"), "C should be leaf: {}", nodes[3]);
    assert!(nodes[4].ends_with(",[]]"), "D should be leaf: {}", nodes[4]);
}

#[test]
fn test_serialize_bfs_deep_linear_chain() {
    // A → B → C → D (linear, each node has exactly one child).
    // BFS guarantees: A's child is at index 1, B's at 2, C's at 3, D is a leaf.
    let tok = |pos| Token { src: 0, kind: TokenKind::Text, pos, length: 1 };
    let d = ASTNode { kind: NodeKind::Text,  src: 0, token: tok(3), end_pos: 4, parts: vec![], name: None };
    let c = ASTNode { kind: NodeKind::Macro, src: 0, token: tok(2), end_pos: 4, parts: vec![d], name: None };
    let b = ASTNode { kind: NodeKind::Macro, src: 0, token: tok(1), end_pos: 4, parts: vec![c], name: None };
    let a = ASTNode { kind: NodeKind::Block, src: 0, token: tok(0), end_pos: 4, parts: vec![b], name: None };
    let nodes = serialize_ast_nodes(&a);
    assert_eq!(nodes.len(), 4);
    assert!(nodes[0].contains("[1]"), "A should point to child at 1: {}", nodes[0]);
    assert!(nodes[1].contains("[2]"), "B should point to child at 2: {}", nodes[1]);
    assert!(nodes[2].contains("[3]"), "C should point to child at 3: {}", nodes[2]);
    assert!(nodes[3].ends_with(",[]]"), "D should be a leaf: {}", nodes[3]);
}

#[test]
fn test_serialize_token_src_field_present() {
    // token.src must appear in the output so external evaluators can trace
    // which source file a node came from.
    let root = ASTNode {
        kind: NodeKind::Block,
        src: 0,
        token: Token { src: 2, kind: TokenKind::Text, pos: 5, length: 3 },
        end_pos: 8,
        parts: vec![],
        name: None,
    };
    let nodes = serialize_ast_nodes(&root);
    assert_eq!(nodes.len(), 1);
    assert!(nodes[0].starts_with("[10,2,"), "token src not present: {}", nodes[0]);
}

// ── strip_space_before_comments ────────────────────────────────────────

#[test]
fn test_strip_removes_space_node_before_line_comment() {
    let content = b"hello %%// comment\n";
    let mut parser = Parser::new();
    let text_idx    = n(&mut parser, NodeKind::Text,        0,  5, vec![]);
    let space_idx   = n(&mut parser, NodeKind::Space,       5,  1, vec![]);
    let comment_idx = n(&mut parser, NodeKind::LineComment, 6, 12, vec![]);
    let root_idx    = n(&mut parser, NodeKind::Block,       0, 18,
                        vec![text_idx, space_idx, comment_idx]);
    strip_space_before_comments(content, &mut parser, root_idx).unwrap();
    let root = parser.get_node(root_idx).unwrap();
    assert_eq!(root.parts.len(), 2);
    assert_eq!(root.parts[0], text_idx);
    assert_eq!(root.parts[1], comment_idx);
}

#[test]
fn test_strip_trims_trailing_spaces_in_text_before_line_comment() {
    let content = b"hello   %%// comment\n";
    let mut parser = Parser::new();
    let text_idx    = n(&mut parser, NodeKind::Text,        0,  8, vec![]);
    let comment_idx = n(&mut parser, NodeKind::LineComment, 8, 12, vec![]);
    let root_idx    = n(&mut parser, NodeKind::Block,       0, 20,
                        vec![text_idx, comment_idx]);
    strip_space_before_comments(content, &mut parser, root_idx).unwrap();
    let root = parser.get_node(root_idx).unwrap();
    assert_eq!(root.parts.len(), 2);
    let text = parser.get_node(text_idx).unwrap();
    assert_eq!(text.token.length, 5, "trailing spaces should be stripped from text");
}

#[test]
fn test_strip_removes_space_before_block_comment_followed_by_newline() {
    let content = b" %/* c %*/\nmore";
    let mut parser = Parser::new();
    let space_idx   = n(&mut parser, NodeKind::Space,        0,  1, vec![]);
    let comment_idx = n(&mut parser, NodeKind::BlockComment, 1,  9, vec![]);
    let text_idx    = n(&mut parser, NodeKind::Text,        11,  4, vec![]);
    let root_idx    = n(&mut parser, NodeKind::Block,        0, 15,
                        vec![space_idx, comment_idx, text_idx]);
    strip_space_before_comments(content, &mut parser, root_idx).unwrap();
    let root = parser.get_node(root_idx).unwrap();
    assert_eq!(root.parts.len(), 2);
    assert_eq!(root.parts[0], comment_idx);
    assert_eq!(root.parts[1], text_idx);
}

#[test]
fn test_no_strip_before_inline_block_comment() {
    let content = b" %/* c %*/ more";
    let mut parser = Parser::new();
    let space_idx   = n(&mut parser, NodeKind::Space,        0,  1, vec![]);
    let comment_idx = n(&mut parser, NodeKind::BlockComment, 1,  9, vec![]);
    let text_idx    = n(&mut parser, NodeKind::Text,        10,  5, vec![]);
    let root_idx    = n(&mut parser, NodeKind::Block,        0, 15,
                        vec![space_idx, comment_idx, text_idx]);
    strip_space_before_comments(content, &mut parser, root_idx).unwrap();
    let root = parser.get_node(root_idx).unwrap();
    assert_eq!(root.parts.len(), 3);
}

#[test]
fn test_strip_removes_multiple_spaces_before_line_comment() {
    // Text("hello") / Space / Space / LineComment — both Spaces must be removed.
    let content = b"hello  %%// comment\n";
    let mut parser = Parser::new();
    let text_idx     = n(&mut parser, NodeKind::Text,        0,  5, vec![]);
    let space1_idx   = n(&mut parser, NodeKind::Space,       5,  1, vec![]);
    let space2_idx   = n(&mut parser, NodeKind::Space,       6,  1, vec![]);
    let comment_idx  = n(&mut parser, NodeKind::LineComment, 7, 12, vec![]);
    let root_idx     = n(&mut parser, NodeKind::Block,       0, 19,
                         vec![text_idx, space1_idx, space2_idx, comment_idx]);
    strip_space_before_comments(content, &mut parser, root_idx).unwrap();
    let root = parser.get_node(root_idx).unwrap();
    // Both Space nodes must be gone; only Text + Comment remain.
    assert_eq!(root.parts.len(), 2,
        "expected 2 parts after stripping two spaces, got {}", root.parts.len());
    assert_eq!(root.parts[0], text_idx);
    assert_eq!(root.parts[1], comment_idx);
}

#[test]
fn test_strip_trims_trailing_tab_in_text_before_comment() {
    // `strip_ending_space` should strip tabs as well as ASCII spaces.
    // "hello\t" followed by a line comment — the tab must be stripped.
    let content = b"hello\t%%// c\n";
    let mut parser = Parser::new();
    // Text token covers "hello\t" (6 bytes), comment token covers "%%// c\n" (6 bytes).
    let text_idx    = n(&mut parser, NodeKind::Text,        0, 6, vec![]);
    let comment_idx = n(&mut parser, NodeKind::LineComment, 6, 6, vec![]);
    let root_idx    = n(&mut parser, NodeKind::Block,       0, 12,
                        vec![text_idx, comment_idx]);
    strip_space_before_comments(content, &mut parser, root_idx).unwrap();
    let text = parser.get_node(text_idx).unwrap();
    assert_eq!(text.token.length, 5,
        "trailing tab should be stripped — expected length 5, got {}", text.token.length);
}

#[test]
fn test_strip_removes_spaces_before_multiple_consecutive_comments() {
    // Text / Space / Comment1 / Space / Comment2:
    // both Space nodes must be removed (one before each comment).
    // Content layout: "text %%// c1\n %%// c2\n"
    //   0..4  "text"   (Text)
    //   4     " "      (Space1)
    //   5..11 "%%// c1" (LineComment1, ends at 11; next byte is \n at 11 but
    //                   we don't need newline accuracy for LineComment)
    //   12    " "      (Space2)
    //   13..19"%%// c2" (LineComment2)
    let content = b"text %%// c1\n %%// c2\n";
    let mut parser = Parser::new();
    let text_idx     = n(&mut parser, NodeKind::Text,        0,  4, vec![]);
    let space1_idx   = n(&mut parser, NodeKind::Space,       4,  1, vec![]);
    let comment1_idx = n(&mut parser, NodeKind::LineComment, 5,  7, vec![]);
    let space2_idx   = n(&mut parser, NodeKind::Space,      12,  1, vec![]);
    let comment2_idx = n(&mut parser, NodeKind::LineComment,13,  7, vec![]);
    let root_idx = n(&mut parser, NodeKind::Block, 0, 20,
                     vec![text_idx, space1_idx, comment1_idx, space2_idx, comment2_idx]);
    strip_space_before_comments(content, &mut parser, root_idx).unwrap();
    let root = parser.get_node(root_idx).unwrap();
    assert_eq!(root.parts.len(), 3,
        "expected [text, comment1, comment2], got {} parts", root.parts.len());
    assert_eq!(root.parts[0], text_idx);
    assert_eq!(root.parts[1], comment1_idx);
    assert_eq!(root.parts[2], comment2_idx);
}

// ── Full pipeline ──────────────────────────────────────────────────────

#[test]
fn test_pipeline_plain_text() {
    use crate::evaluator::lex_parse_content;
    let ast = lex_parse_content("hello world", '%', 0).unwrap();
    assert_eq!(ast.kind, NodeKind::Block);
    assert_eq!(ast.parts.len(), 1);
    assert_eq!(ast.parts[0].kind, NodeKind::Text);
}

#[test]
fn test_pipeline_comments_stripped_from_ast() {
    use crate::evaluator::lex_parse_content;
    let ast = lex_parse_content("before %// comment\nafter", '%', 0).unwrap();
    fn no_comments(node: &ASTNode) {
        assert_ne!(node.kind, NodeKind::LineComment, "LineComment leaked into AST");
        assert_ne!(node.kind, NodeKind::BlockComment, "BlockComment leaked into AST");
        for child in &node.parts { no_comments(child); }
    }
    no_comments(&ast);
    assert!(ast.parts.iter().any(|n| n.kind == NodeKind::Text));
}

#[test]
fn test_pipeline_var_node() {
    use crate::evaluator::lex_parse_content;
    let ast = lex_parse_content("%(x)", '%', 0).unwrap();
    assert!(ast.parts.iter().any(|n| n.kind == NodeKind::Var));
}

#[test]
fn test_pipeline_macro_with_named_param() {
    use crate::evaluator::lex_parse_content;
    let ast = lex_parse_content("%foo(a, b=val)", '%', 0).unwrap();
    let mac = ast.parts.iter().find(|n| n.kind == NodeKind::Macro)
        .expect("expected Macro node");
    assert_eq!(mac.parts.len(), 2);
    let unnamed = mac.parts.iter().find(|p| p.name.is_none()).expect("unnamed param");
    let named   = mac.parts.iter().find(|p| p.name.is_some()).expect("named param");
    assert_eq!(named.name.unwrap().length, 1);
    assert!(unnamed.parts.iter().any(|n| n.kind == NodeKind::Ident || n.kind == NodeKind::Text));
}

#[test]
fn test_pipeline_tagged_block() {
    use crate::evaluator::lex_parse_content;
    let ast = lex_parse_content("%foo{ content %foo}", '%', 0).unwrap();
    let block = ast.parts.iter().find(|n| n.kind == NodeKind::Block)
        .expect("expected Block node");
    assert!(!block.parts.is_empty());
}

#[test]
fn test_strip_is_idempotent() {
    use crate::Lexer;
    use crate::parser::Parser;
    let src = "hello %%// comment\nworld";
    let (tokens, _) = Lexer::new(src, '%', 0).lex();
    let li = crate::line_index::LineIndex::new(src);

    let mut parser = Parser::new();
    parser.parse(&tokens, src.as_bytes(), &li).unwrap();
    let root = 0;

    // First strip
    strip_space_before_comments(src.as_bytes(), &mut parser, root).unwrap();
    let ast1 = crate::ast::build_ast(&parser).unwrap();

    // Second strip on already-stripped parser
    strip_space_before_comments(src.as_bytes(), &mut parser, root).unwrap();
    let ast2 = crate::ast::build_ast(&parser).unwrap();

    // Both ASTs should have the same number of top-level parts
    assert_eq!(ast1.parts.len(), ast2.parts.len(), "strip is not idempotent");
}

#[test]
fn test_ast_no_comments_invariant() {
    use crate::evaluator::lex_parse_content;
    fn check_no_comments(node: &ASTNode) {
        assert!(
            !matches!(node.kind, NodeKind::LineComment | NodeKind::BlockComment),
            "comment node {:?} leaked into AST",
            node.kind
        );
        for child in &node.parts { check_no_comments(child); }
    }
    for src in &[
        "plain text",
        "before %%// line comment\nafter",
        "before %%/* block %%*/ mid after",
        "%%def(foo, body) %%foo()",
    ] {
        let ast = lex_parse_content(src, '%', 0).unwrap();
        check_no_comments(&ast);
    }
}
// @
```

