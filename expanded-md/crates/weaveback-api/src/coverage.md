# Coverage & Attribution Analysis

Coverage and attribution analysis: maps generated-file locations to literate
sources, annotates cargo output, processes LCOV coverage data, and provides
text/stdin attribution.

## Implementation Sources

Coverage is split by concern under `crates/weaveback-api/src-wvb/coverage/`:

* `impl-error.wvb` defines imports and the shared error type.
* `impl-locations.wvb` parses/scans generated locations and runs direct trace commands.
* `impl-lcov.wvb` parses LCOV and builds coverage summaries.
* `impl-cargo.wvb` attributes cargo diagnostics and wraps cargo command execution.
* `impl-text.wvb` attributes text input and delegates graph/search/tag/trace commands.

## Tests

The generated coverage tests remain in `crates/weaveback-api/src/coverage/`, but
their bodies are split by concern:

* `tests-helpers.wvb` contains shared imports and workspace helpers.
* `tests-locations.wvb` covers generated-location parsing/scanning and command wrappers.
* `tests-cargo.wvb` covers cargo diagnostic attribution.
* `tests-lcov-summary.wvb` covers LCOV parsing and source coverage grouping.
* `tests-summary-output.wvb` covers rendered summary and attribution JSON output.
* `tests-location-errors.wvb` covers generated-location and DB error cases.
* `tests-cargo-extra.wvb` covers remaining cargo/text attribution regressions.

## Assembly

`coverage.rs` remains the public module facade and includes focused generated
files in one module to preserve private visibility while reducing file size.

```rust
// <[@file weaveback-api/src/coverage.rs]>=
// weaveback-api/src/coverage.rs
// I'd Really Rather You Didn't edit this generated file.

include!("coverage/error.rs");
include!("coverage/locations.rs");
include!("coverage/lcov.rs");
include!("coverage/cargo.rs");
include!("coverage/text.rs");

#[cfg(test)]
mod tests_coverage;

// @
```


```rust
// <[@file weaveback-api/src/coverage/error.rs]>=
// weaveback-api/src/coverage/error.rs
// I'd Really Rather You Didn't edit this generated file.

// <[coverage error]>

// @
```


```rust
// <[@file weaveback-api/src/coverage/locations.rs]>=
// weaveback-api/src/coverage/locations.rs
// I'd Really Rather You Didn't edit this generated file.

// <[coverage-locations]>

// @
```


```rust
// <[@file weaveback-api/src/coverage/lcov.rs]>=
// weaveback-api/src/coverage/lcov.rs
// I'd Really Rather You Didn't edit this generated file.

include!("lcov/parse.rs");
include!("lcov/summary.rs");
include!("lcov/output.rs");
include!("lcov/run.rs");

// @
```


```rust
// <[@file weaveback-api/src/coverage/cargo.rs]>=
// weaveback-api/src/coverage/cargo.rs
// I'd Really Rather You Didn't edit this generated file.

// <[coverage-cargo-attribution]>

// @
```


```rust
// <[@file weaveback-api/src/coverage/text.rs]>=
// weaveback-api/src/coverage/text.rs
// I'd Really Rather You Didn't edit this generated file.

// <[coverage-text-query]>

// @
```

