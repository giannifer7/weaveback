---
title: |-
  weaveback-api Tangle tests
description: |-
  Literate source for crates/weaveback-api/src/tangle/tests.rs
toc: left
toclevels: 3
---
# weaveback-api Tangle tests

The tangle test root keeps shared imports and delegates command construction, config parsing, and run-path tests to focused child modules.

```rust
// <[@file weaveback-api/src/tangle/tests.rs]>=
// weaveback-api/src/tangle/tests.rs
// I'd Really Rather You Didn't edit this generated file.

mod command;
mod config;
mod run;

use super::*;
use tempfile::TempDir;

// @
```

