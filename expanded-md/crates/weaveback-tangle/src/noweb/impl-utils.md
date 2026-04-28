# Noweb Store Utilities

ChunkStore query and warning helpers.

### Utilities

After all `@file` chunks are written, `check_unused_chunks` warns about named
chunks that were defined but never referenced — a common mistake when
refactoring literate sources.

```rust
// <[noweb-chunkstore-utils]>=
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
// @
```

