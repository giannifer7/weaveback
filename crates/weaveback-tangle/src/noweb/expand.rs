// weaveback-tangle/src/noweb/expand.rs
// I'd Really Rather You Didn't edit this generated file.

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

#[derive(Debug, Clone, Copy, Default)]
struct RefOptions {
    reversed: bool,
    compact: bool,
    tight: bool,
}

fn trim_blank_edge_lines(lines: Vec<(String, NowebMapEntry)>) -> Vec<(String, NowebMapEntry)> {
    let start = lines
        .iter()
        .position(|(line, _)| !line.trim().is_empty())
        .unwrap_or(lines.len());
    let end = lines
        .iter()
        .rposition(|(line, _)| !line.trim().is_empty())
        .map(|idx| idx + 1)
        .unwrap_or(start);
    lines.into_iter().skip(start).take(end.saturating_sub(start)).collect()
}

fn drop_blank_only_lines(lines: Vec<(String, NowebMapEntry)>) -> Vec<(String, NowebMapEntry)> {
    lines
        .into_iter()
        .filter(|(line, _)| !line.trim().is_empty())
        .collect()
}

fn apply_ref_space_options(
    lines: Vec<(String, NowebMapEntry)>,
    options: RefOptions,
) -> Vec<(String, NowebMapEntry)> {
    let mut lines = lines;
    if options.compact || options.tight {
        lines = trim_blank_edge_lines(lines);
    }
    if options.tight {
        lines = drop_blank_only_lines(lines);
    }
    lines
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
        options: RefOptions,
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
        let indices: Vec<usize> = if options.reversed {
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
            let mut def_result = Vec::new();

            for (line_count, line) in def.content.iter().enumerate() {
                if let Some(slot_match) = self.syntax.parse_reference_line(line) {
                    let add_indent = slot_match.add_indent.as_str();
                    let modifier = slot_match.modifier.as_str();
                    let referenced_chunk = slot_match.referenced_chunk.as_str();

                    let child_options = RefOptions {
                        reversed: modifier.contains("@reversed"),
                        compact: modifier.contains("@compact"),
                        tight: modifier.contains("@tight"),
                    };
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
                        child_options,
                    )?;
                    def_result.extend(apply_ref_space_options(expanded, child_options));
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
                    def_result.push((out_line, entry));
                }
            }
            result.extend(apply_ref_space_options(def_result, options));
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
        let pairs = self.expand_inner(
            chunk_name,
            indent,
            &mut state,
            loc,
            RefOptions::default(),
        )?;
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

