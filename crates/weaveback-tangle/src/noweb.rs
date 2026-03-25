use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Component, Path};

use crate::db::NowebMapEntry;
use crate::safe_writer::SafeWriterError;
use crate::WeavebackError;
use crate::SafeFileWriter;
use log::{debug, warn};

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
#[derive(Debug, Clone)]
pub struct ChunkLocation {
    pub file_idx: usize,
    pub line: usize,
}

#[derive(Debug)]
pub enum ChunkError {
    RecursionLimit {
        chunk: String,
        file_name: String,
        location: ChunkLocation,
    },
    RecursiveReference {
        chunk: String,
        cycle: Vec<String>,
        file_name: String,
        location: ChunkLocation,
    },
    UndefinedChunk {
        chunk: String,
        file_name: String,
        location: ChunkLocation,
    },
    IoError(io::Error),
    FileChunkRedefinition {
        file_chunk: String,
        file_name: String,
        location: ChunkLocation,
    },
}

impl std::fmt::Display for ChunkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChunkError::RecursionLimit { chunk, file_name, location } => write!(
                f,
                "Error: {} line {}: maximum recursion depth exceeded while expanding chunk '{}'",
                file_name, location.line + 1, chunk
            ),
            ChunkError::RecursiveReference { chunk, cycle, file_name, location } => {
                let trace = cycle.join(" -> ");
                write!(
                    f,
                    "Error: {} line {}: recursive reference detected in chunk '{}' (cycle: {})",
                    file_name, location.line + 1, chunk, trace
                )
            }
            ChunkError::UndefinedChunk { chunk, file_name, location } => write!(
                f,
                "Error: {} line {}: referenced chunk '{}' is undefined",
                file_name, location.line + 1, chunk
            ),
            ChunkError::IoError(e) => write!(f, "Error: I/O error: {}", e),
            ChunkError::FileChunkRedefinition { file_chunk, file_name, location } => write!(
                f,
                "Error: {} line {}: file chunk '{}' is already defined (use @replace to redefine)",
                file_name, location.line + 1, file_chunk
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
#[derive(Debug)]
struct NamedChunk {
    definitions: Vec<ChunkDef>,
}

impl NamedChunk {
    fn new() -> Self {
        Self {
            definitions: Vec::new(),
        }
    }
}
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
pub struct ChunkStore {
    chunks: HashMap<String, NamedChunk>,
    file_chunks: Vec<String>,
    open_re: Regex,
    slot_re: Regex,
    close_re: Regex,
    file_names: Vec<String>,
}
impl ChunkStore {
    pub fn new(
        open_delim: &str,
        close_delim: &str,
        chunk_end: &str,
        comment_markers: &[String],
    ) -> Self {
        let od = regex::escape(open_delim);
        let cd = regex::escape(close_delim);

        let escaped_comments = comment_markers
            .iter()
            .map(|m| regex::escape(m))
            .collect::<Vec<_>>()
            .join("|");

        let open_pattern = format!(
            r"^(?P<indent>\s*)(?:{})?[ \t]*{}(?P<replace>@replace[ \t]+)?(?P<file>@file[ \t]+)?(?P<name>.+?){}=",
            escaped_comments, od, cd
        );
        let slot_pattern = format!(
            r"^(\s*)(?:{})?\s*{}((?:@file\s+|@reversed\s+)?)(.+?){}\s*$",
            escaped_comments, od, cd
        );
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

    fn validate_chunk_name(&self, chunk_name: &str, is_file: bool) -> bool {
        if is_file {
            let path = chunk_name.strip_prefix("@file ").unwrap_or(chunk_name);
            path_is_safe(path).is_ok()
        } else {
            !chunk_name.is_empty()
        }
    }
}
impl ChunkStore {
    pub fn read(&mut self, text: &str, file_idx: usize) {
        debug!("Reading text for file_idx: {}", file_idx);
        let mut current_chunk: Option<(String, usize)> = None;

        for (line_no, line) in text.lines().enumerate() {
            if let Some(caps) = self.open_re.captures(line) {
                let indentation = caps.name("indent").map_or("", |m| m.as_str());
                let base_name = caps.name("name").map_or("", |m| m.as_str()).to_string();
                debug!(
                    "Found open pattern: indentation='{}', base_name='{}'",
                    indentation, base_name
                );

                let is_replace = caps.name("replace").is_some();
                let is_file = caps.name("file").is_some();
                let full_name = if is_file {
                    format!("@file {}", base_name)
                } else {
                    base_name
                };

                if self.validate_chunk_name(&full_name, is_file) {
                    if full_name.starts_with("@file ") {
                        if self.chunks.contains_key(&full_name) && !is_replace {
                            let location = ChunkLocation { file_idx, line: line_no };
                            // Report and skip: silently dropping both definitions would hide the mistake.
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
                            self.chunks.remove(&full_name);
                        }
                    } else if is_replace {
                        self.chunks.remove(&full_name);
                    }

                    let chunk = self
                        .chunks
                        .entry(full_name.clone())
                        .or_insert_with(NamedChunk::new);
                    let def_idx = chunk.definitions.len();
                    chunk.definitions.push(ChunkDef::new(
                        indentation.len(),
                        file_idx,
                        line_no,
                    ));

                    current_chunk = Some((full_name.clone(), def_idx));
                    if full_name.starts_with("@file ") && !self.file_chunks.contains(&full_name) {
                        self.file_chunks.push(full_name.clone());
                    }
                    debug!("Started chunk: {}", full_name);
                }
                continue;
            }

            if self.close_re.is_match(line) {
                current_chunk = None;
                continue;
            }

            if let Some((ref cname, idx)) = current_chunk
                && let Some(chunk) = self.chunks.get_mut(cname)
            {
                let def = chunk.definitions.get_mut(idx)
                    .expect("internal invariant: def_idx is valid");
                if line.ends_with('\n') {
                    def.content.push(line.to_string());
                } else {
                    def.content.push(format!("{}\n", line));
                }
            }
        }

        debug!("Finished reading. File chunks: {:?}", self.file_chunks);
    }
}
impl ChunkStore {
    fn expand_inner(
        &self,
        chunk_name: &str,
        target_indent: &str,
        depth: usize,
        seen: &mut HashSet<String>,
        stack: &mut Vec<String>,
        referenced_chunks: &mut HashSet<String>,
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

        if seen.contains(chunk_name) {
            let file_name = self
                .file_names
                .get(reference_location.file_idx)
                .cloned()
                .unwrap_or_default();
            let mut cycle = stack.clone();
            cycle.push(chunk_name.to_string());
            return Err(ChunkError::RecursiveReference {
                chunk: chunk_name.to_string(),
                cycle,
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

        referenced_chunks.insert(chunk_name.to_string());

        let chunk = self.chunks.get(chunk_name)
            .expect("internal invariant: chunk exists after contains_key check");
        let defs = &chunk.definitions;

        // Collect indices so we can reverse without a Box<dyn Iterator>.
        let indices: Vec<usize> = if reversed_mode {
            (0..defs.len()).rev().collect()
        } else {
            (0..defs.len()).collect()
        };

        seen.insert(chunk_name.to_string());
        stack.push(chunk_name.to_string());
        let mut result = Vec::new();

        for def_idx in indices {
            let def = &defs[def_idx];
            let src_file = self
                .file_names
                .get(def.file_idx)
                .cloned()
                .unwrap_or_default();

            for (line_count, line) in def.content.iter().enumerate() {
                if let Some(caps) = self.slot_re.captures(line) {
                    let add_indent = caps.get(1).map_or("", |m| m.as_str());
                    let modifier = caps.get(2).map_or("", |m| m.as_str());
                    let referenced_chunk = caps.get(3).map_or("", |m| m.as_str());

                    let line_is_reversed = modifier.contains("@reversed");
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
                        line: def.line + line_count,
                    };

                    let expanded = self.expand_inner(
                        referenced_chunk.trim(),
                        &new_indent,
                        depth + 1,
                        seen,
                        stack,
                        referenced_chunks,
                        new_loc,
                        line_is_reversed,
                    )?;
                    result.extend(expanded);
                } else {
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
                        src_line: (def.line + line_count + 1) as u32,
                        indent: target_indent.to_string(),
                    };
                    result.push((out_line, entry));
                }
            }
        }

        stack.pop();
        seen.remove(chunk_name);
        Ok(result)
    }

    pub fn expand_with_map(
        &self,
        chunk_name: &str,
        indent: &str,
    ) -> Result<(Vec<String>, Vec<NowebMapEntry>, HashSet<String>), ChunkError> {
        let mut seen = HashSet::new();
        let mut stack = Vec::new();
        let mut referenced_chunks = HashSet::new();
        let loc = ChunkLocation { file_idx: 0, line: 0 };
        let pairs = self.expand_inner(
            chunk_name, indent, 0, &mut seen, &mut stack,
            &mut referenced_chunks, loc, false,
        )?;
        let (lines, entries) = pairs.into_iter().unzip();
        Ok((lines, entries, referenced_chunks))
    }

    pub fn expand(&self, chunk_name: &str, indent: &str) -> Result<Vec<String>, ChunkError> {
        let (lines, _, _) = self.expand_with_map(chunk_name, indent)?;
        Ok(lines)
    }

    pub fn get_chunk_content(&self, chunk_name: &str) -> Result<Vec<String>, ChunkError> {
        self.expand(chunk_name, "")
    }
}
impl ChunkStore {
    pub fn get_file_chunks(&self) -> &[String] {
        &self.file_chunks
    }

    pub fn has_chunk(&self, name: &str) -> bool {
        self.chunks.contains_key(name)
    }

    pub fn reset(&mut self) {
        self.chunks.clear();
        self.file_chunks.clear();
        self.file_names.clear();
    }

    pub fn check_unused_chunks(&self, referenced: &HashSet<String>) -> Vec<String> {
        let mut warns = Vec::new();
        for (name, chunk) in &self.chunks {
            if !name.starts_with("@file ") && !referenced.contains(name)
                && let Some(first_def) = chunk.definitions.first()
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
        warns.sort();
        warns
    }
}
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
            // @file ~/foo.rs tilde-expands to an absolute path outside gen/.
            // This is only allowed when allow_home is set; otherwise we refuse
            // rather than silently escape the sandbox.
            if !self.safe_file_writer.get_config().allow_home {
                return Err(WeavebackError::SafeWriter(
                    SafeWriterError::SecurityViolation(format!(
                        "writing outside gen/ requires --allow-home: {}",
                        path.display()
                    )),
                ));
            }
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
fn remap_noweb_entries(
    pre_lines: &[String],
    post_content: &str,
    entries: Vec<NowebMapEntry>,
) -> Vec<(u32, NowebMapEntry)> {
    use similar::{ChangeTag, TextDiff};

    let pre_content: String = pre_lines.concat();
    let diff = TextDiff::from_lines(pre_content.as_str(), post_content);

    // Build old_line → new_line mapping from Equal changes.
    // old_line and new_line are 0-indexed.
    let mut old_to_new: Vec<Option<usize>> = vec![None; pre_lines.len()];
    let mut old_idx = 0usize;
    let mut new_idx = 0usize;

    for change in diff.iter_all_changes() {
        match change.tag() {
            ChangeTag::Equal => {
                if old_idx < old_to_new.len() {
                    old_to_new[old_idx] = Some(new_idx);
                }
                old_idx += 1;
                new_idx += 1;
            }
            ChangeTag::Delete => {
                // Pre-formatter line removed by formatter — no new line.
                old_idx += 1;
            }
            ChangeTag::Insert => {
                // Formatter inserted a new line — no old line.
                new_idx += 1;
            }
        }
    }

    // Build the post-formatter entries.
    // For each new line, find the nearest old line that maps to it.
    let post_line_count = post_content.lines().count();
    let mut new_to_entry: Vec<Option<NowebMapEntry>> = vec![None; post_line_count];

    for (old_i, entry) in entries.into_iter().enumerate() {
        if let Some(&Some(new_i)) = old_to_new.get(old_i) {
            if new_i < post_line_count {
                new_to_entry[new_i] = Some(entry);
            }
        }
    }

    // Fill gaps: lines inserted by the formatter inherit from
    // the nearest preceding mapped line.
    let mut last_entry: Option<NowebMapEntry> = None;
    for slot in new_to_entry.iter_mut() {
        if slot.is_some() {
            last_entry = slot.clone();
        } else if let Some(ref prev) = last_entry {
            *slot = Some(prev.clone());
        }
    }

    new_to_entry
        .into_iter()
        .enumerate()
        .filter_map(|(i, e)| e.map(|entry| (i as u32, entry)))
        .collect()
}
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

    pub fn check_unused_chunks(&self, referenced: &HashSet<String>) -> Vec<String> {
        self.store.check_unused_chunks(referenced)
    }

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

    pub fn read(&mut self, text: &str, file_name: &str) {
        let idx = self.store.add_file_name(file_name);
        self.store.read(text, idx);
    }

    pub fn read_files<P: AsRef<Path>>(&mut self, input_paths: &[P]) -> Result<(), WeavebackError> {
        for path in input_paths {
            self.read_file(path)?;
        }
        Ok(())
    }

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

    pub fn expand(&self, chunk_name: &str, indent: &str) -> Result<Vec<String>, WeavebackError> {
        Ok(self.store.expand(chunk_name, indent)?)
    }

    pub fn get_chunk_content(&self, name: &str) -> Result<Vec<String>, ChunkError> {
        self.store.get_chunk_content(name)
    }
}
impl Clip {
    pub fn write_files(&mut self) -> Result<(), WeavebackError> {
        let fc = self.store.get_file_chunks().to_vec();
        let mut all_referenced = HashSet::new();
        for name in &fc {
            let (lines, map_entries, referenced) = self.store.expand_with_map(name, "")?;
            all_referenced.extend(referenced);

            let mut cw = ChunkWriter::new(&mut self.writer);
            cw.write_chunk(name, &lines)?;

            let out_file = name.strip_prefix("@file ").unwrap_or(name).trim();

            // After formatting, re-key map entries to post-formatter lines.
            let expanded = expand_tilde(out_file);
            let out_path = if std::path::Path::new(&expanded).is_absolute() {
                std::path::PathBuf::from(&expanded)
            } else {
                self.writer.get_gen_base().join(out_file)
            };
            let keyed = if out_path.is_file() {
                let formatted = fs::read_to_string(&out_path)?;
                let pre_content: String = lines.concat();
                if formatted != pre_content {
                    remap_noweb_entries(&lines, &formatted, map_entries)
                } else {
                    map_entries.into_iter().enumerate()
                        .map(|(i, e)| (i as u32, e)).collect()
                }
            } else {
                map_entries.into_iter().enumerate()
                    .map(|(i, e)| (i as u32, e)).collect()
            };

            self.writer
                .db()
                .set_noweb_entries(out_file, &keyed)
                .map_err(|e| WeavebackError::SafeWriter(SafeWriterError::DbError(e)))?;
        }
        let warns = self.store.check_unused_chunks(&all_referenced);
        for w in warns {
            eprintln!("{}", w);
        }
        Ok(())
    }

    pub fn list_output_files(&self) -> Vec<std::path::PathBuf> {
        let gen_base = self.writer.get_gen_base();
        self.store
            .get_file_chunks()
            .iter()
            .map(|name| {
                let path_str = name.strip_prefix("@file ").unwrap_or(name).trim();
                let expanded = expand_tilde(path_str);
                let path = std::path::Path::new(&expanded);
                if path.is_absolute() {
                    path.to_path_buf()
                } else {
                    gen_base.join(path_str)
                }
            })
            .collect()
    }

    pub fn db(&self) -> &crate::db::WeavebackDb {
        self.writer.db()
    }

    pub fn finish(self, target: &Path) -> Result<(), WeavebackError> {
        self.writer.finish(target).map_err(WeavebackError::SafeWriter)
    }
}
