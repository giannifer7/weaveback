# weaveback-api

Shared business logic for the weaveback toolchain.

No HTTP, no LLM, no subprocess calls — pure query and analysis
functions that can be called from any binary in the workspace,
from Python via PyO3, and from MCP tool handlers.

## Modules

```rust
// <[weaveback-api-lib]>=
pub mod apply_back;
pub mod coverage;
pub mod lint;
pub mod lsp_runner;
pub mod lookup;
pub mod mcp;
pub mod process;
pub mod query;
pub mod semantic;
pub mod tag;
pub mod tangle;
// @
```


## Assembly

```rust
// <[@file weaveback-api/src/lib.rs]>=
// weaveback-api/src/lib.rs
// I'd Really Rather You Didn't edit this generated file.

// <[weaveback-api-lib]>

// @
```

