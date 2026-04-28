---
title: |-
  Semantic Annotation Layer
toc: left
---
# Semantic Annotation Layer

Here’s a design that fits Weaveback much better than changing prose language.

## The goal

Keep prose natural, but make the most important parts of intent **structured enough** that:

* humans can read them easily
* agents can query them reliably
* you can trace them to code/chunks/files
* contradictions can be detected

So the idea is:

**natural prose for narrative**
+
**small intent annotations for semantics**

---


## Design principles

The annotation layer should be:

* **tiny**
* **line-oriented**
* **pleasant in plain text**
* **stable under editing**
* **easy to parse without heroics**
* **optional**, not mandatory everywhere

It should feel closer to:

```text
@intent avoid global mutable state
@because cross-pass behavior becomes hard to reason about
```


than to YAML soup or a theorem prover.

---


## A minimal syntax

I would start with a very small family of directives.

### Core directives

```text
@intent ...
@because ...
@invariant ...
@assumption ...
@guarantee ...
@tradeoff ...
@risk ...
@depends ...
@affects ...
@example ...
@status ...
@question ...
```


These are deliberately human-readable.

Example:

```text
The parser uses a push-down state machine rather than a general parser
generator.

@intent keep parser behavior explicit and debuggable
@because grammar is small but edge cases are subtle
@tradeoff more handwritten code for easier tracing
@risk state explosion if too many ad hoc exceptions accumulate
```


That already gives you a lot.

---


## Why this shape works

It preserves the prose as the main thing. The structured lines are just anchors.

Humans read:

[quote]
____
The parser uses a push-down state machine...
____

Agents read:

* intent = keep parser behavior explicit and debuggable
* because = grammar is small but edge cases are subtle
* tradeoff = more handwritten code for easier tracing
* risk = state explosion if too many ad hoc exceptions accumulate

That is enough to support:

* intent-aware search
* design review
* consistency checks
* targeted agent prompts

---


## Two levels: freeform and normalized

You probably want two layers eventually.

### Level 1: freeform

Very easy to write:

```text
@intent avoid hidden coupling
@because reverse mapping must stay explainable
```


### Level 2: normalized

For places where you want more machine leverage:

```text
@intent avoid(hidden_coupling)
@because explainable(reverse_mapping)
```


My advice: start with **freeform**, and only normalize where repeated patterns emerge. Do not over-formalize too early.

---


## Scoping model

This matters a lot.

Annotations should attach to the **nearest semantic unit**:

* a section
* a chunk definition
* a file chunk
* maybe a prose-only code block

A simple rule:

[quote]
____
annotations belong to the paragraph/block/chunk they immediately follow
____

Example:

```adoc
== Why the safe writer checks baselines

External edits are expected. Silent overwrite would destroy trust in the
system.

@intent preserve user trust during reverse writes
@invariant generated files modified externally must not be overwritten silently
@because apply-back must be auditable

// <[safe-writer-check]>=
...
// @
```


These annotations apply to the section above unless a chunk immediately follows and you define that adjacency binds to the chunk. You need one clear rule and to keep it simple.

I would actually make it explicit.

---


## Better: explicit attachment

Use one optional attachment directive:

```text
@about section
@about chunk safe-writer-check
@about file crates/weaveback-noweb/src/safe_writer.rs
```


Example:

```text
@about chunk safe-writer-check
@intent preserve user trust during reverse writes
@invariant generated files modified externally must not be overwritten silently
@because apply-back must be auditable
```


This removes ambiguity and is easier for tooling.

---


## Recommended concrete grammar

I would use this:

```text
@about <kind> <target?>
@intent <text>
@because <text>
@invariant <text>
@assumption <text>
@guarantee <text>
@tradeoff <text>
@risk <text>
@depends <text>
@affects <text>
@status <text>
@question <text>
```


Where `<kind>` is one of:

* `section`
* `chunk`
* `file`
* `block`
* `function`
* `type`

Examples:

```text
@about chunk parser-block-state
@intent terminate only on matching block close
@invariant block state must never consume a mismatched closing tag
@because mismatches must be reported, not normalized away
```


```text
@about file crates/weaveback-noweb/src/db.rs
@intent centralize persistence logic
@tradeoff tighter coupling to sqlite for simpler trace queries
@risk schema drift across tool versions
```


---


## Multiline values

You will need them.

Keep the single-line form for most cases, but allow an indented block:

```text
@about chunk apply-back
@because
  Reverse application is only trustworthy if the user can inspect both
  the source span and the generated baseline.
  A black-box overwrite would defeat the point of literate traceability.
```


Parsing rule:

* `@key value` => single-line
* `@key` followed by indented lines => multiline block

This is easy to parse and nice to write.

---


## Cross references

You will want relations between concepts.

Add a tiny reference convention:

```text
@see chunk parser-block-state
@see file crates/weaveback-macro/src/parser/mod.rs
@see concept reverse-mapping
```


and maybe:

```text
@conflicts chunk old-safe-writer
@implements invariant no-silent-overwrite
```


But I would not start with too many relation types. Maybe just:

* `@see`
* `@depends`
* `@affects`

---


## Suggested first version

If I were trying to keep this sane, version 1 would include only:

```text
@about
@intent
@because
@invariant
@tradeoff
@risk
@depends
@see
@status
@question
```


That is enough to be useful without becoming a second programming language.

---


## Example inside a literate source

Here’s a realistic fragment.

```adoc
== Safe reverse write policy

The reverse write path is where the tool either becomes trustworthy or
dangerous. If a user edits generated output by hand, we must detect that
fact before applying changes back to the literate source.

@about section
@intent preserve trust in apply-back
@because reverse synchronization must be explainable
@invariant externally modified generated files must not be overwritten silently
@tradeoff extra baseline bookkeeping for safer reverse writes
@risk false positives may annoy users, but false negatives would be worse

// <[safe-writer-check]>=
fn verify_baseline(...) -> Result<(), SafeWriterError> {
    ...
}
// @

@about chunk safe-writer-check
@intent reject unsafe reverse propagation
@depends baseline database entry for generated path
@guarantee modified_externally is reported before source mutation
@see concept apply-back-safety
```


This is readable even without tooling.

---


## How it maps to SQLite

This is where it gets interesting for Weaveback.

You already care about traceability. So treat annotations as first-class mapped entities.

I’d add tables roughly like these.

### `doc_node`

Represents semantic anchors in the literate source.

Columns:

* `id`
* `source_path`
* `kind` — section, chunk, file, block, function, concept
* `name`
* `start_line`
* `end_line`

### `annotation`

Stores structured intent statements.

Columns:

* `id`
* `doc_node_id`
* `key` — intent, because, invariant, tradeoff, risk, ...
* `value`
* `ordinal`
* `source_path`
* `line_start`
* `line_end`

### `relation`

Optional normalized edges.

Columns:

* `id`
* `from_doc_node_id`
* `rel_type` — see, depends, affects, conflicts
* `to_kind`
* `to_target`

### `generated_span_to_doc_node`

A bridge from generated code spans to semantic source nodes.

Columns:

* `generated_file`
* `generated_line_start`
* `generated_line_end`
* `doc_node_id`

You may already have enough of this indirectly via your noweb/source map.

The crucial capability is:

[quote]
____
for any generated line, find not only the source line, but also the
nearest intent/invariant/tradeoff attached to that source region
____

That is gold.

---


## Queries this enables

Once stored, you can do genuinely useful things.

### 1. “Why does this line exist?”

Given a generated line:

* map to source chunk
* fetch nearby `@intent`, `@because`, `@tradeoff`

### 2. “What invariants apply to this function?”

Find source node for chunk/file/function, then attached invariants.

### 3. “Show all risks in this module”

Search annotations where key = `risk` and source path matches module.

### 4. “Find undocumented chunks”

Chunks with no attached `intent` or `because`.

### 5. “Find stale intent”

Chunks whose code changed significantly since the annotation changed.

That last one is especially valuable.

---


## Agent workflows

This is where your “project-aware AI” idea gets much stronger.

Instead of throwing raw source at an agent, you can give it:

* the chunk
* the surrounding prose
* the attached annotations
* neighboring related chunks
* recent diffs

For example:

[quote]
____
Review `safe-writer-check`. Preserve invariants attached to this chunk.
Pay attention to stated tradeoffs and risks.
____

That is far better than generic RAG over arbitrary chunks.

---


## Consistency checks you could build

Not formal proof, just useful linting.

### Documentation lint

Warn if:

* chunk has no `@intent`
* invariant exists but no code chunk nearby
* `@question` remains unresolved
* `@status deprecated` but still widely referenced

### Drift lint

Warn if:

* code changed but annotation did not
* annotation mentions caching, but cache code disappeared
* annotation says “never mutates” but diff adds mutation-looking ops

These checks will be fuzzy, but still useful.

---


## Why this is better than just prose

Normal prose is rich but hard to query.

This layer gives you a small semantic index without killing the human writing experience.

It’s basically:

* lighter than Lojban
* lighter than a DSL
* much more useful than plain comments
* aligned with literate programming

---


## Why this is better than YAML blocks

You could write:

```yaml
intent:
  - preserve trust
invariants:
  - no silent overwrite
```


But YAML has a bad habit: it visually dominates the prose and encourages schema creep.

Inline `@intent` lines feel more like annotations and less like config.

That matters.

---


## A possible future extension

Later, if you want more rigor, you can add stable IDs.

```text
@id apply-back-safety
@intent preserve trust in reverse synchronization
@invariant no silent overwrite of externally modified generated files
```


Then other places can reference:

```text
@see concept apply-back-safety
@implements invariant apply-back-safety
```


This would let you build a graph of design concepts across files.

But I would not begin there unless you already feel the pain.

---


## My recommended adoption path

### Phase 1

Add only:

* `@about`
* `@intent`
* `@because`
* `@invariant`
* `@tradeoff`
* `@risk`

Do this by hand in a few important files.

### Phase 2

Parse them and store them in SQLite.

### Phase 3

Teach `where` or `trace` to show nearby annotations.

For example:

```bash
wb-query where gen/src/parser.rs:148
```


Could output something like:

```text
Generated: gen/src/parser.rs:148
Source: docs/parser.adoc:212
Chunk: parser-block-state

Intent:
  keep parser behavior explicit and debuggable

Invariant:
  block state must never consume a mismatched closing tag

Tradeoff:
  more handwritten code for easier tracing
```


That would be excellent.

### Phase 4

Add linting for missing/stale intent.

---


## My honest recommendation on language choice

Use **English** for these annotations, even if some prose is in Italian.

Why:

* closer to code and tooling vocabulary
* easier for agents
* easier to normalize later
* less friction when terms become identifiers or concept names

You can still keep surrounding prose in whichever language helps you think best.

A mixed model is fine:

* narrative in Italian or English
* structured annotations in short technical English

---


## Bottom line

What you want is not a better natural language.

What you want is a **thin semantic layer** over natural language.

This gives you:

* intent
* invariants
* tradeoffs
* risks
* traceability
* better agent context

without turning your docs into a logic experiment.

---


## Multiline Formatting Guidelines

Good question—this is one of those details that can make or break the ergonomics.

You want something that:

* feels natural in both **AsciiDoc (.adoc)** and **Markdown (.md)**
* doesn’t fight the renderer
* is trivial to parse (your lexer will thank you)
* doesn’t visually dominate the prose

---


### Core rule (keep it consistent everywhere)

Use **indent-based multiline blocks**, like this:

```text
@because
  Reverse application is only trustworthy if the user can inspect both
  the source span and the generated baseline.
  A black-box overwrite would defeat the point of literate traceability.
```


#### Parsing rule

* `@key VALUE` → single-line
* `@key` + **indented lines** → multiline
* block ends when:

  * indentation stops, or
  * another `@key` starts

This works identically in `.adoc` and `.md`.

---


### In Markdown (.md)

#### Recommended style

```markdown
We avoid caching here.

@because
  Cached values would become stale across passes.
  This would break reverse mapping assumptions.

@tradeoff
  Slight performance loss in exchange for correctness.
```


#### Why this works

* Markdown treats this as plain text → no rendering issues
* indentation is visually clear
* no need for code blocks (which would be noisy)

#### Avoid this (tempting but bad)

```markdown
@because """
Reverse application is only trustworthy...
"""
```


Why:

* introduces quoting rules
* harder to parse
* visually heavier

#### Also avoid fenced blocks

```markdown
@because
----

[source,markdown]
----
Reverse application...
```


Why:

* breaks flow
* looks like code, not intent
* harder to associate structurally

### In AsciiDoc (.adoc)

AsciiDoc is trickier because it has richer syntax—but the same approach still works.

#### Recommended (same as Markdown)

```adoc
We avoid caching here.

@because
  Cached values would become stale across passes.
  This would break reverse mapping assumptions.

@tradeoff
  Slight performance loss in exchange for correctness.
```


AsciiDoc will render this as a paragraph with line breaks—fine.

#### Optional: make it visually distinct (if you want)

You *can* wrap annotations in a block, but I wouldn’t require it:

```adoc
[.intent]
--
@because
  Reverse application is only trustworthy if the user can inspect both
  the source span and the generated baseline.
--
```


Pros:

* visually grouped

Cons:

* more typing
* adds noise
* parser now needs to understand block boundaries

👉 I would **not require this** in v1.

---


### Edge cases you should define

#### 1. Blank lines inside multiline blocks

Allow them:

```text
@because
  First reason.

  Second reason.
```


Rule:

* blank line is allowed **if still indented**

#### 2. Mixed single + multi

```text
@intent preserve trust
@because
  Reverse application must be explainable.
```


Perfectly fine.

#### 3. Multiple annotations of same type

```text
@risk false positives may annoy users
@risk false negatives would be catastrophic
```


Keep them as separate entries (simpler than lists).

### Why indentation beats everything else

Alternatives you might consider:

<!-- weaveback-table:adoc -->

| Approach | Problem |
| --- | --- |
| YAML blocks | too heavy, dominates prose |
| JSON | unreadable in docs |
| Triple quotes | new syntax rules |
| Code blocks | wrong semantic signal |
| Lojban 😄 | overkill |

<!-- /weaveback-table -->

Indentation gives you:

* zero new syntax
* familiar mental model (Python, Markdown lists, etc.)
* trivial parsing (just count spaces)

---


### Minimal parser sketch (fits your style)

Given your lexer/DPDA mindset, this is easy:

1. detect `@key`
2. if same line has content → single-line
3. else:
   * capture following lines while:
     ** line is empty **or**
     ** line starts with ≥1 space or tab
   * strip leading indentation

You don’t need a full parser—this is almost regex + state.

---


### Example mixing with chunks (your real use case)

#### Markdown

```markdown
## Safe reverse write

External edits must not be overwritten silently.

@intent preserve user trust
@because
  Users may edit generated files manually.
  Silent overwrite would destroy confidence in the tool.

```rust
// <[safe-writer-check]>=
fn verify_baseline(...) { ... }
// @
```

@about chunk safe-writer-check
@invariant
If generated file differs from baseline,
reverse write must abort with explicit error.
```


#### AsciiDoc

```adoc
== Safe reverse write

External edits must not be overwritten silently.

@intent preserve user trust
@because
  Users may edit generated files manually.
  Silent overwrite would destroy confidence in the tool.

[source,rust]
----
// <[safe-writer-check]>=
fn verify_baseline(...) { ... }
// @
----

@about chunk safe-writer-check
@invariant
  If generated file differs from baseline,
  reverse write must abort with explicit error.
```


### Small refinement (optional but nice)

Allow **inline continuation with `\`** for tight blocks:

```text
@because Reverse application must be explainable \
         and traceable to the original source.
```


But I’d treat this as sugar, not core.

---


### Final recommendation

Use exactly this:

```text
@key value              # single line
@key                    # multiline
  indented text
  more text
```


Same in `.md` and `.adoc`.

No extra syntax.

No blocks.

No quoting.
