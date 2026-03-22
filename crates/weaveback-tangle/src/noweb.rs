// <[@file src/noweb.rs]>=
// src/noweb.rs
use regex::Regex;
use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Component, Path};
use std::rc::Rc;

use crate::db::NowebMapEntry;
use crate::safe_writer::SafeWriterError;
use crate::WeavebackError;
use crate::SafeFileWriter;
use log::{debug, warn};

/// Represents a single definition of a named chunk.
#[derive(Debug, Clone)]
struct ChunkDef {
    content: Vec<String>,
    base_indent: usize,
    file_idx: usize,
    line: usize,
}

impl ChunkDef {
    fn new(base_indent: usize, file_idx: usize, line: usize) -> Self {
        Self {
            content: Vec::new(),
            base_indent,
            file_idx,
            line,
        }
    }
}

/// Indicates file + line for error reporting.
#[derive(Debug, Clone)]
pub struct ChunkLocation {
    pub file_idx: usize,
    pub line: usize,
}

/// Possible errors during expansion/definition.
#[derive(Debug)]
pub enum ChunkError {
    RecursionLimit {
        chunk: String,
        file_name: String,
        location: ChunkLocation,
    },
    RecursiveReference {
        chunk: String,
        file_name: String,
        location: ChunkLocation,
    },
    UndefinedChunk {
        chunk: String,
        file_name: String,
        location: ChunkLocation,
    },
    IoError(io::Error),
    /// We add a custom error for multiple @file definitions without @replace.
    FileChunkRedefinition {
        file_chunk: String,
        file_name: String,
        location: ChunkLocation,
    },
}

impl std::fmt::Display for ChunkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChunkError::RecursionLimit {
                chunk,
                file_name,
                location,
            } => {
                write!(
                    f,
                    "Error: {} line {}: maximum recursion depth exceeded while expanding chunk '{}'",
                    file_name,
                    location.line + 1,
                    chunk
                )
            }
            ChunkError::RecursiveReference {
                chunk,
                file_name,
                location,
            } => write!(
                f,
                "Error: {} line {}: recursive reference detected in chunk '{}'",
                file_name,
                location.line + 1,
                chunk
            ),
            ChunkError::UndefinedChunk {
                chunk,
                file_name,
                location,
            } => write!(
                f,
                "Error: {} line {}: referenced chunk '{}' is undefined",
                file_name,
                location.line + 1,
                chunk
            ),
            ChunkError::IoError(e) => write!(f, "Error: I/O error: {}", e),
            ChunkError::FileChunkRedefinition {
                file_chunk,
                file_name,
                location,
            } => write!(
                f,
                "Error: {} line {}: file chunk '{}' is already defined (use @replace to redefine)",
                file_name,
                location.line + 1,
                file_chunk
            ),
        }
    }
}

impl std::error::Error for ChunkError {}

impl From<io::Error> for ChunkError {
    fn from(e: io::Error) -> Self {
        ChunkError::IoError(e)
    }
}

impl From<WeavebackError> for ChunkError {
    fn from(e: WeavebackError) -> Self {
        ChunkError::IoError(std::io::Error::other(e.to_string()))
    }
}

/// Each named chunk can have multiple definitions plus a reference counter.
#[derive(Debug)]
struct NamedChunk {
    definitions: Vec<ChunkDef>,
    references: usize,
}

impl NamedChunk {
    fn new() -> Self {
        Self {
            definitions: Vec::new(),
            references: 0,
        }
    }
}

/// Main store: chunk name -> Rc<RefCell<NamedChunk>>,
/// plus a list of which chunk names start with @file .
pub struct ChunkStore {
    chunks: HashMap<String, Rc<RefCell<NamedChunk>>>,
    file_chunks: Vec<String>,

    open_re: Regex,
    slot_re: Regex,
    close_re: Regex,

    /// All file names for error reporting, indexed by file_idx.
    file_names: Vec<String>,
}

/// Expand a leading `~` to `$HOME` (Unix only; no-op if HOME is unset).
fn expand_tilde(path: &str) -> String {
    if path == "~" {
        return std::env::var("HOME").unwrap_or_else(|_| path.to_string());
    }
    if let Some(rest) = path.strip_prefix("~/") {
        let home = std::env::var("HOME").unwrap_or_else(|_| "~".to_string());
        return format!("{}/{}", home, rest);
    }
    path.to_string()
}

/// Check if the given path is safe (not absolute, no .., no colon).
fn path_is_safe(path: &str) -> Result<(), SafeWriterError> {
    let p = Path::new(path);
    if p.is_absolute() {
        return Err(SafeWriterError::SecurityViolation(
            "Absolute paths are not allowed".to_string(),
        ));
    }
    if p.to_string_lossy().contains(':') {
        return Err(SafeWriterError::SecurityViolation(
            "Windows-style paths are not allowed".to_string(),
        ));
    }
    if p.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(SafeWriterError::SecurityViolation(
            "Path traversal is not allowed".to_string(),
        ));
    }
    Ok(())
}

impl ChunkStore {
    pub fn new(
        open_delim: &str,           // e.g. "<["
        close_delim: &str,          // e.g. "]>"
        chunk_end: &str,            // e.g. "@"
        comment_markers: &[String], // e.g. ["#", "//"]
    ) -> Self {
        let od = regex::escape(open_delim);
        let cd = regex::escape(close_delim);

        // Build patterns that match lines like:
        //   # <<@replace @file chunk>>=
        //   # <<chunk>>=
        // for references:
        //   # <<chunk>>
        //   # <<@reversed chunk>>
        // for closings:
        //   # @
        let escaped_comments = comment_markers
            .iter()
            .map(|m| regex::escape(m))
            .collect::<Vec<_>>()
            .join("|");

        // Opening lines
        let open_pattern = format!(
            r"^(\s*)(?:{})?[ \t]*{}(?:@replace[ \t]+)?(?:@file[ \t]+)?(.+?){}=",
            escaped_comments, od, cd
        );
        // Reference lines
        let slot_pattern = format!(
            r"^(\s*)(?:{})?\s*{}(?:@file\s+|@reversed\s+)?(.+?){}\s*$",
            escaped_comments, od, cd
        );
        // Closing lines
        let close_pattern = format!(
            r"^(?:{})?[ \t]*{}\s*$",
            escaped_comments,
            regex::escape(chunk_end)
        );

        Self {
            chunks: HashMap::new(),
            file_chunks: Vec::new(),
            open_re: Regex::new(&open_pattern).expect("Invalid open pattern"),
            slot_re: Regex::new(&slot_pattern).expect("Invalid slot pattern"),
            close_re: Regex::new(&close_pattern).expect("Invalid close pattern"),
            file_names: Vec::new(),
        }
    }

    pub fn add_file_name(&mut self, fname: &str) -> usize {
        let idx = self.file_names.len();
        self.file_names.push(fname.to_string());
        idx
    }

    fn validate_chunk_name(&self, chunk_name: &str, line: &str) -> bool {
        if line.contains("@file") {
            // chunk_name is "@file <path>"; validate only the path part.
            let path = chunk_name.strip_prefix("@file ").unwrap_or(chunk_name);
            path_is_safe(path).is_ok()
        } else {
            !chunk_name.is_empty()
        }
    }

    /// The main function for reading lines from the input text.
    /// - If the line opens a chunk, we define it (or replace it).
    /// - If the line closes a chunk, we end the current one.
    /// - Otherwise, if we’re inside a chunk, we add lines to it.
    ///
    /// Then we fill out file_chunks for any chunk name that starts with @file .
    pub fn read(&mut self, text: &str, file_idx: usize) {
        debug!("Reading text for file_idx: {}", file_idx);
        let mut current_chunk: Option<(String, usize)> = None;
        let mut line_no: i32 = -1;

        for line in text.lines() {
            line_no += 1;

            // Check if it's an opening line for a chunk
            if let Some(caps) = self.open_re.captures(line) {
                let indentation = caps.get(1).map_or("", |m| m.as_str());
                let base_name = caps.get(2).map_or("", |m| m.as_str()).to_string();
                debug!(
                    "Found open pattern: indentation='{}', base_name='{}'",
                    indentation, base_name
                );

                let is_replace = line.contains("@replace");
                let is_file = line.contains("@file");
                // If line has @file, chunk name should be "@file something"
                let full_name = if is_file {
                    format!("@file {}", base_name)
                } else {
                    base_name
                };

                if self.validate_chunk_name(&full_name, line) {
                    // If this is a file chunk, check for existing definitions
                    // unless @replace is present
                    if full_name.starts_with("@file ") {
                        if self.chunks.contains_key(&full_name) && !is_replace {
                            // Report the error and keep the first definition.
                            // Silently dropping both definitions would hide the mistake.
                            let location = ChunkLocation {
                                file_idx,
                                line: line_no as usize,
                            };
                            eprintln!(
                                "{}",
                                ChunkError::FileChunkRedefinition {
                                    file_chunk: full_name.clone(),
                                    file_name: self
                                        .file_names
                                        .get(file_idx)
                                        .cloned()
                                        .unwrap_or_default(),
                                    location,
                                }
                            );
                            continue;
                        }
                        if is_replace {
                            // remove old definition
                            self.chunks.remove(&full_name);
                        }
                    } else if is_replace {
                        // normal chunk with @replace
                        self.chunks.remove(&full_name);
                    }

                    // Now define the chunk
                    let rc = self
                        .chunks
                        .entry(full_name.clone())
                        .or_insert_with(|| Rc::new(RefCell::new(NamedChunk::new())));
                    let mut borrowed = rc.borrow_mut();
                    let def_idx = borrowed.definitions.len();
                    borrowed.definitions.push(ChunkDef::new(
                        indentation.len(),
                        file_idx,
                        line_no as usize,
                    ));
                    drop(borrowed);

                    current_chunk = Some((full_name.clone(), def_idx));
                    if full_name.starts_with("@file ") && !self.file_chunks.contains(&full_name) {
                        self.file_chunks.push(full_name.clone());
                    }
                    debug!("Started chunk: {}", full_name);
                }
                continue;
            }

            // If it's a closing line
            if self.close_re.is_match(line) {
                current_chunk = None;
                continue;
            }

            // If we're in a chunk, add lines to it
            if let Some((ref cname, idx)) = current_chunk
                && let Some(rc) = self.chunks.get(cname)
            {
                let mut borrowed = rc.borrow_mut();
                let def = borrowed.definitions.get_mut(idx).unwrap();
                if line.ends_with('\n') {
                    def.content.push(line.to_string());
                } else {
                    def.content.push(format!("{}\n", line));
                }
            }
        }

        debug!("Finished reading. File chunks: {:?}", self.file_chunks);
    }

    /// Increments references on a chunk or returns an error if undefined.
    fn inc_references(&self, chunk_name: &str, location: &ChunkLocation) -> Result<(), ChunkError> {
        if let Some(rc) = self.chunks.get(chunk_name) {
            let mut borrowed = rc.borrow_mut();
            borrowed.references += 1;
            Ok(())
        } else {
            let file_name = self
                .file_names
                .get(location.file_idx)
                .cloned()
                .unwrap_or_default();
            Err(ChunkError::UndefinedChunk {
                chunk: chunk_name.to_string(),
                file_name,
                location: location.clone(),
            })
        }
    }

    /// Expands chunk references, possibly reversing definitions if @reversed is in the line.
    pub fn expand_with_depth(
        &self,
        chunk_name: &str,
        target_indent: &str,
        depth: usize,
        seen: &mut Vec<(String, ChunkLocation)>,
        reference_location: ChunkLocation,
        reversed_mode: bool,
    ) -> Result<Vec<String>, ChunkError> {
        const MAX_DEPTH: usize = 100;
        if depth > MAX_DEPTH {
            let file_name = self
                .file_names
                .get(reference_location.file_idx)
                .cloned()
                .unwrap_or_default();
            return Err(ChunkError::RecursionLimit {
                chunk: chunk_name.to_string(),
                file_name,
                location: reference_location,
            });
        }

        // Check recursion
        if seen.iter().any(|(nm, _)| nm == chunk_name) {
            let file_name = self
                .file_names
                .get(reference_location.file_idx)
                .cloned()
                .unwrap_or_default();
            return Err(ChunkError::RecursiveReference {
                chunk: chunk_name.to_string(),
                file_name,
                location: reference_location,
            });
        }

        // Check existence first
        if !self.chunks.contains_key(chunk_name) {
            let file_name = self
                .file_names
                .get(reference_location.file_idx)
                .cloned()
                .unwrap_or_default();
            warn!(
                "Undefined chunk '{}' referenced at {} line {}. Treating as empty.",
                chunk_name,
                file_name,
                reference_location.line + 1
            );
            return Ok(Vec::new());
        }

        // Bump references
        self.inc_references(chunk_name, &reference_location)?;

        let rc = self.chunks.get(chunk_name).unwrap();

        let borrowed = rc.borrow();
        let defs = &borrowed.definitions;

        // Reverse definitions if @reversed
        let iter: Box<dyn Iterator<Item = &ChunkDef>> = if reversed_mode {
            Box::new(defs.iter().rev())
        } else {
            Box::new(defs.iter())
        };

        seen.push((chunk_name.to_string(), reference_location));
        let mut result = Vec::new();

        for def in iter {
            let mut def_output = Vec::new();
            let mut line_count = 0;
            for line in &def.content {
                line_count += 1;
                // Check if line references another chunk
                if let Some(caps) = self.slot_re.captures(line) {
                    let add_indent = caps.get(1).map_or("", |m| m.as_str());
                    let referenced_chunk = caps.get(2).map_or("", |m| m.as_str());

                    let line_is_reversed = line.contains("@reversed");
                    let relative_indent = if add_indent.len() > def.base_indent {
                        &add_indent[def.base_indent..]
                    } else {
                        ""
                    };
                    let new_indent = if target_indent.is_empty() {
                        relative_indent.to_owned()
                    } else {
                        format!("{}{}", target_indent, relative_indent)
                    };
                    let new_loc = ChunkLocation {
                        file_idx: def.file_idx,
                        line: def.line + line_count - 1,
                    };

                    let expanded = self.expand_with_depth(
                        referenced_chunk.trim(),
                        &new_indent,
                        depth + 1,
                        seen,
                        new_loc,
                        line_is_reversed,
                    )?;
                    def_output.extend(expanded);
                } else {
                    // Plain line
                    let line_indent = if line.len() > def.base_indent {
                        &line[def.base_indent..]
                    } else {
                        line
                    };
                    if target_indent.is_empty() {
                        def_output.push(line_indent.to_owned());
                    } else {
                        def_output.push(format!("{}{}", target_indent, line_indent));
                    }
                }
            }
            result.extend(def_output);
        }

        seen.pop();
        Ok(result)
    }

    /// Expand from top-level (no reversed).
    pub fn expand(&self, chunk_name: &str, indent: &str) -> Result<Vec<String>, ChunkError> {
        let mut seen = Vec::new();
        let loc = ChunkLocation {
            file_idx: 0,
            line: 0,
        };
        self.expand_with_depth(chunk_name, indent, 0, &mut seen, loc, false)
    }

    /// Like `expand_with_depth` but also returns a `NowebMapEntry` per output
    /// line for source-map purposes.  Slot lines (chunk references) are replaced
    /// by the entries of their sub-expansion; plain lines produce one entry each.
    fn expand_with_depth_impl(
        &self,
        chunk_name: &str,
        target_indent: &str,
        depth: usize,
        seen: &mut Vec<(String, ChunkLocation)>,
        reference_location: ChunkLocation,
        reversed_mode: bool,
    ) -> Result<Vec<(String, NowebMapEntry)>, ChunkError> {
        const MAX_DEPTH: usize = 100;
        if depth > MAX_DEPTH {
            let file_name = self
                .file_names
                .get(reference_location.file_idx)
                .cloned()
                .unwrap_or_default();
            return Err(ChunkError::RecursionLimit {
                chunk: chunk_name.to_string(),
                file_name,
                location: reference_location,
            });
        }

        if seen.iter().any(|(nm, _)| nm == chunk_name) {
            let file_name = self
                .file_names
                .get(reference_location.file_idx)
                .cloned()
                .unwrap_or_default();
            return Err(ChunkError::RecursiveReference {
                chunk: chunk_name.to_string(),
                file_name,
                location: reference_location,
            });
        }

        if !self.chunks.contains_key(chunk_name) {
            let file_name = self
                .file_names
                .get(reference_location.file_idx)
                .cloned()
                .unwrap_or_default();
            warn!(
                "Undefined chunk '{}' referenced at {} line {}. Treating as empty.",
                chunk_name,
                file_name,
                reference_location.line + 1
            );
            return Ok(Vec::new());
        }

        self.inc_references(chunk_name, &reference_location)?;

        let rc = self.chunks.get(chunk_name).unwrap();
        let borrowed = rc.borrow();
        let defs = &borrowed.definitions;

        let iter: Box<dyn Iterator<Item = &ChunkDef>> = if reversed_mode {
            Box::new(defs.iter().rev())
        } else {
            Box::new(defs.iter())
        };

        seen.push((chunk_name.to_string(), reference_location));
        let mut result = Vec::new();

        for def in iter {
            let src_file = self
                .file_names
                .get(def.file_idx)
                .cloned()
                .unwrap_or_default();
            let mut line_count = 0usize;

            for line in &def.content {
                line_count += 1;

                if let Some(caps) = self.slot_re.captures(line) {
                    let add_indent = caps.get(1).map_or("", |m| m.as_str());
                    let referenced_chunk = caps.get(2).map_or("", |m| m.as_str());

                    let line_is_reversed = line.contains("@reversed");
                    let relative_indent = if add_indent.len() > def.base_indent {
                        &add_indent[def.base_indent..]
                    } else {
                        ""
                    };
                    let new_indent = if target_indent.is_empty() {
                        relative_indent.to_owned()
                    } else {
                        format!("{}{}", target_indent, relative_indent)
                    };
                    let new_loc = ChunkLocation {
                        file_idx: def.file_idx,
                        line: def.line + line_count - 1,
                    };

                    let expanded = self.expand_with_depth_impl(
                        referenced_chunk.trim(),
                        &new_indent,
                        depth + 1,
                        seen,
                        new_loc,
                        line_is_reversed,
                    )?;
                    result.extend(expanded);
                } else {
                    // Plain line — emit with source-map entry.
                    let line_indent = if line.len() > def.base_indent {
                        &line[def.base_indent..]
                    } else {
                        line
                    };
                    let out_line = if target_indent.is_empty() {
                        line_indent.to_owned()
                    } else {
                        format!("{}{}", target_indent, line_indent)
                    };
                    let entry = NowebMapEntry {
                        src_file: src_file.clone(),
                        chunk_name: chunk_name.to_string(),
                        src_line: (def.line + line_count) as u32,
                        indent: target_indent.to_string(),
                    };
                    result.push((out_line, entry));
                }
            }
        }

        seen.pop();
        Ok(result)
    }

    /// Expand a top-level chunk and return both the output lines and their
    /// source-map entries (one entry per output line, in order).
    pub fn expand_with_map(
        &self,
        chunk_name: &str,
        indent: &str,
    ) -> Result<(Vec<String>, Vec<NowebMapEntry>), ChunkError> {
        let mut seen = Vec::new();
        let loc = ChunkLocation {
            file_idx: 0,
            line: 0,
        };
        let pairs = self.expand_with_depth_impl(chunk_name, indent, 0, &mut seen, loc, false)?;
        let (lines, entries) = pairs.into_iter().unzip();
        Ok((lines, entries))
    }

    /// For tests or direct usage: get chunk content with no indentation.
    pub fn get_chunk_content(&self, chunk_name: &str) -> Result<Vec<String>, ChunkError> {
        self.expand(chunk_name, "")
    }

    /// Return a slice of chunk names that start with "@file ".
    pub fn get_file_chunks(&self) -> &[String] {
        &self.file_chunks
    }

    /// Check if the store has a chunk of the given name.
    pub fn has_chunk(&self, name: &str) -> bool {
        self.chunks.contains_key(name)
    }

    /// Reset everything
    pub fn reset(&mut self) {
        self.chunks.clear();
        self.file_chunks.clear();
        self.file_names.clear();
    }

    /// Warnings for any chunk never referenced.
    pub fn check_unused_chunks(&self) -> Vec<String> {
        let mut warns = Vec::new();
        for (name, rc) in &self.chunks {
            if !name.starts_with("@file ") {
                let borrowed = rc.borrow();
                if borrowed.references == 0
                    && let Some(first_def) = borrowed.definitions.first()
                {
                    let fname = self
                        .file_names
                        .get(first_def.file_idx)
                        .cloned()
                        .unwrap_or_default();
                    let ln = first_def.line + 1;
                    warns.push(format!(
                        "Warning: {} line {}: chunk '{}' is defined but never referenced",
                        fname, ln, name
                    ));
                }
            }
        }
        warns.sort();
        warns
    }
}

/// Writes @file ... chunks to disk
pub struct ChunkWriter<'a> {
    safe_file_writer: &'a mut SafeFileWriter,
}

impl<'a> ChunkWriter<'a> {
    pub fn new(sw: &'a mut SafeFileWriter) -> Self {
        Self {
            safe_file_writer: sw,
        }
    }

    pub fn write_chunk(&mut self, chunk_name: &str, content: &[String]) -> Result<(), WeavebackError> {
        if !chunk_name.starts_with("@file ") {
            return Ok(());
        }
        let path_str = chunk_name["@file ".len()..].trim();
        let expanded = expand_tilde(path_str);
        let path = std::path::Path::new(&expanded);

        if path.is_absolute() {
            // Tilde-expanded (or explicitly absolute) path: write directly.
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut f = fs::File::create(path)?;
            for line in content {
                f.write_all(line.as_bytes())?;
            }
        } else {
            let final_path = self.safe_file_writer.before_write(path_str)?;
            let mut f = fs::File::create(&final_path)?;
            for line in content {
                f.write_all(line.as_bytes())?;
            }
            self.safe_file_writer.after_write(path_str)?;
        }
        Ok(())
    }
}

/// High-level reading, expanding, writing API.
pub struct Clip {
    store: ChunkStore,
    writer: SafeFileWriter,
}

impl Clip {
    pub fn new(
        safe_file_writer: SafeFileWriter,
        open_delim: &str,
        close_delim: &str,
        chunk_end: &str,
        comment_markers: &[String],
    ) -> Self {
        Self {
            store: ChunkStore::new(open_delim, close_delim, chunk_end, comment_markers),
            writer: safe_file_writer,
        }
    }

    pub fn reset(&mut self) {
        self.store.reset();
    }

    pub fn has_chunk(&self, name: &str) -> bool {
        self.store.has_chunk(name)
    }

    pub fn get_file_chunks(&self) -> Vec<String> {
        self.store.get_file_chunks().to_vec()
    }

    pub fn check_unused_chunks(&self) -> Vec<String> {
        self.store.check_unused_chunks()
    }

    /// Read from a file on disk, storing chunk definitions.
    /// Pass `"-"` to read from stdin.
    pub fn read_file<P: AsRef<Path>>(&mut self, path: P) -> Result<(), WeavebackError> {
        let fname = path.as_ref().to_string_lossy().to_string();
        let idx = self.store.add_file_name(&fname);
        let text = if path.as_ref() == Path::new("-") {
            let mut buf = String::new();
            io::stdin().lock().read_to_string(&mut buf)?;
            buf
        } else {
            fs::read_to_string(&path)?
        };
        self.store.read(&text, idx);
        Ok(())
    }

    /// Read from an in-memory string, specifying a "filename" for error messages.
    pub fn read(&mut self, text: &str, file_name: &str) {
        let idx = self.store.add_file_name(file_name);
        self.store.read(text, idx);
    }

    /// Write all file chunks to disk and populate the noweb_map table.
    pub fn write_files(&mut self) -> Result<(), WeavebackError> {
        let fc = self.store.get_file_chunks().to_vec();
        for name in &fc {
            let (lines, map_entries) = self.store.expand_with_map(name, "")?;

            let mut cw = ChunkWriter::new(&mut self.writer);
            cw.write_chunk(name, &lines)?;

            // Write source-map entries for this output file.
            let out_file = name.strip_prefix("@file ").unwrap_or(name).trim();
            let keyed: Vec<(u32, NowebMapEntry)> = map_entries
                .into_iter()
                .enumerate()
                .map(|(i, e)| (i as u32, e))
                .collect();
            self.writer
                .db()
                .set_noweb_entries(out_file, &keyed)
                .map_err(|e| WeavebackError::SafeWriter(SafeWriterError::DbError(e)))?;
        }
        let warns = self.store.check_unused_chunks();
        for w in warns {
            eprintln!("{}", w);
        }
        Ok(())
    }

    /// Access the underlying database (for writing src_snapshots after
    /// `write_files()` returns).
    pub fn db(&self) -> &crate::db::WeavebackDb {
        self.writer.db()
    }

    /// Merge the temp database into `target` (e.g. `./weaveback.db`) and clean up.
    /// Call this after `write_files()` and any src_snapshot writes.
    pub fn finish(self, target: &Path) -> Result<(), WeavebackError> {
        self.writer.finish(target).map_err(WeavebackError::SafeWriter)
    }

    /// Expand a chunk and write to an arbitrary writer.
    pub fn get_chunk<W: io::Write>(
        &self,
        chunk_name: &str,
        out_stream: &mut W,
    ) -> Result<(), WeavebackError> {
        let lines = self.store.expand(chunk_name, "")?;
        for line in lines {
            out_stream.write_all(line.as_bytes())?;
        }
        out_stream.write_all(b"\n")?;
        Ok(())
    }

    /// Expand a chunk into a vector of lines.
    pub fn expand(&self, chunk_name: &str, indent: &str) -> Result<Vec<String>, WeavebackError> {
        Ok(self.store.expand(chunk_name, indent)?)
    }

    /// Retrieve the chunk content directly (commonly used in tests).
    pub fn get_chunk_content(&self, name: &str) -> Result<Vec<String>, ChunkError> {
        self.store.get_chunk_content(name)
    }

    pub fn read_files<P: AsRef<Path>>(&mut self, input_paths: &[P]) -> Result<(), WeavebackError> {
        for path in input_paths {
            self.read_file(path)?;
        }
        Ok(())
    }
}
// $$
