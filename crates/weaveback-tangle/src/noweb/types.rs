// weaveback-tangle/src/noweb/types.rs
// I'd Really Rather You Didn't edit this generated file.

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkDefinitionMatch {
    pub indent_len: usize,
    pub base_name: String,
    pub is_replace: bool,
    pub is_file: bool,
}

#[derive(Debug, Clone)]
struct ChunkReferenceMatch {
    add_indent: String,
    modifier: String,
    referenced_chunk: String,
}

#[derive(Debug, Clone)]
pub struct NowebSyntax {
    open_re: Regex,
    slot_re: Regex,
    close_re: Regex,
    open_bytes: Box<[u8]>,
    close_bytes: Box<[u8]>,
}

impl NowebSyntax {
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
            r"^(?P<indent>\s*)(?:{})?[ \t]*{}(?P<replace>@replace[ \t]+)?(?P<file>@file[ \t]+)?(?P<name>.+?){}=[ \t]*$",
            escaped_comments, od, cd
        );
        let slot_pattern = format!(
            r"^(\s*)(?:{})?\s*{}((?:(?:@file|@reversed|@compact|@tight)\s+)*)?(.+?){}\s*$",
            escaped_comments, od, cd
        );
        let close_pattern = format!(
            r"^(?:{})?[ \t]*{}\s*$",
            escaped_comments,
            regex::escape(chunk_end)
        );

        Self {
            open_re: Regex::new(&open_pattern).expect("Invalid open pattern"),
            slot_re: Regex::new(&slot_pattern).expect("Invalid slot pattern"),
            close_re: Regex::new(&close_pattern).expect("Invalid close pattern"),
            open_bytes: open_delim.as_bytes().into(),
            close_bytes: chunk_end.as_bytes().into(),
        }
    }

    pub fn parse_definition_line(&self, line: &str) -> Option<ChunkDefinitionMatch> {
        memchr::memmem::find(line.as_bytes(), &self.open_bytes)?;
        let caps = self.open_re.captures(line)?;
        Some(ChunkDefinitionMatch {
            indent_len: caps.name("indent").map_or("", |m| m.as_str()).len(),
            base_name: caps.name("name").map_or("", |m| m.as_str()).to_string(),
            is_replace: caps.name("replace").is_some(),
            is_file: caps.name("file").is_some(),
        })
    }

    pub fn is_close_line(&self, line: &str) -> bool {
        memchr::memmem::find(line.as_bytes(), &self.close_bytes).is_some()
            && self.close_re.is_match(line)
    }

    fn parse_reference_line(&self, line: &str) -> Option<ChunkReferenceMatch> {
        memchr::memmem::find(line.as_bytes(), &self.open_bytes)?;
        let caps = self.slot_re.captures(line)?;
        Some(ChunkReferenceMatch {
            add_indent: caps.get(1).map_or("", |m| m.as_str()).to_string(),
            modifier: caps.get(2).map_or("", |m| m.as_str()).to_string(),
            referenced_chunk: caps.get(3).map_or("", |m| m.as_str()).to_string(),
        })
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

