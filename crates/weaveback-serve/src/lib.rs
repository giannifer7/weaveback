// weaveback-serve/src/lib.rs
// I'd Really Rather You Didn't edit this generated file.

mod server {
use std::collections::HashMap;
use std::io::{BufRead, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use notify::{RecursiveMode, Watcher};
use tiny_http::{Header, Request, Response, Server, StatusCode};
use weaveback_tangle::tangle_check;

mod sse;
mod watcher;
mod source_watcher;
mod static_files;
mod open;
mod config;
mod source_edit;
mod chunk_context;
mod ai;
mod note;
mod dispatch;
mod run;

pub(in crate::server) use ai::handle_ai;
pub(in crate::server) use chunk_context::handle_chunk;
pub use config::{AiBackend, TangleConfig};
pub(in crate::server) use dispatch::handle_request;
pub(in crate::server) use note::handle_save_note;
pub(in crate::server) use open::open_in_editor;
pub(crate) use open::parse_query;
pub(crate) use source_edit::{
    extract_chunk_body,
    insert_note_into_source,
    json_resp,
};
pub(in crate::server) use source_edit::handle_apply;
pub(in crate::server) use source_watcher::spawn_source_watcher;
pub(crate) use sse::SseReader;
pub(in crate::server) use static_files::serve_static;
pub(in crate::server) use watcher::{spawn_watcher, ReloadVersion, SseSenders};

#[cfg(test)]
pub(crate) use ai::{
    AiChannelReader,
    build_chunk_context,
    dep_bodies,
    extract_prose,
    git_log_for_file,
    heading_level,
    section_range,
    sse_headers,
    title_chain,
};
#[cfg(test)]
pub(crate) use open::percent_decode;
#[cfg(test)]
pub(crate) use run::{find_project_root, run_server_loop};
#[cfg(test)]
pub(crate) use source_edit::{apply_chunk_edit, tangle_oracle};
#[cfg(test)]
pub(crate) use source_watcher::find_docgen_bin;
#[cfg(test)]
pub(crate) use static_files::{content_type, safe_path};

pub use run::{run_serve, run_serve as run_server};
}

#[cfg(test)]
pub(crate) use server::{
    build_chunk_context,
    content_type,
    extract_prose,
    heading_level,
    parse_query,
    percent_decode,
    safe_path,
    section_range,
    sse_headers,
    tangle_oracle,
    title_chain,
    AiChannelReader,
    apply_chunk_edit,
    dep_bodies,
    extract_chunk_body,
    find_docgen_bin,
    find_project_root,
    git_log_for_file,
    insert_note_into_source,
    json_resp,
    run_server_loop,
    SseReader,
};

pub use server::{AiBackend, TangleConfig, run_serve, run_server};

#[cfg(test)]
mod tests;

