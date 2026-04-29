# Safe Writer Tests

The parent test module keeps shared imports and exposes focused child modules for baseline writes, modification detection, path validation, and formatter behavior.





```rust
// <[@file weaveback-tangle/src/tests/safe_writer.rs]>=
// weaveback-tangle/src/tests/safe_writer.rs
// I'd Really Rather You Didn't edit this generated file.

mod basic;
mod modification;
mod paths;
mod formatters;

use super::*;
use crate::SafeWriterError;
use crate::WeavebackError;
use crate::safe_writer::{SafeFileWriter, SafeWriterConfig};
use std::{collections::HashMap, fs, io::Write, path::PathBuf, thread, time::Duration};
use tempfile::TempDir;

// @@
```

