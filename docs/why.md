---
title: |-
  Why Weaveback — intent, tradeoffs, and design rationale
toc: left
toclevels: 3
---
# Why Weaveback — intent, tradeoffs, and design rationale

This document explains the *reasons* behind Weaveback's design. Start here
before reading the architecture or source code. The code answers *what* —
this page answers *why*.

## Why this still matters now

The project started from an older frustration: using macros and chunks to keep
related but distant pieces of a codebase in sync. That use case is still real.
What changed is the surrounding environment.

In the 1990s and 2000s, abstraction machinery often tried to save humans from
typing boilerplate. Today coding agents can generate boilerplate on demand.
That does *not* make design less important. It changes where the bottleneck
is.

Code production is now cheap. Understanding, reviewing, validating, and
preserving intent are expensive.

That is the reason Weaveback still exists. The goal is no longer merely "emit
less repetitive code". The goal is to keep *code, provenance, and rationale*
bound together strongly enough that a human can still judge the result.

This is the practical claim behind the project:

* agents reduce the cost of writing code
* they do not reduce the cost of deciding *why* code should exist in that form
* they often increase the volume of code that must later be inspected
* therefore the value of explicit intent, traceability, and reversible
  structure goes up, not down

This is also why "design patterns" need to be interpreted carefully.
As code stencils, many of them are less important than they once were.
An agent can emit a strategy-like or observer-like structure trivially.
But as *reasoning vocabulary* they still matter:

* what pressure is this abstraction responding to?
* what variability axis is being isolated?
* what coupling is being traded for indirection?
* what future change is this extra structure buying us?

An agent can generate the pattern-shaped code. It cannot remove the need for a
human to judge whether the pattern is justified.

That is why the old motto remains true: `Pensare non ha sostituti`.

Weaveback is worthwhile only if it reduces cognitive load at that level. If it
is used merely to create clever macro machinery, it is ornament. If it helps a
team preserve intent, keep distant pieces in sync, and audit agent-made changes,
it is doing real work.

## Tensions, not conclusions

These tradeoffs are not settled. They are the questions the project is trying
to answer.

Weaveback often gives up some degree of *local obviousness* in exchange for
*global traceability*. A direct, hand-written codebase is easier to read in the
small. A literate, reversible system may be easier to audit in the large. The
project is worthwhile only if that global benefit is real enough to justify the
local friction.

The same doubt applies to every meta-layer inside the project. A shared
declaration for CLI options, MCP tools, or configuration keys might reduce
drift. It might also just relocate complexity into a less familiar
intermediate form. If the abstraction becomes harder to understand than the
generated surfaces, it has failed.

Uniformity matters here. A half-adopted abstraction is often worse than
either:

* direct hand-written code and docs
* a fully adopted projection system for a clearly bounded surface

Partial adoption creates decision tax: every new option or feature turns into
"should this one use the macro layer or not?" That is not a victory over
duplication; it is another source of cognitive overhead.

So the working standard is intentionally severe:

* does this structure reduce drift?
* does it preserve rationale in a way comments alone do not?
* does it make agent-produced change easier to validate?
* does it lower the overall cost of understanding the system?

If the answer is no, the right move is not to defend the abstraction. The
right move is to remove it.

The synchronization story sits inside the same tension. `trace`,
`apply-back`, baseline protection, and MCP context do make the literate system
more usable than a one-way tangle tool. They close part of the loop that
traditional literate programming leaves open. But they do not eliminate the
cost entirely. Today the biggest friction is not prose itself; it is the
immaturity of the synchronization tooling around it: drift between `.adoc` and
generated files, diagnostics that still target the generated layer first,
manual reconciliation when the source of truth is unclear, and missing lint
checks for structural invariants. The tools are already good enough to make
the approach defensible. They are not yet good enough to make the cost
disappear.

## The core problem

Literate programming is a fifty-year-old idea: write prose and code together,
with the prose explaining the intent behind the code. The vision is compelling.
The practice almost always fails in the same way.

### One-way flow is a dead end

Every traditional literate tool — noweb, CWEB, Org-mode's `org-babel` — works
like this:

<!-- graph: one-way flow -->
```d2

direction: right

author: Author {shape: person}
source: "source.adoc\n(literate)" {shape: document}
tool: "tangle" {shape: rectangle}
gen: "gen/code.rs\n(generated)" {shape: document}

author -> source: writes
source -> tool: feeds
tool -> gen: produces
gen -> author: "edits\n(fast path)" {style.stroke-dash: 5}
gen -> source: "(never)" {style.stroke: "#cc241d"; style.stroke-dash: 3}

```


The dead end is in that last arrow: there is no path from `gen/` back to the
source. So developers take the fast path — editing the generated file directly,
because it is what the compiler, the IDE, and the refactoring tools all see.
After a few days the source document is out of sync. After a few weeks nobody
trusts it. It becomes ceremonial documentation that nobody reads and nobody
updates.

The root cause is not laziness. It is workflow friction: every standard
developer tool operates on the generated code, not the literate source. If
using the tool correctly is harder than using it incorrectly, people use it
incorrectly.

### Weaveback's answer: close the loop

Weaveback adds bidirectionality. Every generated line carries a precise
trace back to the literate source. When you edit a generated file, `apply-back`
propagates those edits back. When an AI agent edits a chunk, the oracle
verifies the output before writing.

<!-- graph: close the loop -->
```d2

direction: right

author: Author {shape: person}
source: "source.adoc" {shape: document}
macro: "macro\nexpander" {shape: rectangle}
expanded: "expanded.adoc" {shape: document}
tangle: "tangle\n+ source map" {shape: rectangle}
gen: "gen/code.rs" {shape: document}
db: "weaveback.db\n(source map)" {shape: cylinder}

author -> source: writes
source -> macro: feeds
macro -> expanded
expanded -> tangle: feeds
tangle -> gen: writes
tangle -> db: maps every\noutput line

gen -> db: trace
db -> source: apply-back {style.stroke: "#98971a"; style.bold: true}
gen -> source: "tools can edit\ngen/ safely" {style.stroke: "#98971a"; style.stroke-dash: 5}

```


Closing the loop is the single thesis of the project. Every other design
decision follows from this.

## Why two passes?

The pipeline has two independent tools: `weaveback-macro` (macro expander) and
`weaveback-tangle` (chunk extractor). They can be used in sequence, or fused
into a single pass by the `weaveback` binary.

The split exists for three reasons.

### 1. Transparent intermediates

The output of the macro pass is still human-readable AsciiDoc. A reader who
does not know `%def` exists can read the expanded document without confusion.
Tooling that only understands AsciiDoc (Asciidoctor, docgen, a diff viewer)
works on both the input and the output of the macro pass.

If macros and chunk extraction were one step, the intermediate would either not
exist or be illegible.

### 2. Independent tracing

The source map is two-layered: generated code → expanded line → original source
line. `wb-query trace` can report either level. If expansion and tangle were
fused, a single error or tracing query would need to disentangle which pass
introduced a given line — harder to implement and harder to debug.

### 3. Separate responsibilities, separate bugs

The macro expander deals with scope, parameter binding, recursion depth, and
string substitution. The tangle pass deals with chunk assembly, indentation
preservation, file writing, and the source-map database. Bugs in one do not
contaminate the other.

In practice the combined `weaveback` binary is what you run. The split is
invisible to most users. It matters when something breaks: you can determine
in seconds whether the fault is in macro expansion or in chunk assembly.

## Why a custom macro language?

The obvious question is: why not m4, Jinja2, Handlebars, or any of a dozen
existing template languages?

### The prose constraint

Weaveback's macros live inside prose that a human reads and that Asciidoctor
renders. This changes the design requirements fundamentally. A template
language designed for generating HTML pages or configuration files optimises
for *total transformation* — the entire document is template. Weaveback needs
to be *transparent*: the macro calls are invisible to a reader who does not
know the tool.

Consider:

```text
The `greet` macro takes a name and produces a greeting.

%def(greet, name, %{Hello, %(name)!%})

Calling %greet(World) produces: "Hello, World!".
```


The prose around the macro call is still readable. An `m4` version of this
would require quoting the prose to prevent re-scanning, and the result would
not be.

### The re-scanning problem

m4 re-scans its output. Every expansion is pushed back onto the input and
processed again. This is powerful — it enables macro-generating macros — but
it creates a category of bugs that does not exist in single-pass systems:
output that accidentally looks like a macro call is expanded a second time.
Preventing this requires quoting, and quoting in m4 has famously steep
semantics.

Weaveback expands once. An `%` in the output of a macro is not re-processed.
This eliminates quoting entirely. Source locations are accurate.

### Named and positional arguments; no GPL

Weaveback macros support named arguments (`handler = list_users`) and
positional arguments in the same call. m4 has neither. Jinja and Handlebars
have both but are not designed for the prose-with-code use case.

m4 is also GPL-licensed, which may matter for commercial projects that embed
a build toolchain. Weaveback is 0BSD/MIT/Apache-2.0.

### The sigil is configurable

The `%` character is the default sigil, but it can be changed per
pass (`--sigil @`, for example). This matters for self-hosting: the
weaveback-macro source uses `^` as its sigil so that `%def` can
appear literally in examples and tests without being consumed by the expander.

## Why Python scripting?

The built-in macros handle most cases: text substitution, conditionals,
case conversion, includes. When you need to compute something — a byte
offset, a formatted hex constant, a running counter — you need a scripting
escape hatch.

The project now keeps a single escape hatch. Python-familiar teams reach for
`%pydef`, backed by
[monty](https://github.com/pydantic/monty) — a pure-Rust Python interpreter that
is compiled into the weaveback binary. There is no CPython dependency, no
virtualenv, no installed Python packages required.

It is sandboxed: no filesystem I/O, no network, no subprocess spawning.
A script macro can only transform strings. This is a deliberate constraint.

Separately, **code produced inside a scripted macro cannot be mapped back by
`wb-query trace`**. This is not a consequence of sandboxing — it is an
inherent property of dynamic code generation: the source map can point to the
`%pydef` call site, but there is no static source line inside the script body
that corresponds to a particular output character. This makes scripted macros
unsuitable as a primary structuring mechanism. They are for isolated
calculations.

The inability to trace scripted macro output is not a bug to be fixed later.
It is pressure toward the better design: express structure with chunks, use
scripting only where chunks are genuinely insufficient.

## Why apply-back?

apply-back is the answer to a workflow question: what do you do when
rust-analyzer, your IDE, or `cargo fix` edits the generated file?

The honest answer before apply-back was: manually propagate the diff back
to the literate source, which is tedious and error-prone. In practice people
did not do it. The source drifted.

apply-back automates the propagation. It reads the diff between the current
generated file and the baseline stored in `weaveback.db`, locates the
corresponding chunk lines in the literate source via the source map, and
rewrites them.

<!-- graph: apply-back flow -->
```d2

direction: down

gen: "gen/code.rs\n(developer edited)" {shape: document}
db: "weaveback.db\nbaseline + source map" {shape: cylinder}
diff: "compute diff\nvs baseline" {shape: diamond}
trace: "trace each changed\nline → src_file:src_line" {shape: rectangle}
patch: "patch literate\nsource" {shape: rectangle}
source: "source.adoc\n(updated)" {shape: document}

gen -> diff
db -> diff
diff -> trace: "changed lines"
trace -> patch
db -> patch: "source map"
patch -> source

```


apply-back handles three tracing cases that correspond to how lines end up
in generated code: literal chunk text, macro body expansions, and macro
argument substitutions. Each requires a different strategy for locating the
right source line.

Without apply-back, the bidirectionality claim is hollow. The loop is only
closed if it is closed cheaply enough that developers actually use it.

## Why SQLite for the source map?

The source map is a database, not a set of flat files. Three things drove
this choice.

### Queries that flat files cannot answer efficiently

The source map needs to answer questions like:

* What chunks does chunk `foo` directly depend on?
* What would stop compiling if I edit chunk `bar`? (Reverse deps.)
* What generated files does chunk `baz` contribute to?
* What is the full transitive dependency graph? (For `weaveback graph` DOT export.)

These are graph queries. SQLite handles them with indexed joins. A flat file
would require loading everything into memory and scanning.

### Concurrent access without contention

SQLite in WAL (Write-Ahead Logging) mode allows multiple readers concurrently
with a single writer. A `ninja -j8` build running eight weaveback passes in
parallel, the `wb-serve` HTTP server, and an MCP server attached to an
editor can all read `weaveback.db` simultaneously without blocking each other.

### Atomicity

A weaveback run writes many generated files. If it is interrupted mid-run,
the database stays consistent: the uncommitted transaction is rolled back.
Flat files would leave partial state that the next run might misinterpret.

## Why the MCP server?

The MCP server is designed for a specific workflow: an AI coding agent working
on a weaveback project.

### The problem with agentic code generation on normal repos

On a standard repository, an agent edits files and hopes the output compiles.
It cannot verify correctness before writing. It cannot ask "what was the
designer's intent for this function?" without reading unstructured README text.
After a refactoring tool renames a function in generated files, the agent has
no path to propagate that rename back to the source.

### What the MCP tools provide

<!-- graph: MCP tools -->
```d2

direction: right

agent: "Coding Agent" {shape: person}

trace: weaveback_trace {shape: rectangle}
context: weaveback_chunk_context {shape: rectangle}
fix: weaveback_apply_fix {shape: rectangle}
lsp: weaveback_lsp_definition {shape: rectangle}

agent -> trace: "which .adoc line\nproduced gen/:42?"
agent -> context: "what is the intent\nbehind chunk X?"
agent -> fix: "apply this edit\nand oracle-verify it"
agent -> lsp: "what calls this\nfunction?"

trace -> source: reads {shape: document}
context -> source: reads {shape: document}
context -> prose: reads {shape: document}
fix -> oracle: "re-expand macro;\nmatch expected output?" {shape: diamond}
oracle -> source: "write only\nif verified" {style.stroke: "#98971a"}

```


`weaveback_apply_fix` is the key primitive. It re-runs the macro expander on
the modified chunk and checks whether the output line matches what the agent
expected. If the match fails, no file is written. The agent gets a diff.
This turns a guess-and-check loop into a verify-then-commit operation.

`weaveback_chunk_context` returns the chunk body, its enclosing AsciiDoc
section (including prose paragraphs and design notes), and the bodies of all
direct dependencies. An agent reading this gets code and intent together in
one query.

### Why not just give the agent the whole source tree?

Context windows and cost. A request for chunk context returns ~200 lines of
targeted information. Reading the whole source tree returns tens of thousands
of lines, most of which are irrelevant to the current edit. The structured
query model lets the agent navigate incrementally and spend its context budget
on the right things.

## Why self-hosting?

Weaveback's own source code is written as literate AsciiDoc. The `.rs` files
are generated by running `just tangle`. This is not decoration.

Self-hosting exerts constant pressure on correctness. Every weaveback
developer runs the tool on the tool's own source every day. Bugs in tracing
are noticed immediately because the developer cannot navigate from `noweb.rs`
back to `noweb.adoc`. Bugs in apply-back are noticed immediately because
editor-assisted refactorings cannot be propagated back. Bugs in the source
map database are noticed immediately because the build fails or produces stale
output.

It also validates the claim that the tool is usable for real projects. A
literate programming tool that only works on toy examples, or that is painful
to use on non-trivial code, reveals its own limitations quickly when its own
codebase is the test case.

The cost is that there is no "we will fix tracing properly before we start
self-hosting." The capability had to be correct and usable from the point
when the codebase was converted.

## Design principles in summary

These are the principles that connect the decisions above. They are not
stated explicitly anywhere in the source, but they are consistent across
all the design choices.

*Source locations must be accurate.* Every feature that would break source
location accuracy — m4-style re-scanning, lazy evaluation, in-process
formatter transformation — was rejected. The source map is only useful if
you can trust it.

*Generated code must not be read-only.* The whole premise fails if developers
cannot use their normal tools. apply-back, the baseline detection in
SafeFileWriter, and the oracle in apply_fix all exist to make generated code
writable without consequence.

*Scripting is an escape hatch, not a primary mechanism.* The inability to
trace scripted macro output is intentional pressure toward using chunks.

*Agents are first-class users.* The MCP tools, the oracle verification, and
the chunk-context query are not add-ons. They reflect the belief that the
most important future use case for literate programming is providing coding
agents with intent and code together.

*Eating your own dogfood is not optional.* Self-hosting is the only reliable
way to keep the tool honest.

## Where to go from here

* [architecture.adoc](architecture.adoc) — the *what*: pipeline stages,
  data structures, CLI flags, source-map schema
* [weaveback-macro](../crates/weaveback-macro/src/weaveback_macro.adoc) —
  the macro expander in detail: lexer, parser, evaluator, scripting
* [weaveback-tangle](../crates/weaveback-tangle/src/weaveback_tangle.adoc) —
  chunk extraction, safe writer, database schema
* [apply-back](../crates/weaveback-api/src/apply_back.adoc) — how bidirectional
  propagation works in detail
* [m4-comparison](../docs/m4-comparison.adoc) — why not m4, point by point
