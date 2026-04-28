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

include!("noweb/types.rs");
include!("noweb/paths.rs");
include!("noweb/store_read.rs");
include!("noweb/expand.rs");
include!("noweb/utils.rs");
include!("noweb/writer.rs");
include!("noweb/clip.rs");
include!("noweb/remap.rs");
include!("noweb/write_files.rs");

