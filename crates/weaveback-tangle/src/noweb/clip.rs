// weaveback-tangle/src/noweb/clip.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;

pub struct Clip {
    pub(super) store: ChunkStore,
    pub(super) writer: SafeFileWriter,
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

    /// Control whether unused-chunk warnings are emitted (`true`) or
    /// suppressed (`false`, the default).  Opt in with `--warn-unused`.
    pub fn set_warn_unused(&mut self, warn: bool) {
        self.store.warn_unused = warn;
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

