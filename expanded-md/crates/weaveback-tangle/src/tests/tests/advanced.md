# Advanced Tangle Tests

The advanced test module is intentionally a thin map. Each child module owns one behavior family so regressions can be reviewed without scanning a monolithic generated file.





```rust
// <[@file weaveback-tangle/src/tests/advanced.rs]>=
// weaveback-tangle/src/tests/advanced.rs
// I'd Really Rather You Didn't edit this generated file.

mod chunks;
mod replace;
mod modifiers;
mod paths;
mod unused;
mod tangle_check;
mod syntax;
mod outputs;
mod strict_write;

// @@
```

