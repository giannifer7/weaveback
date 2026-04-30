// weaveback-api/src/coverage/tests_coverage.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;
use rusqlite;
use serde_json::json;
use tempfile::tempdir;
use weaveback_tangle::db::{Confidence, NowebMapEntry, WeavebackDb};

mod helpers;
use helpers::*;
mod locations;
mod cargo;
use cargo::CARGO_TEST_MUTEX;
mod lcov_summary;
mod summary_output;
mod location_errors;
mod cargo_extra;

