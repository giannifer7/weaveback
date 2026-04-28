# Single-Pass Tests

## Tests

The test body is generated as `process/tests.rs` and linked from
`process.rs` with `#[cfg(test)] mod tests;`.  This keeps the single-pass
pipeline implementation shorter while preserving local literate ownership.


```rust
// <[@file weaveback-api/src/process/tests.rs]>=
// weaveback-api/src/process/tests.rs
// I'd Really Rather You Didn't edit this generated file.

mod filesystem;
mod run_basic;
mod run_macros;
mod skip;
mod tables;

// @
```












The focused test bodies live in `process/tests/*.wvb` files.
