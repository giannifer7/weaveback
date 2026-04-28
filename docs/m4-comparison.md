---
title: |-
  weaveback-macro vs GNU m4
toc: left
---
# weaveback-macro vs GNU m4

GNU m4 is the oldest surviving general-purpose macro processor and the
closest conceptual relative of weaveback-macro. This document compares
them across several dimensions to help you decide which is the right tool
for a given problem, and to be honest about where each one falls short.

## Purpose and design philosophy

m4 was designed by Brian Kernighan and Dennis Ritchie in the early 1970s
as a general-purpose text-substitution layer for Unix. Its canonical use
today is as the expansion engine inside GNU Autoconf: it processes `.m4`
files at configure-generation time and never runs on the end user's
machine. The sendmail mail transfer agent also ships a large library of
m4 macros for generating its notoriously complex configuration files.
These two use cases, build-system glue and system-administration
templating, define m4's sweet spot. m4 does not know or care what the
text around a macro means; it is a pure text-replacement machine.

weaveback-macro was designed for a narrower and more specific job:
pre-processing the prose layer of a literate programming document before
the tangle pass extracts code chunks. The text around a macro *does*
matter: it is AsciiDoc or Markdown that a human will read and that
Asciidoctor will render. This shapes every design decision: the special
character is configurable so it does not collide with document markup,
macros are defined inside ordinary prose, and the tool is meant to be
transparent to a reader who does not know it exists.

In short: m4 is a universal macro language that happens to be used for
literate-adjacent tasks. weaveback-macro is a literate-programming tool
that happens to be implemented as a macro language.

## Expressiveness

### GNU m4

m4 is Turing-complete. Its expansion model, push the result back onto
the input stream and re-scan, means any computation expressible as text
transformation can be encoded in m4, including recursion, loops via
recursive macros, arithmetic with `eval`, regex substitution with `patsubst`,
character translation with `translit`, substring extraction with `substr`, and
printf-style formatting with `format`. Diversions allow output to be
buffered and reordered, which enables forward-reference tricks.
`system` and `esyscmd` let macros call arbitrary shell commands and
capture their output. Stack-based scoping with `pushdef` and `popdef` lets you
temporarily shadow a macro and restore the previous definition.

This power comes with a cost: any sufficiently large m4 program starts
to feel less like a macro layer and more like an accidental programming
language written in line noise. The GNU Autoconf project responded by
building two abstraction layers on top of raw m4, m4sugar for safe,
conventional macro utilities and m4sh for macros that generate shell code,
precisely because raw m4 is too sharp an instrument for daily use.

### weaveback-macro

weaveback-macro is intentionally less expressive at the core level.
String macros with `%def` and variables with `%set` cover the common case.
The conditional `%if(cond, then, else)` and file inclusion
with `%include` and `%import` complete the basic feature set.

Where weaveback-macro regains expressiveness is through a scripting escape
hatch. `%pydef` writes a macro body in Python-like syntax via
[monty](https://github.com/pydantic/monty), a minimal, secure Python
interpreter written in Rust, not CPython, not a subprocess. Scripts
receive the current scope's variables as strings and return the expanded
text. They run inside a sandboxed runtime with no filesystem or network
access.

The practical difference: m4 can express anything, but you have to
express it entirely in m4's opaque notation. weaveback-macro covers
most document-preprocessing needs directly, and delegates the rest to
Python when computation is genuinely needed.

| Capability | GNU m4 | weaveback-macro |
| --- | --- | --- |
| Recursive macros | ✓ (core feature) | via `%pydef` |
| Arithmetic | ✓ `eval` | via Python |
| Regex substitution | ✓ `patsubst` | via Python |
| String operations | ✓ `substr`, `index`, `len`, `translit` | via Python |
| Conditionals | ✓ `ifelse` | ✓ `%if(cond, then, else)` |
| File inclusion | ✓ `include`, `sinclude` | ✓ `%include`, `%import` |
| Shell invocation | ✓ `system`, `esyscmd` | ✗ (monty is sandboxed) |
| Output reordering | ✓ diversions | ✗ |
| Scoped redefinition | ✓ `pushdef`/`popdef` (manual) | ✓ automatic (local by default, `%export` to promote) |
| Named arguments | ✗ | ✓ |
| Scripting language body | ✗ | ✓ (Python) |
| Turing-complete | ✓ | ✓ (via scripting) |

## Learning curve

### GNU m4

m4's learning curve is famously steep. The central difficulty is
*quoting*. By default, a backtick opens a quoted string and an
apostrophe closes it, a convention that collides with English prose,
Markdown, shell scripts, and most programming languages. The alternative
`changequote([, ])` helps but introduces bracket-balancing requirements.
Quoted strings suppress expansion and are stripped of their delimiters
before the result is used, which means the number of quote layers needed
around any expression depends on how many times the text will be
re-scanned, a question that requires understanding the evaluation model
before you can answer it.

The re-scanning model itself is the second source of confusion. m4
pushes the result of every expansion back onto the input stream and
re-reads it. An unquoted comma inside a macro argument is therefore not
syntactically inert: if a macro expansion generates a comma, that comma
participates in argument delimiting. This is conceptually elegant and
practically treacherous.

The official GNU m4 manual acknowledges: _"The most common problem with
existing macros is improper quotation."_ External guides add: _"Quoting
can be cantankerous on occasion in m4."_ Many experienced developers
avoid writing nontrivial m4 from scratch and instead cargo-cult existing
patterns, which is not a good sign.

### weaveback-macro

weaveback-macro's model is simpler because it does not re-scan. A macro
expansion produces text that is output as-is; it does not feed back into
the macro expander. This eliminates the entire category of quoting bugs
that makes m4 difficult.

Arguments follow Python-style calling conventions: positional arguments
bind left-to-right, named arguments bind by name and may appear in any
order after positional arguments. Missing parameters bind to the empty
string. Extra positional arguments are silently ignored. This is
familiar to anyone who has written Python.

The special character (`%` by default, configurable) only triggers macro
syntax when followed by a known macro name or opening parenthesis. A
lone `%` in prose, as in "the failure rate was 4%", passes through
untouched. The escape for a literal special is doubling (`%%`), the
same convention used by `printf`.

The main learning investment is understanding which constructs exist
(`%def`, `%set`, `%if(cond,then,else)`, `%include`,
`%pydef`) and the distinction between defining a macro (`%def`) and
setting a variable (`%set`). A developer familiar with any template language
will reach productive use within an hour.

## Quoting and scoping

m4 provides explicit scope manipulation via `pushdef` and `popdef`, which
requires manual discipline: the programmer must balance every `pushdef`
with a `popdef`.

weaveback-macro uses a scope stack that is managed automatically.
`%def` inside a macro body creates a definition local to that
invocation; it is discarded when the call returns. To promote a
definition to the caller's scope, `%export` is used explicitly.
`%include` evaluates the included file in the caller's current scope,
equivalent to inlining the text at the call site. When called at the
top level, the normal case, definitions land at the top level; when
called inside a macro body they are local to that invocation.
`%export` promotes a name one level up to the parent scope.
This model gives the locality of `pushdef` and `popdef` without
requiring the programmer to balance them manually.

m4 uses `ifdef` to test whether a name is defined. weaveback-macro uses
`%if(cond, then, else)` where the condition is truthy if it expands to a
non-empty string; Python handles any condition more complex than
that.

## Error messages and debugging

### GNU m4

m4's error messages are terse and often point to the wrong location,
because by the time the error is detected the input has been transformed
by multiple expansion passes. Debugging uses `traceon`, `traceoff`, and
the `-d` flag, which prints a trace of each macro call and its expansion
with depth indicators. The output is useful once you understand it, but
the format is unstructured text that requires visual parsing. The manual
warns that m4 cannot detect infinite rescanning loops because the problem
is undecidable.

### weaveback-macro

Error messages include the source file and line number from the original
document before expansion. Because there is no re-scanning, the error
site in the message corresponds to the site a human sees in the source.
Recursion depth is tracked and capped at a configurable limit, 100 by
default, so infinite-recursion mistakes produce a clear error rather
than a stack overflow. There is no interactive debugger, but the
`--dump-expanded` flag on the combined `weaveback` binary prints the
fully expanded text to stderr before the tangle pass, which is usually
sufficient for diagnosis.

## Speed and scaling

m4 is written in C and processes tens of megabytes of macro-heavy input
per second. Its `--freeze-state` and `--reload-state` mechanism lets you
pre-compute a common initialization state and amortise it across many
runs. Its `changeword` feature, which allows redefining word syntax,
slows it down considerably and is rarely used. m4 is stateless between
runs: no database, no carry-over cost.

weaveback-macro itself is fast: Rust and a `memchr` single-byte scan.
The macro pass and the tangle pass are not the bottleneck for large projects.
The bottleneck is `weaveback.db`.

### Database scaling

weaveback.db has two tables that store full file content:

* `gen_baselines`, a byte-for-byte copy of every generated file, used
  to detect external edits between runs. Size tracks the total size of
  your `gen/` directory.
* `src_snapshots`, a byte-for-byte copy of every source file read.
  Size tracks the total size of your literate sources.

`noweb_map` adds roughly 150 bytes per output line.

Rough projections:

| Project scale | Source | Generated | Estimated weaveback.db |
| --- | --- | --- | --- |
| Small | 500 KB | 300 KB | ~1.5 MB |
| Medium | 5 MB | 3 MB | ~12 MB |
| Large | 30 MB | 15 MB | ~80 MB |
| Very large | 300 MB | 150 MB | ~700 MB |

These are proportional, not fixed costs. The database grows linearly
with project size. `80 MB` is unremarkable; `700 MB` starts to be a
consideration for disk space and backup.

### Memory during a run

The more immediate concern for large projects is RAM. The temp database
that accumulates during a tangle run uses SQLite in-memory storage
with `open_in_memory`. At merge time the entire contents of that database,
all of `gen_baselines`, `src_snapshots`, `noweb_map`, and `chunk_deps`,
lives simultaneously in RAM before being flushed to disk in one
`BEGIN IMMEDIATE` transaction. Add the `ChunkStore`, all parsed chunks,
and the evaluator state and the per-run RAM footprint is roughly twice the
final database size.

For small and medium projects this is invisible. For a very large
project a single weaveback run could require 1 to 2 GB of RAM just for the
database merge, before accounting for the rest of the process.

m4 has no equivalent cost: it writes output as it expands and keeps
nothing after the run.

### Summary

For the overwhelming majority of literate programming projects, a few
dozen source files and tens of thousands of lines of generated output,
weaveback's database overhead is acceptable if the source mapping and
apply-back guarantees matter. If you want pure text expansion with
minimum runtime state and no provenance model, m4 remains lighter.
