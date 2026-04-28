---
title: |-
  Macro Language Tightening Plan
toc: left
---
# Macro Language Tightening Plan

This note captures the next cleanup pass for the macro language after the
recent `%def` / `%redef` split and the introduction of verbatim blocks.

The goal is not to make the language more powerful. The goal is to make it
more locally predictable for both humans and coding agents.

## Main objective

The language is now much more legible than before, but a few semantic pressure
points still weaken local reasoning.

The next phase should tighten invariants around:

* stable vs rebindable names
* effectful argument evaluation
* silent-empty behavior
* operational builtins that do not fit the conceptual core
* namespace/import behavior that is still more magical than ideal

## Priority order

### 1. Make `%def` truly constant

This should become a hard semantic promise, not only a documentation claim.

Target invariant:

* `%def(name, ...)` introduces an immutable macro binding in the current frame
* `%redef(name, ...)` introduces or updates a rebindable binding in the current frame
* `%redef` may not replace a `%def` binding
* duplicate `%def` in the same frame is an error

Why:

* this restores genuine name stability
* it makes `%redef` the only visually marked rebinding path
* it improves prose reliability and agent reasoning

Concrete work:

* evaluator enforcement
* tests for `%def` / `%redef` replacement rules
* docs/examples kept aligned

### 2. Remove `%set` from argument position

The biggest remaining semantic hazard is eager argument evaluation in caller
scope combined with `%set`.

Problem shape:

* arguments are effectful programs, not just values
* `%set` in argument position mutates caller state before the callee frame exists
* harmless-looking calls can hide mutations

Implemented direction:

* keep `%set`
* do not redesign the whole evaluation model immediately
* reject `%set` in argument position with `InvalidUsage`

Why this is the right first cut:

* it removes the worst hidden caller-scope mutation
* it keeps eager argument evaluation for ordinary value-producing expressions
* it tightens the language without adding new syntax or mode flags

### 3. Add a strictness surface for silent-empty behavior

The language still collapses:

* intentionally absent
* missing by mistake

into the same empty-string result too often.

Highest-value cases:

* undefined variable lookup `%(name)`
* unbound formal parameters

Recommended direction:

* strict vars by default
* strict params by default
* explicit opt-outs:
  `--no-strict-vars` / `--no-strict-params`

Helpful companion builtins:

* `%defined(name)` or `%is_set(name)`
* `%default(x, fallback)`
* maybe `%require(x, msg)`

The point is not types. The point is making intentional emptiness explicit.

### 4. Demote `%here` from the conceptual core

`%here` is operationally useful, but it does not belong comfortably in the
core expression model.

Why it is different:

* rewrites source instead of producing ordinary output
* one-file preflight invariant: multiple live `%here` calls are rejected
* sets global early-exit state
* mutates future semantics by escaping itself

Near-term work:

* move it further out of the conceptual core in docs
* done: hard-error on multiple live `%here` calls in the same file

Possible longer-term direction:

* treat it as a dedicated workflow primitive instead of a normal builtin

### 5. Remove `%importas`

`%importas` was added speculatively and did not pay for its complexity.

Reasons to remove it:

* no real project workflow currently depends on it
* its semantics were more magical than the value justified
* it invited namespace expectations it did not cleanly satisfy

Direction:

* keep `%include` and `%import`
* keep `%alias` as the explicit composition tool
* if a real namespace need appears later, design it from that concrete use case

### 6. Keep the block model explicit

The distinction is good, but easy to misread if not reinforced:

* `%{ ... %}` = quoted argument block, still macro-active
* `%[ ... %]` = verbatim block, opaque to macro parsing

Near-term work:

* done: keep both names visible in docs
* done: use examples that show the contrast directly
* continue avoiding prose that treats both simply as generic “blocks”

## Coding-model implications

For agent-facing guidance, the preferred profile should become:

Preferred:

* `%def` for stable names
* `%redef` only for deliberate phase-oriented rebinding
* `%[ ... %]` for literal embedded regions
* `%{ ... %}` only when you need a single macro-active argument
* `%alias(..., k=v)` as the only capture mechanism

Acceptable but advanced:

* `%pydef`
* `%export`

Red flags:

* `%set` in argument position
* relying on undefined variables becoming empty
* relying on missing parameters becoming empty
* `%here` in generated code

## Review questions

Each tightening step should be judged against a small set of blunt questions:

* Did this improve local reasoning?
* Did this make names more stable?
* Did this reduce hidden side effects?
* Did this reduce ambiguous “empty means anything” behavior?
* Did this keep the language small?

If the answer is no, the change is probably moving in the wrong direction.
