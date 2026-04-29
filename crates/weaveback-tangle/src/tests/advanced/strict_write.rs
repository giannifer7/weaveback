// weaveback-tangle/src/tests/advanced/strict_write.rs
// I'd Really Rather You Didn't edit this generated file.

use super::super::*;
use crate::{ChunkError, WeavebackError};

#[test]
fn write_files_strict_rejects_file_chunk_redefinition() {
    // strict_undefined must be set BEFORE read() so the error is captured
    let mut setup = TestSetup::new(&["#"]);
    setup.clip.set_strict_undefined(true);
    setup.clip.read(
        "# <<@file out.txt>>=\nfirst\n# @\n\n# <<@file out.txt>>=\nsecond\n# @\n",
        "src.nw",
    );
    let err = setup.clip.write_files().unwrap_err();
    match err {
        WeavebackError::Chunk(ChunkError::FileChunkRedefinition { .. }) => {}
        other => panic!("expected FileChunkRedefinition, got: {:?}", other),
    }
}

#[test]
fn write_files_incremental_strict_rejects_parse_errors() {
    // strict_undefined must be set BEFORE read() so the error is captured
    let mut setup = TestSetup::new(&["#"]);
    setup.clip.set_strict_undefined(true);
    setup.clip.read(
        "# <<@file out.txt>>=\nfirst\n# @\n\n# <<@file out.txt>>=\nsecond\n# @\n",
        "src.nw",
    );
    let skip = std::collections::HashSet::new();
    let err = setup.clip.write_files_incremental(&skip).unwrap_err();
    match err {
        WeavebackError::Chunk(ChunkError::FileChunkRedefinition { .. }) => {}
        other => panic!("expected FileChunkRedefinition, got: {:?}", other),
    }
}

