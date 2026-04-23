---
title: |-
  Public eval API
toc: left
---
# Public eval API

`eval_api.rs` provides thin wrappers around `Evaluator` that cover the common
entry points: evaluating a string, a single file, or a batch of files.  These
are the functions used by the binary, the test suite, and external callers.

## Design rationale

### Shared evaluator across multiple files

`eval_files` processes each input file through the *same* `Evaluator` instance.
This means macro definitions in file N are visible in file N+1 — the intended
behaviour when processing a collection of literate-source fragments that share
a common macro library.

`eval_files_with_config` creates a fresh evaluator; use it when isolation
between runs is required.

### `eval_string` vs `eval_string_with_defaults`

`eval_string` accepts an existing evaluator and an optional `real_path` for
source attribution (used when the string originated from a file on disk).
`eval_string_with_defaults` is the simplest entry point — a fresh evaluator,
no path, `%` as the sigil.  It is used extensively in tests.

### `canonical()` — resolving paths that do not yet exist

The output file may not exist before the first run.  `canonical` resolves the
parent directory (which must exist) and appends the file name.  This keeps the
in == out guard reliable without requiring the output file to be created first.

### Input == output guard

`eval_file` compares canonical input and output paths before evaluating.
Without this guard, `weaveback-macro src.adoc src.adoc` would silently
overwrite the source with the expanded output.

## File structure


```rust
// <[@file weaveback-macro/src/evaluator/eval_api.rs]>=
// weaveback-macro/src/evaluator/eval_api.rs
// I'd Really Rather You Didn't edit this generated file.

// <[eval api preamble]>
// <[eval string]>
// <[canonical helper]>
// <[eval file]>
// <[eval file with config]>
// <[eval files]>
// <[eval files with config]>
// <[eval string with defaults]>

// @
```


## Preamble


```rust
// <[eval api preamble]>=
// crates/weaveback-macro/src/evaluator/eval_api.rs

use std::fs;
use std::path::{Path, PathBuf};

use super::core::Evaluator;
use super::errors::{EvalError, EvalResult};
use super::state::EvalConfig;
// @
```


## `eval_string`

Parses `source` using `evaluator.parse_string` and then evaluates the AST.
If `real_path` is supplied, the evaluator's `current_file` is updated so that
`%here` and error messages reference the correct file path.


```rust
// <[eval string]>=
pub fn eval_string(
    source: &str,
    real_path: Option<&Path>,
    evaluator: &mut Evaluator,
) -> Result<String, EvalError> {
    let path_for_parsing = match real_path {
        Some(rp) => rp.to_path_buf(),
        None => PathBuf::from(format!("<string-{}>", evaluator.num_source_files())),
    };
    let ast = evaluator.parse_string(source, &path_for_parsing)?;
    if let Some(rp) = real_path {
        evaluator.set_current_file(rp.to_path_buf());
    }
    evaluator.evaluate(&ast)
}
// @
```


## `canonical` — resolve path through parent when file does not exist


```rust
// <[canonical helper]>=
/// Returns the canonical path of `p`, resolving through the parent directory
/// when `p` itself does not yet exist (common for output files).
fn canonical(p: &Path) -> std::io::Result<PathBuf> {
    if p.exists() {
        return p.canonicalize();
    }
    let parent = p.parent().unwrap_or(Path::new("."));
    let name = p.file_name().unwrap_or_default();
    Ok(parent.canonicalize()?.join(name))
}
// @
```


## `eval_file`


```rust
// <[eval file]>=
pub fn eval_file(
    input_file: &Path,
    output_file: &Path,
    evaluator: &mut Evaluator,
) -> EvalResult<()> {
    // Guard: refuse to overwrite the input file.
    let canon_in = input_file.canonicalize().map_err(|e| {
        EvalError::Runtime(format!("Cannot resolve input path {input_file:?}: {e}"))
    })?;
    let canon_out = canonical(output_file).map_err(|e| {
        EvalError::Runtime(format!("Cannot resolve output path {output_file:?}: {e}"))
    })?;
    if canon_in == canon_out {
        return Err(EvalError::Runtime(format!(
            "Output path {output_file:?} is the same as the input file — refusing to overwrite"
        )));
    }

    let content = fs::read_to_string(input_file)
        .map_err(|e| EvalError::Runtime(format!("Cannot read {input_file:?}: {e}")))?;

    let expanded = eval_string(&content, Some(input_file), evaluator)?;

    if let Some(parent) = output_file.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| EvalError::Runtime(format!("Cannot create dir {parent:?}: {e}")))?;
    }

    fs::write(output_file, expanded.as_bytes())
        .map_err(|e| EvalError::Runtime(format!("Cannot write {output_file:?}: {e}")))?;

    Ok(())
}
// @
```


## `eval_file_with_config`


```rust
// <[eval file with config]>=
pub fn eval_file_with_config(
    input_file: &Path,
    output_file: &Path,
    config: EvalConfig,
) -> EvalResult<()> {
    let mut evaluator = Evaluator::new(config);
    eval_file(input_file, output_file, &mut evaluator)
}
// @
```


## `eval_files`

Processes a batch of inputs through the same evaluator, writing each output
to `output_dir / input_filename`.


```rust
// <[eval files]>=
pub fn eval_files(
    inputs: &[PathBuf],
    output_dir: &Path,
    evaluator: &mut Evaluator,
) -> EvalResult<()> {
    fs::create_dir_all(output_dir)
        .map_err(|e| EvalError::Runtime(format!("Cannot create {output_dir:?}: {e}")))?;

    for input_path in inputs {
        let out_name = match input_path.file_name() {
            Some(n) => n.to_os_string(),
            None => "output".into(),
        };
        let out_file = output_dir.join(out_name);

        eval_file(input_path, &out_file, evaluator)?;
    }
    Ok(())
}
// @
```


## `eval_files_with_config`


```rust
// <[eval files with config]>=
pub fn eval_files_with_config(
    inputs: &[PathBuf],
    output_dir: &Path,
    config: EvalConfig,
) -> EvalResult<()> {
    let mut evaluator = Evaluator::new(config);
    eval_files(inputs, output_dir, &mut evaluator)
}
// @
```


## `eval_string_with_defaults`

The simplest entry point: fresh evaluator, default `%` sigil, no path.
Used extensively in unit tests.


```rust
// <[eval string with defaults]>=
pub fn eval_string_with_defaults(source: &str) -> EvalResult<String> {
    let mut evaluator = Evaluator::new(EvalConfig::default());
    eval_string(source, None, &mut evaluator)
}
// @
```

