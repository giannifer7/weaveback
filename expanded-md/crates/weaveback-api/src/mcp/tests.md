# MCP Tests

## Tests

The test body is generated as `mcp/tests.rs` and linked from `mcp.rs`
with `#[cfg(test)] mod tests;`.  This keeps the server implementation file
shorter while preserving local literate ownership of the tests.


```rust
// <[@file weaveback-api/src/mcp/tests.rs]>=
// weaveback-api/src/mcp/tests.rs
// I'd Really Rather You Didn't edit this generated file.

mod data;
mod errors;
mod helpers;
mod lsp;
mod protocol;
mod smoke;

// @
```


The focused test bodies live in `mcp/tests/*.wvb` files.
