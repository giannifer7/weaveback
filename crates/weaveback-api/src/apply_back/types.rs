// weaveback-api/src/apply_back/types.rs
// I'd Really Rather You Didn't edit this generated file.

use weaveback_macro::evaluator::{EvalConfig, Evaluator};
use weaveback_macro::macro_api::process_string;
use weaveback_tangle::db::{WeavebackDb, DbError, NowebMapEntry};
use weaveback_tangle::lookup::find_best_noweb_entry;
use weaveback_core::PathResolver;
use weaveback_lsp::LspClient;
use regex::Regex;
use similar::TextDiff;
use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;

use crate::lookup;

#[derive(Debug)]
pub enum ApplyBackError {
    Db(DbError),
    Io(std::io::Error),
    Lookup(lookup::LookupError),
}

impl From<DbError> for ApplyBackError {
    fn from(e: DbError) -> Self { ApplyBackError::Db(e) }
}
impl From<std::io::Error> for ApplyBackError {
    fn from(e: std::io::Error) -> Self { ApplyBackError::Io(e) }
}
impl From<lookup::LookupError> for ApplyBackError {
    fn from(e: lookup::LookupError) -> Self { ApplyBackError::Lookup(e) }
}

impl std::fmt::Display for ApplyBackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApplyBackError::Db(e)     => write!(f, "database error: {e}"),
            ApplyBackError::Io(e)     => write!(f, "I/O error: {e}"),
            ApplyBackError::Lookup(e) => write!(f, "trace lookup error: {e:?}"),
        }
    }
}

