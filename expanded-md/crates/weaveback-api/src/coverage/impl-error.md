# Coverage Error Type

Shared imports and the public coverage API error type.

## Error Type

```rust
// <[coverage error]>=
use weaveback_tangle::WeavebackError;
use weaveback_core::PathResolver;
use weaveback_macro::evaluator::EvalConfig;
use weaveback_agent_core::{Workspace as AgentWorkspace, WorkspaceConfig as AgentWorkspaceConfig};
use weaveback_tangle::lookup::distinctive_suffix_candidates;
use serde_json::json;
use std::collections::HashSet;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use crate::lookup;

#[derive(Debug, thiserror::Error)]
pub enum CoverageApiError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Noweb(#[from] WeavebackError),
}

impl From<weaveback_tangle::db::DbError> for CoverageApiError {
    fn from(e: weaveback_tangle::db::DbError) -> Self {
        CoverageApiError::Noweb(WeavebackError::Db(e))
    }
}

impl From<lookup::LookupError> for CoverageApiError {
    fn from(e: lookup::LookupError) -> Self {
        match e {
            lookup::LookupError::Db(e) => CoverageApiError::Noweb(WeavebackError::Db(e)),
            lookup::LookupError::Io(e) => CoverageApiError::Io(e),
            lookup::LookupError::InvalidInput(s) => CoverageApiError::Io(
                std::io::Error::new(std::io::ErrorKind::InvalidInput, s),
            ),
        }
    }
}

impl From<crate::query::ApiError> for CoverageApiError {
    fn from(e: crate::query::ApiError) -> Self {
        match e {
            crate::query::ApiError::Db(e) => CoverageApiError::Noweb(WeavebackError::Db(e)),
            crate::query::ApiError::Io(e) => CoverageApiError::Io(e),
        }
    }
}
// @
```

