// weaveback-tangle/src/noweb.rs
// I'd Really Rather You Didn't edit this generated file.

use memchr;
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Component, Path};

use crate::db::{ChunkDefEntry, Confidence, NowebMapEntry};
use crate::safe_writer::SafeWriterError;
use crate::WeavebackError;
use crate::SafeFileWriter;
use log::debug;

mod types;
mod paths;
mod store_read;
mod expand;
mod utils;
mod writer;
mod clip;
mod remap;
mod write_files;

pub use clip::{tangle_check, Clip};
pub use types::{ChunkDefinitionMatch, ChunkError, NowebSyntax};

pub(in crate::noweb) use paths::{expand_tilde, path_is_safe};
pub(in crate::noweb) use remap::remap_noweb_entries;
pub(in crate::noweb) use store_read::ChunkStore;
pub(in crate::noweb) use types::{ChunkDef, ChunkLocation, NamedChunk};
pub(in crate::noweb) use writer::ChunkWriter;

