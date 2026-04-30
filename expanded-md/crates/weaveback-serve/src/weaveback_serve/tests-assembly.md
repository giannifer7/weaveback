# Serve Test Assembly

The generated `tests.rs` file includes focused generated test bodies so runtime
and test concerns stay physically separate.

```rust
// <[@file weaveback-serve/src/tests.rs]>=
// weaveback-serve/src/tests.rs
// I'd Really Rather You Didn't edit this generated file.

mod helpers;
use helpers::*;
mod core;
mod edge_cases;
mod editing;

// @
```


```rust
// <[@file weaveback-serve/src/tests/helpers.rs]>=
// weaveback-serve/src/tests/helpers.rs
// I'd Really Rather You Didn't edit this generated file.

// <[serve-tests-helpers]>

// @
```


```rust
// <[@file weaveback-serve/src/tests/core.rs]>=
// weaveback-serve/src/tests/core.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;

// <[serve-tests-core]>

// @
```


```rust
// <[@file weaveback-serve/src/tests/edge_cases.rs]>=
// weaveback-serve/src/tests/edge_cases.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;

// <[serve-tests-edge-cases]>

// @
```


```rust
// <[@file weaveback-serve/src/tests/editing.rs]>=
// weaveback-serve/src/tests/editing.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;

// <[serve-tests-editing]>

// @
```

