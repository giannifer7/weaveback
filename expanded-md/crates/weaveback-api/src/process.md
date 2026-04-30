# Single-Pass Processing

`process.rs` contains the single-pass tangle pipeline: discover driver
files, macro-expand (or pass through raw), feed chunks to `Clip`, write
output files, snapshot sources, merge the database.

This is the function called for each `[[pass]]` entry by
`tangle::run_tangle_all` (via subprocess self-invocation), and directly
by the `wb-tangle` binary for its single-pass mode.

The implementation is split into focused literate files under
`crates/weaveback-api/src-wvb/process/`:

* `arguments.wvb` owns `ProcessError` and `SinglePassArgs`.
* `filesystem.wvb` owns input discovery and depfile writing.
* `macro-prelude.wvb` owns prelude evaluation.
* `markdown-normalize.wvb` owns expanded-document table normalization.
* `expanded-paths.wvb` owns expanded `.adoc` / `.md` output paths.
* `skip.wvb` owns incremental skip-set computation.
* `run.wvb` owns `run_single_pass` orchestration.
* `tests.wvb` owns the split generated process tests.

This file remains the assembly point for the public `process` module so the
facade layout is visible in one place.

## Assembly

```rust
// <[@file weaveback-api/src/process.rs]>=
// weaveback-api/src/process.rs
// I'd Really Rather You Didn't edit this generated file.

mod args;
mod expanded_paths;
mod fs;
mod macro_prelude;
mod markdown_normalize;
mod run;
mod skip;

pub use args::{ProcessError, SinglePassArgs};
pub use fs::{find_files, write_depfile};
pub use run::run_single_pass;
pub use skip::compute_skip_set;

#[cfg(test)]
pub(crate) use markdown_normalize::{normalize_adoc_tables_for_markdown, normalize_expanded_document};

#[cfg(test)]
mod tests;

// @
```


```rust
// <[@file weaveback-api/src/process/args.rs]>=
// weaveback-api/src/process/args.rs
// I'd Really Rather You Didn't edit this generated file.

// <[process-args]>

// @
```


```rust
// <[@file weaveback-api/src/process/fs.rs]>=
// weaveback-api/src/process/fs.rs
// I'd Really Rather You Didn't edit this generated file.

use std::path::PathBuf;

// <[process-fs]>

// @
```


```rust
// <[@file weaveback-api/src/process/macro_prelude.rs]>=
// weaveback-api/src/process/macro_prelude.rs
// I'd Really Rather You Didn't edit this generated file.

use std::path::PathBuf;

use weaveback_macro::evaluator::Evaluator;
use weaveback_macro::macro_api::process_string;

// <[process-macro-prelude]>

// @
```


```rust
// <[@file weaveback-api/src/process/markdown_normalize.rs]>=
// weaveback-api/src/process/markdown_normalize.rs
// I'd Really Rather You Didn't edit this generated file.

// <[process-markdown-ext]>
mod adoc_table;
mod explicit_table;
mod markdown_table;

pub(crate) use adoc_table::normalize_adoc_tables_for_markdown;

use explicit_table::normalize_explicit_table_blocks;

// <[process-normalize-expanded-document]>

// @
```


```rust
// <[@file weaveback-api/src/process/markdown_normalize/adoc_table.rs]>=
// weaveback-api/src/process/markdown_normalize/adoc_table.rs
// I'd Really Rather You Didn't edit this generated file.

// <[process-adoc-table-types]>
// <[process-adoc-to-markdown]>

// @
```


```rust
// <[@file weaveback-api/src/process/markdown_normalize/markdown_table.rs]>=
// weaveback-api/src/process/markdown_normalize/markdown_table.rs
// I'd Really Rather You Didn't edit this generated file.

// <[process-markdown-to-adoc]>

// @
```


```rust
// <[@file weaveback-api/src/process/markdown_normalize/explicit_table.rs]>=
// weaveback-api/src/process/markdown_normalize/explicit_table.rs
// I'd Really Rather You Didn't edit this generated file.

use super::{is_asciidoc_ext, is_markdown_ext};
use super::adoc_table::normalize_adoc_tables_for_markdown;
use super::markdown_table::normalize_markdown_table_for_asciidoc;

// <[process-explicit-table-blocks]>

// @
```


```rust
// <[@file weaveback-api/src/process/expanded_paths.rs]>=
// weaveback-api/src/process/expanded_paths.rs
// I'd Really Rather You Didn't edit this generated file.

use std::path::{Path, PathBuf};

use super::markdown_normalize::is_markdown_ext;

// <[process-expanded-paths]>

// @
```


```rust
// <[@file weaveback-api/src/process/skip.rs]>=
// weaveback-api/src/process/skip.rs
// I'd Really Rather You Didn't edit this generated file.

use std::collections::{HashMap, HashSet};

// <[process-skip]>

// @
```


```rust
// <[@file weaveback-api/src/process/run.rs]>=
// weaveback-api/src/process/run.rs
// I'd Really Rather You Didn't edit this generated file.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use weaveback_macro::evaluator::{EvalConfig, Evaluator};
use weaveback_macro::macro_api::{discover_includes_in_string, process_string};
use weaveback_tangle::{Clip, SafeFileWriter, SafeWriterConfig};
use weaveback_tangle::db::WeavebackDb;

use super::args::SinglePassArgs;
use super::expanded_paths::{expanded_source_key, write_expanded_document};
use super::fs::{find_files, write_depfile};
use super::macro_prelude::evaluate_macro_preludes;
use super::markdown_normalize::normalize_expanded_document;
use super::skip::compute_skip_set;

// <[process-run]>

// @
```

