---
title: |-
  Macro Language Critique Prep
toc: left
---
# Macro Language Critique Prep

This note is meant to accompany
[macro-language.adoc](macro-language.adoc) when asking for outside review.
It states what changed recently, what is already intentional, and what still
deserves critique.

## Recent changes

The language surface was simplified in two important ways:

* `%def` and `%redef` now have distinct roles:
  - `%def` creates a constant macro binding in the current frame
  - `%redef` creates or replaces a rebindable macro binding in the current frame
* opaque verbatim blocks were added:
  - `%[ ... %]`
  - `%tag[ ... %tag]`

The verbatim blocks are now the general mechanism for “treat this region
literally”. That removed the need for a separate `%pydef_raw` variant.

The intended consequence is a smaller and more explicit model:

* constant names stay constant
* deliberate rebinding is marked with `%redef`
* literal regions are marked with verbatim syntax instead of builtin variants

## What is intentionally settled

These points are not accidents in the current design:

* arguments evaluate eagerly in caller scope
* `%if` is the only lazy branch-forming builtin
* `%alias(..., k=v)` is the only capture mechanism
* `%set` remains distinct from macro definition forms
* `%pydef` is the only scripting escape hatch
* verbatim blocks are lexer-level opaque, not evaluator-level conventions

## What should be critiqued hard

The most useful external criticism is on these questions:

* Is the `%def` / `%redef` split the right semantic boundary?
* Are verbatim blocks the right replacement for raw-script variants?
* Is the distinction between macro-aware `%{...%}` blocks and opaque `%[...%]`
  blocks obvious enough from the syntax alone?
* Is `%set` still the right variable primitive, or should the variable model be
  tightened further?
* Are the remaining silent-empty behaviours still too permissive?
* Is the scripting escape hatch too implicit, or appropriately minimal now?

## Known weak spots in the current spec

These are real candidates for critique, not oversights in the review request:

* `%here` remains operationally odd even though the docs are clearer now
* discovery mode is still a flag on config rather than a separate API surface

## Suggested reviewer posture

The most valuable review is not:

* “could this have more features?”
* “could this be more clever?”

The valuable review is:

* is the language now smaller and more legible?
* are the invariants explicit enough for human and agent authors?
* are the remaining exceptions defensible?
* what still feels surprising after the recent simplifications?
