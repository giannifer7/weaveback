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

#[derive(Debug, Clone)]
struct ChunkDef {
    content: Vec<String>,
    base_indent: usize,
    file_idx: usize,
    /// 0-indexed line of the open marker (`// <<name>>=`) in the source file.
    line: usize,
    /// 0-indexed line of the close marker (`// @@`).  `None` if the file ended
    /// before the close marker was seen (malformed input).
    def_end: Option<usize>,
}

impl ChunkDef {
    fn new(base_indent: usize, file_idx: usize, line: usize) -> Self {
        Self {
            content: Vec::new(),
            base_indent,
            file_idx,
            line,
            def_end: None,
        }
    }
}
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct ChunkLocation {
    pub file_idx: usize,
    pub line: usize,
}

#[derive(Debug, Error)]
pub enum ChunkError {
    #[error("{file_name} line {}: maximum recursion depth exceeded while expanding chunk '{chunk}'", .location.line + 1)]
    RecursionLimit {
        chunk: String,
        file_name: String,
        location: ChunkLocation,
    },
    #[error("{file_name} line {}: recursive reference detected in chunk '{chunk}' (cycle: {})", .location.line + 1, .cycle.join(" -> "))]
    RecursiveReference {
        chunk: String,
        cycle: Vec<String>,
        file_name: String,
        location: ChunkLocation,
    },
    #[error("{file_name} line {}: referenced chunk '{chunk}' is undefined", .location.line + 1)]
    UndefinedChunk {
        chunk: String,
        file_name: String,
        location: ChunkLocation,
    },
    #[error("I/O error: {0}")]
    IoError(#[from] io::Error),
    #[error("{file_name} line {}: file chunk '{file_chunk}' is already defined (use @replace to redefine)", .location.line + 1)]
    FileChunkRedefinition {
        file_chunk: String,
        file_name: String,
        location: ChunkLocation,
    },
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
    /// Byte sequence of the open delimiter — used as a fast pre-filter
    /// before running `open_re` or `slot_re`.  A line without these bytes
    /// cannot contain a chunk marker.
    open_bytes: Box<[u8]>,
    /// Byte sequence of the chunk-end marker — used as a fast pre-filter
    /// before running `close_re`.
    close_bytes: Box<[u8]>,
    file_names: Vec<String>,
    /// When `true`, referencing an undefined chunk is a fatal error
    /// and `@file` redefinition without `@replace` is also a fatal error.
    /// Default `false`: undefined chunks expand to nothing; redefinitions warn.
    pub strict_undefined: bool,
    /// Errors accumulated during `read()` that are promoted to hard errors
    /// when `strict_undefined` is `true`.  Checked by `Clip::write_files`.
    pub parse_errors: Vec<ChunkError>,
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
            open_bytes: open_delim.as_bytes().into(),
            close_bytes: chunk_end.as_bytes().into(),
            file_names: Vec::new(),
            strict_undefined: false,
            parse_errors: Vec::new(),
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
            let bytes = line.as_bytes();
            // Fast reject: skip regex entirely when the open-delimiter bytes
            // are absent.  memmem uses SIMD and is much cheaper than the regex
            // NFA for the common case where most lines are plain prose or code.
            if memchr::memmem::find(bytes, &self.open_bytes).is_some() {
            } else {
                // No open delimiter — can only be a close marker or content.
                if memchr::memmem::find(bytes, &self.close_bytes).is_some()
                    && self.close_re.is_match(line)
                {
                    if let Some((ref cname, idx)) = current_chunk
                        && let Some(chunk) = self.chunks.get_mut(cname)
                        && let Some(def) = chunk.definitions.get_mut(idx)
                    {
                        def.def_end = Some(line_no);
                    }
                    current_chunk = None;
                } else if let Some((ref cname, idx)) = current_chunk
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
                continue;
            }
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
                            let err = ChunkError::FileChunkRedefinition {
                                file_chunk: full_name.clone(),
                                file_name: self
                                    .file_names
                                    .get(file_idx)
                                    .cloned()
                                    .unwrap_or_default(),
                                location,
                            };
                            if self.strict_undefined {
                                self.parse_errors.push(err);
                            } else {
                                eprintln!("{}", err);
                            }
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

            if memchr::memmem::find(bytes, &self.close_bytes).is_some()
                && self.close_re.is_match(line)
            {
                if let Some((ref cname, idx)) = current_chunk
                    && let Some(chunk) = self.chunks.get_mut(cname)
                    && let Some(def) = chunk.definitions.get_mut(idx)
                {
                    def.def_end = Some(line_no);
                }
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
/// Mutable state threaded through the recursive chunk expansion.
struct ExpandState {
    seen: HashSet<String>,
    /// Call stack (chunk names in descent order).  Its length is the current
    /// recursion depth, replacing a separate `depth` counter.
    stack: Vec<String>,
    referenced_chunks: HashSet<String>,
    /// Direct dependency edges collected during expansion:
    /// `(from_chunk, to_chunk, src_file)`.  Deduplicated via HashSet.
    deps: HashSet<(String, String, String)>,
}

impl ExpandState {
    fn new() -> Self {
        Self {
            seen: HashSet::new(),
            stack: Vec::new(),
            referenced_chunks: HashSet::new(),
            deps: HashSet::new(),
        }
    }
}

/// Return type of `expand_with_map`: expanded lines, source-map entries,
/// referenced chunk names, and direct dependency edges.
type ExpandResult = (Vec<String>, Vec<NowebMapEntry>, HashSet<String>, Vec<(String, String, String)>);

impl ChunkStore {
    fn expand_inner(
        &self,
        chunk_name: &str,
        target_indent: &str,
        state: &mut ExpandState,
        reference_location: ChunkLocation,
        reversed_mode: bool,
    ) -> Result<Vec<(String, NowebMapEntry)>, ChunkError> {
        if state.stack.len() > weaveback_core::MAX_RECURSION_DEPTH {
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

        if state.seen.contains(chunk_name) {
            let file_name = self
                .file_names
                .get(reference_location.file_idx)
                .cloned()
                .unwrap_or_default();
            let mut cycle = state.stack.clone();
            cycle.push(chunk_name.to_string());
            return Err(ChunkError::RecursiveReference {
                chunk: chunk_name.to_string(),
                cycle,
                file_name,
                location: reference_location,
            });
        }

        if !self.chunks.contains_key(chunk_name) {
            if self.strict_undefined {
                let file_name = self
                    .file_names
                    .get(reference_location.file_idx)
                    .cloned()
                    .unwrap_or_default();
                return Err(ChunkError::UndefinedChunk {
                    chunk: chunk_name.to_string(),
                    file_name,
                    location: reference_location,
                });
            }
            return Ok(Vec::new());
        }

        state.referenced_chunks.insert(chunk_name.to_string());

        let chunk = self.chunks.get(chunk_name)
            .expect("internal invariant: chunk exists after contains_key check");
        let defs = &chunk.definitions;

        // Collect indices so we can reverse without a Box<dyn Iterator>.
        let indices: Vec<usize> = if reversed_mode {
            (0..defs.len()).rev().collect()
        } else {
            (0..defs.len()).collect()
        };

        state.seen.insert(chunk_name.to_string());
        state.stack.push(chunk_name.to_string());
        let mut result = Vec::new();

        for def_idx in indices {
            let def = &defs[def_idx];
            let src_file = self
                .file_names
                .get(def.file_idx)
                .cloned()
                .unwrap_or_default();

            for (line_count, line) in def.content.iter().enumerate() {
                if let Some(caps) = memchr::memmem::find(line.as_bytes(), &self.open_bytes)
                    .and_then(|_| self.slot_re.captures(line))
                {
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

                    // Record the direct dependency edge before recursing.
                    state.deps.insert((
                        chunk_name.to_string(),
                        referenced_chunk.trim().to_string(),
                        src_file.clone(),
                    ));

                    let expanded = self.expand_inner(
                        referenced_chunk.trim(),
                        &new_indent,
                        state,
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
                        confidence: Confidence::Exact,
                    };
                    result.push((out_line, entry));
                }
            }
        }

        state.stack.pop();
        state.seen.remove(chunk_name);
        Ok(result)
    }

    pub fn expand_with_map(
        &self,
        chunk_name: &str,
        indent: &str,
    ) -> Result<ExpandResult, ChunkError> {
        let mut state = ExpandState::new();
        let loc = ChunkLocation { file_idx: 0, line: 0 };
        let pairs = self.expand_inner(chunk_name, indent, &mut state, loc, false)?;
        let (lines, entries) = pairs.into_iter().unzip();
        let deps: Vec<_> = state.deps.into_iter().collect();
        Ok((lines, entries, state.referenced_chunks, deps))
    }

    pub fn expand(&self, chunk_name: &str, indent: &str) -> Result<Vec<String>, ChunkError> {
        let (lines, _, _, _) = self.expand_with_map(chunk_name, indent)?;
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

    /// Return a `ChunkDefEntry` for every chunk definition that has a recorded
    /// close-marker line.  Definitions where `def_end` is `None` (file ended
    /// without a close marker) are silently skipped.
    pub fn chunk_defs(&self) -> Vec<ChunkDefEntry> {
        let mut out = Vec::new();
        for (chunk_name, named_chunk) in &self.chunks {
            for (nth, def) in named_chunk.definitions.iter().enumerate() {
                let Some(def_end_0) = def.def_end else { continue };
                let src_file = self
                    .file_names
                    .get(def.file_idx)
                    .cloned()
                    .unwrap_or_default();
                out.push(ChunkDefEntry {
                    src_file,
                    chunk_name: chunk_name.clone(),
                    nth: nth as u32,
                    def_start: (def.line + 1) as u32,
                    def_end:   (def_end_0 + 1) as u32,
                });
            }
        }
        out
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

    /// Write a single `@file` chunk.  Returns the final on-disk bytes (after
    /// any configured formatter has run) so the caller can use them for
    /// source-map remapping without an extra read.  Returns `None` for
    /// absolute-path chunks (written directly, not through `SafeFileWriter`).
    pub fn write_chunk(
        &mut self,
        chunk_name: &str,
        content: &[String],
    ) -> Result<Option<Vec<u8>>, WeavebackError> {
        if !chunk_name.starts_with("@file ") {
            return Ok(None);
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
            Ok(None)
        } else {
            let final_path = self.safe_file_writer.before_write(path_str)?;
            let mut f = fs::File::create(&final_path)?;
            for line in content {
                f.write_all(line.as_bytes())?;
            }
            let written = self.safe_file_writer.after_write(path_str)?;
            Ok(Some(written))
        }
    }
}
/// Normalise a source line for content-hash matching:
/// strip leading/trailing whitespace and drop any trailing `//` comment.
fn normalise_for_hash(line: &str) -> &str {
    let trimmed = line.trim();
    // Drop inline // comment (not inside strings — good enough for heuristics).
    if let Some(pos) = trimmed.find("//") {
        trimmed[..pos].trim_end()
    } else {
        trimmed
    }
}

fn remap_noweb_entries(
    pre_lines: &[String],
    post_content: &str,
    entries: Vec<NowebMapEntry>,
) -> Vec<(u32, NowebMapEntry)> {
    use similar::{ChangeTag, TextDiff};
    use std::collections::{HashMap, HashSet};

    // Pre-normalise both sides once so all tiers can reuse the slices.
    let pre_norm: Vec<&str> = pre_lines.iter().map(|l| normalise_for_hash(l)).collect();
    let post_lines_vec: Vec<&str> = post_content.lines().collect();
    let post_norm: Vec<&str> = post_lines_vec.iter().map(|l| normalise_for_hash(l)).collect();
    let post_line_count = post_lines_vec.len();

    // --- Tier 1: diff-based exact mapping ---
    let pre_content: String = pre_lines.concat();
    let diff = TextDiff::from_lines(pre_content.as_str(), post_content);

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
            ChangeTag::Delete => { old_idx += 1; }
            ChangeTag::Insert => { new_idx += 1; }
        }
    }

    let mut new_to_entry: Vec<Option<NowebMapEntry>> = vec![None; post_line_count];
    for (old_i, entry) in entries.iter().enumerate() {
        if let Some(&Some(new_i)) = old_to_new.get(old_i)
            && new_i < post_line_count
        {
            new_to_entry[new_i] = Some(entry.clone()); // Confidence::Exact from expand_inner
        }
    }

    // --- Tier 2: contextual content-hash fallback ---
    // Key = (prev_norm, curr_norm, next_norm).  Three-line context prevents
    // false matches on trivial lines ({, }, etc.).
    //
    // We store *all* candidate old indices per key (Vec<usize>) so that when
    // multiple pre-formatter lines share the same context triple (e.g. two
    // identical import lines in the same chunk), we pick the *closest unused*
    // one to the new_i position rather than arbitrarily using the last.
    //
    // Chunk-aware ambiguity rejection: if the same context triple spans lines
    // from *different* chunks, the key is discarded entirely — a cross-chunk
    // false match is worse than no match.
    type CtxKey<'a> = (&'a str, &'a str, &'a str);
    let mut hash_to_old: HashMap<CtxKey<'_>, Vec<usize>> = HashMap::new();
    let mut ambiguous: HashSet<CtxKey<'_>> = HashSet::new();

    for old_i in 0..pre_norm.len() {
        let curr = pre_norm[old_i];
        if curr.len() <= 1 { continue; }
        let prev = if old_i > 0 { pre_norm[old_i - 1] } else { "" };
        let next = pre_norm.get(old_i + 1).copied().unwrap_or("");
        let key: CtxKey<'_> = (prev, curr, next);
        if ambiguous.contains(&key) { continue; }
        if let Some(existing) = hash_to_old.get(&key) {
            // If any existing candidate is from a different chunk, discard.
            let first_chunk = &entries[existing[0]].chunk_name;
            if entries[old_i].chunk_name != *first_chunk {
                hash_to_old.remove(&key);
                ambiguous.insert(key);
                continue;
            }
        }
        hash_to_old.entry(key).or_default().push(old_i);
    }

    // Pre-claim lines already placed by tier 1.
    let mut claimed: HashSet<usize> = old_to_new.iter()
        .enumerate()
        .filter_map(|(i, m)| m.map(|_| i))
        .collect();

    for new_i in 0..post_line_count {
        if new_to_entry[new_i].is_some() { continue; }
        let curr = post_norm[new_i];
        if curr.len() <= 1 { continue; }
        let prev = if new_i > 0 { post_norm[new_i - 1] } else { "" };
        let next = post_norm.get(new_i + 1).copied().unwrap_or("");
        let key: CtxKey<'_> = (prev, curr, next);
        if let Some(candidates) = hash_to_old.get(&key) {
            // Pick the unclaimed candidate whose old position is closest to new_i.
            let best = candidates.iter()
                .filter(|&&old_i| !claimed.contains(&old_i))
                .min_by_key(|&&old_i| (new_i as isize - old_i as isize).abs());
            if let Some(&old_i) = best {
                claimed.insert(old_i);
                let mut entry = entries[old_i].clone();
                entry.confidence = Confidence::HashMatch;
                new_to_entry[new_i] = Some(entry);
            }
        }
    }

    // --- Tier 3: bidirectional nearest-neighbour fill (Confidence::Inferred) ---
    // Forward pass.
    let mut last: Option<NowebMapEntry> = None;
    for slot in new_to_entry.iter_mut() {
        if slot.is_some() {
            last = slot.clone();
        } else if let Some(ref src) = last {
            let mut e = src.clone();
            e.confidence = Confidence::Inferred;
            *slot = Some(e);
        }
    }
    // Backward pass: fill remaining gaps (leading insertions).
    let mut next: Option<NowebMapEntry> = None;
    for slot in new_to_entry.iter_mut().rev() {
        if slot.is_some() {
            next = slot.clone();
        } else if let Some(ref src) = next {
            let mut e = src.clone();
            e.confidence = Confidence::Inferred;
            *slot = Some(e);
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

    /// Control whether referencing an undefined chunk is a fatal error (`true`)
    /// or silently expands to nothing (`false`, the default).
    pub fn set_strict_undefined(&mut self, strict: bool) {
        self.store.strict_undefined = strict;
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
/// Verify that `texts` tangle without errors.
///
/// Each element of `texts` is a `(source_text, filename)` pair — the same
/// inputs you would pass to [`Clip::read`].  Every `@file` chunk is expanded
/// in memory; no filesystem I/O is performed.
///
/// Returns a map from output file path (relative to `gen/`) to its expanded
/// lines on success, or the first expansion error encountered.
pub fn tangle_check(
    texts: &[(&str, &str)],
    open_delim: &str,
    close_delim: &str,
    chunk_end: &str,
    comment_markers: &[String],
) -> Result<HashMap<String, Vec<String>>, WeavebackError> {
    let mut store = ChunkStore::new(open_delim, close_delim, chunk_end, comment_markers);
    for (text, fname) in texts {
        let idx = store.add_file_name(fname);
        store.read(text, idx);
    }
    let mut out = HashMap::new();
    for name in store.get_file_chunks() {
        let lines = store.expand(name, "")?;
        let out_name = name.strip_prefix("@file ").unwrap_or(name).trim().to_string();
        out.insert(out_name, lines);
    }
    Ok(out)
}
impl Clip {
    /// Write all `@file` chunks, skipping those whose name is in `skip`.
    /// Skipped chunks do not expand or write; the source-map and chunk_deps
    /// entries for them are not updated (the previous run's entries remain).
    /// Chunk definitions for all chunks (including skipped ones) are still
    /// recorded so `weaveback serve` navigation stays accurate.
    pub fn write_files_incremental(
        &mut self,
        skip: &std::collections::HashSet<String>,
    ) -> Result<(), WeavebackError> {
        if self.store.strict_undefined && !self.store.parse_errors.is_empty() {
            return Err(WeavebackError::Chunk(
                self.store.parse_errors.remove(0),
            ));
        }
        let fc = self.store.get_file_chunks().to_vec();
        let mut all_referenced = HashSet::new();
        for name in &fc {
            if skip.contains(name) {
                continue;
            }
            let (lines, map_entries, referenced, deps) = self.store.expand_with_map(name, "")?;
            all_referenced.extend(referenced);

            let mut cw = ChunkWriter::new(&mut self.writer);
            let written_bytes = cw.write_chunk(name, &lines)?;

            let out_file = name.strip_prefix("@file ").unwrap_or(name).trim();

            let keyed = if let Some(bytes) = written_bytes {
                let formatted = String::from_utf8_lossy(&bytes);
                let pre_content: String = lines.concat();
                if formatted.as_ref() != pre_content {
                    remap_noweb_entries(&lines, formatted.as_ref(), map_entries)
                } else {
                    map_entries.into_iter().enumerate()
                        .map(|(i, e)| (i as u32, e)).collect()
                }
            } else {
                map_entries.into_iter().enumerate()
                    .map(|(i, e)| (i as u32, e)).collect()
            };

            self.writer
                .db_mut()
                .set_noweb_entries(out_file, &keyed)
                .map_err(|e| WeavebackError::SafeWriter(SafeWriterError::DbError(e)))?;

            self.writer
                .db_mut()
                .set_chunk_deps(&deps)
                .map_err(|e| WeavebackError::SafeWriter(SafeWriterError::DbError(e)))?;
        }
        let chunk_def_entries = self.store.chunk_defs();
        self.writer
            .db_mut()
            .set_chunk_defs(&chunk_def_entries)
            .map_err(|e| WeavebackError::SafeWriter(SafeWriterError::DbError(e)))?;

        let warns = self.store.check_unused_chunks(&all_referenced);
        for w in warns {
            eprintln!("{}", w);
        }
        Ok(())
    }

    pub fn write_files(&mut self) -> Result<(), WeavebackError> {
        // In strict mode, promote any parse-time errors (e.g. @file redefinition)
        // to hard errors before writing anything.
        if self.store.strict_undefined && !self.store.parse_errors.is_empty() {
            return Err(WeavebackError::Chunk(
                self.store.parse_errors.remove(0),
            ));
        }
        let fc = self.store.get_file_chunks().to_vec();
        let mut all_referenced = HashSet::new();
        for name in &fc {
            let (lines, map_entries, referenced, deps) = self.store.expand_with_map(name, "")?;
            all_referenced.extend(referenced);

            let mut cw = ChunkWriter::new(&mut self.writer);
            let written_bytes = cw.write_chunk(name, &lines)?;

            let out_file = name.strip_prefix("@file ").unwrap_or(name).trim();

            // After formatting, re-key map entries to post-formatter lines.
            // Use the bytes already returned by write_chunk — no second disk read.
            let keyed = if let Some(bytes) = written_bytes {
                let formatted = String::from_utf8_lossy(&bytes);
                let pre_content: String = lines.concat();
                if formatted.as_ref() != pre_content {
                    remap_noweb_entries(&lines, formatted.as_ref(), map_entries)
                } else {
                    map_entries.into_iter().enumerate()
                        .map(|(i, e)| (i as u32, e)).collect()
                }
            } else {
                map_entries.into_iter().enumerate()
                    .map(|(i, e)| (i as u32, e)).collect()
            };

            self.writer
                .db_mut()
                .set_noweb_entries(out_file, &keyed)
                .map_err(|e| WeavebackError::SafeWriter(SafeWriterError::DbError(e)))?;

            self.writer
                .db_mut()
                .set_chunk_deps(&deps)
                .map_err(|e| WeavebackError::SafeWriter(SafeWriterError::DbError(e)))?;
        }
        // Persist chunk definition line ranges for `weaveback serve` navigation.
        let chunk_def_entries = self.store.chunk_defs();
        self.writer
            .db_mut()
            .set_chunk_defs(&chunk_def_entries)
            .map_err(|e| WeavebackError::SafeWriter(SafeWriterError::DbError(e)))?;

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

    pub fn db_mut(&mut self) -> &mut crate::db::WeavebackDb {
        self.writer.db_mut()
    }

    pub fn finish(self, target: &Path) -> Result<(), WeavebackError> {
        self.writer.finish(target).map_err(WeavebackError::SafeWriter)
    }
}
