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

| State | Transitions in | Carries |
| --- | --- | --- |
| `Start` | initial state | — |
| `SeenName` | `Start` + `Ident` token | the `name: Token` and its `name_idx: usize` |
| `SeenEqual` | `SeenName` + `Equal` token | the `name: Token` |

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

```text
some text %%// strip me
```


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


The test module is generated from focused files under
`crates/weaveback-macro/src-wvb/ast/tests/`, with the shared module map in
`crates/weaveback-macro/src-wvb/ast/tests-assembly.wvb`.

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

| Pattern | Result |
| --- | --- |
| `[spaces] Ident [spaces] = value...` | Named param — `name` token set, parts start after `=` |
| `[spaces] Ident [spaces] =` (nothing after `=`) | Named param with blank value — `name` set, `parts` empty |
| anything else | Positional param — `name` is `None`, parts start from first non-skippable node |

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

<table>
  <tr><th>Invariant</th><th>Explanation</th></tr>
  <tr><td>No comment nodes</td><td>`clean_node` returns `None` for `LineComment` and `BlockComment` nodes;<br>
callers in `clean_node`&#39;s own recursion skip `None` results.  The root<br>
`build_ast` call surfaces a `None` root as an error.</td></tr>
  <tr><td>`Param.name` iff `SeenEqual`</td><td>`analyze_param` sets `name: Some(token)` exactly when the DFA reaches<br>
`SeenEqual`, i.e. the pattern `Ident =` was found.  Positional params<br>
always have `name: None`.</td></tr>
  <tr><td>Leaf nodes have no children</td><td>`Text`, `Space`, `Ident`, and `Equal` nodes are lexer terminals; they<br>
carry no children in the parse arena and therefore produce `ASTNode`s<br>
with `parts: vec![]`.  Asserted by `debug_assert!` in `clean_node`.</td></tr>
  <tr><td>BFS output is contiguous</td><td>The serialization walk writes nodes in BFS order; each node&#39;s children<br>
occupy a contiguous range immediately following all previously written<br>
sibling subtrees.  Consumers can compute child ranges from child counts<br>
alone without storing back-references.</td></tr>
</table>

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

```text
[node_kind, token_src, token_kind, token_pos, token_length, end_pos, [child_indices...]]
```


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

The AST tests are intentionally split away from this implementation document.
The test suite covers parameter classification, DFA edge cases, wire-format
stability, serialization ordering, comment stripping, and full lex/parse/AST
pipeline invariants.  See `src-wvb/ast/tests-assembly.wvb` and the child files
under `src-wvb/ast/tests/` for the canonical test sources.

