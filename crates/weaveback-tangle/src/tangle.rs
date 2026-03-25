use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use std::path::Path;

const CREATE_SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS gen_baselines (
    path    TEXT PRIMARY KEY NOT NULL,
    content BLOB NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS noweb_map (
    out_file   TEXT    NOT NULL,
    out_line   INTEGER NOT NULL,
    src_file   TEXT    NOT NULL,
    chunk_name TEXT    NOT NULL,
    src_line   INTEGER NOT NULL,
    indent     TEXT    NOT NULL,
    PRIMARY KEY (out_file, out_line)
) STRICT;

CREATE TABLE IF NOT EXISTS macro_map (
    driver_file   TEXT    NOT NULL,
    expanded_line INTEGER NOT NULL,
    data          BLOB    NOT NULL,
    PRIMARY KEY (driver_file, expanded_line)
) STRICT;

CREATE TABLE IF NOT EXISTS src_snapshots (
    path    TEXT PRIMARY KEY NOT NULL,
    content BLOB NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS var_defs (
    var_name TEXT    NOT NULL,
    src_file TEXT    NOT NULL,
    pos      INTEGER NOT NULL,
    length   INTEGER NOT NULL,
    PRIMARY KEY (var_name, src_file, pos)
) STRICT;

CREATE TABLE IF NOT EXISTS macro_defs (
    macro_name TEXT    NOT NULL,
    src_file   TEXT    NOT NULL,
    pos        INTEGER NOT NULL,
    length     INTEGER NOT NULL,
    PRIMARY KEY (macro_name, src_file, pos)
) STRICT;
";
#[derive(Debug)]
pub enum DbError {
    Sql(rusqlite::Error),
}

impl std::fmt::Display for DbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DbError::Sql(e) => write!(f, "database error: {e}"),
        }
    }
}

impl std::error::Error for DbError {}

impl From<rusqlite::Error> for DbError {
    fn from(e: rusqlite::Error) -> Self {
        DbError::Sql(e)
    }
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct NowebMapEntry {
    pub src_file: String,
    pub chunk_name: String,
    pub src_line: u32,
    pub indent: String,
}
pub struct WeavebackDb {
    conn: Connection,
}

fn apply_schema(conn: &Connection) -> Result<(), DbError> {
    conn.execute_batch(CREATE_SCHEMA).map_err(DbError::Sql)
}

impl WeavebackDb {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, DbError> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        apply_schema(&conn)?;
        Ok(Self { conn })
    }

    pub fn open_read_only<P: AsRef<Path>>(path: P) -> Result<Self, DbError> {
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        Ok(Self { conn })
    }

    pub fn open_temp() -> Result<Self, DbError> {
        let conn = Connection::open_in_memory()?;
        apply_schema(&conn)?;
        Ok(Self { conn })
    }
}
impl WeavebackDb {
    pub fn get_baseline(&self, path: &str) -> Result<Option<Vec<u8>>, DbError> {
        Ok(self
            .conn
            .query_row(
                "SELECT content FROM gen_baselines WHERE path = ?1",
                params![path],
                |row| row.get(0),
            )
            .optional()?)
    }

    pub fn list_baselines(&self) -> Result<Vec<(String, Vec<u8>)>, DbError> {
        let mut stmt = self
            .conn
            .prepare("SELECT path, content FROM gen_baselines")?;
        let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?
        )
    }

    pub fn set_baseline(&self, path: &str, content: &[u8]) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT OR REPLACE INTO gen_baselines (path, content) VALUES (?1, ?2)",
            params![path, content],
        )?;
        Ok(())
    }
}
impl WeavebackDb {
    pub fn set_noweb_entries(
        &self,
        out_file: &str,
        entries: &[(u32, NowebMapEntry)],
    ) -> Result<(), DbError> {
        if entries.is_empty() {
            return Ok(());
        }
        let tx = self.conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare_cached(
                "INSERT OR REPLACE INTO noweb_map
                 (out_file, out_line, src_file, chunk_name, src_line, indent)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?;
            for (line, e) in entries {
                stmt.execute(params![
                    out_file,
                    *line,
                    e.src_file,
                    e.chunk_name,
                    e.src_line,
                    e.indent
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn get_noweb_entry(
        &self,
        out_file: &str,
        out_line: u32,
    ) -> Result<Option<NowebMapEntry>, DbError> {
        Ok(self
            .conn
            .query_row(
                "SELECT src_file, chunk_name, src_line, indent
                 FROM noweb_map WHERE out_file = ?1 AND out_line = ?2",
                params![out_file, out_line],
                |row| {
                    Ok(NowebMapEntry {
                        src_file: row.get(0)?,
                        chunk_name: row.get(1)?,
                        src_line: row.get::<_, u32>(2)?,
                        indent: row.get(3)?,
                    })
                },
            )
            .optional()?)
    }
}
impl WeavebackDb {
    pub fn set_macro_map_entries(
        &self,
        driver_file: &str,
        entries: &[(u32, Vec<u8>)],
    ) -> Result<(), DbError> {
        if entries.is_empty() {
            return Ok(());
        }
        let tx = self.conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare_cached(
                "INSERT OR REPLACE INTO macro_map (driver_file, expanded_line, data)
                 VALUES (?1, ?2, ?3)",
            )?;
            for (line, bytes) in entries {
                stmt.execute(params![driver_file, *line, bytes.as_slice()])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn get_macro_map_bytes(
        &self,
        driver_file: &str,
        expanded_line: u32,
    ) -> Result<Option<Vec<u8>>, DbError> {
        Ok(self
            .conn
            .query_row(
                "SELECT data FROM macro_map
                 WHERE driver_file = ?1 AND expanded_line = ?2",
                params![driver_file, expanded_line],
                |row| row.get(0),
            )
            .optional()?)
    }
}
/// Escape a string for use inside a SQLite single-quoted string literal.
fn sqlite_string_literal(s: &str) -> String {
    s.replace('\'', "''")
}

impl WeavebackDb {
    pub fn merge_into(&self, target_path: &Path) -> Result<(), DbError> {
        {
            let t = Connection::open(target_path)?;
            t.busy_timeout(std::time::Duration::from_millis(200))?;
            t.pragma_update(None, "journal_mode", "WAL")?;
            t.pragma_update(None, "synchronous", "NORMAL")?;
            apply_schema(&t)?;
        }

        self.conn.busy_timeout(std::time::Duration::from_millis(200))?;
        let target_str = target_path.to_string_lossy();
        let escaped = sqlite_string_literal(&target_str);
        self.conn
            .execute_batch(&format!("ATTACH DATABASE '{escaped}' AS target"))?;

        let result = self.conn.execute_batch(
            "BEGIN;
             INSERT OR REPLACE INTO target.gen_baselines SELECT * FROM gen_baselines;
             INSERT OR REPLACE INTO target.noweb_map     SELECT * FROM noweb_map;
             INSERT OR REPLACE INTO target.macro_map     SELECT * FROM macro_map;
             INSERT OR REPLACE INTO target.src_snapshots SELECT * FROM src_snapshots;
             INSERT OR REPLACE INTO target.var_defs      SELECT * FROM var_defs;
             INSERT OR REPLACE INTO target.macro_defs    SELECT * FROM macro_defs;
             COMMIT;",
        );

        // Always detach, even on error.
        let _ = self.conn.execute_batch("DETACH DATABASE target");
        result?;
        Ok(())
    }
}
impl WeavebackDb {
    pub fn get_src_snapshot(&self, path: &str) -> Result<Option<Vec<u8>>, DbError> {
        Ok(self
            .conn
            .query_row(
                "SELECT content FROM src_snapshots WHERE path = ?1",
                params![path],
                |row| row.get(0),
            )
            .optional()?)
    }

    pub fn list_src_snapshots(&self) -> Result<Vec<(String, Vec<u8>)>, DbError> {
        let mut stmt = self
            .conn
            .prepare("SELECT path, content FROM src_snapshots")?;
        let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?
        )
    }

    pub fn set_src_snapshot(&self, path: &str, content: &[u8]) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT OR REPLACE INTO src_snapshots (path, content) VALUES (?1, ?2)",
            params![path, content],
        )?;
        Ok(())
    }

    pub fn record_var_def(
        &self,
        var_name: &str,
        src_file: &str,
        pos: u32,
        length: u32,
    ) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT OR REPLACE INTO var_defs (var_name, src_file, pos, length)
             VALUES (?1, ?2, ?3, ?4)",
            params![var_name, src_file, pos, length],
        )?;
        Ok(())
    }

    pub fn query_var_defs(&self, var_name: &str) -> Result<Vec<(String, u32, u32)>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT src_file, pos, length FROM var_defs WHERE var_name = ?1",
        )?;
        let rows = stmt.query_map(params![var_name], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, u32>(1)?,
                row.get::<_, u32>(2)?,
            ))
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?
        )
    }

    pub fn record_macro_def(
        &self,
        macro_name: &str,
        src_file: &str,
        pos: u32,
        length: u32,
    ) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT OR REPLACE INTO macro_defs (macro_name, src_file, pos, length)
             VALUES (?1, ?2, ?3, ?4)",
            params![macro_name, src_file, pos, length],
        )?;
        Ok(())
    }

    pub fn query_macro_defs(
        &self,
        macro_name: &str,
    ) -> Result<Vec<(String, u32, u32)>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT src_file, pos, length FROM macro_defs WHERE macro_name = ?1",
        )?;
        let rows = stmt.query_map(params![macro_name], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, u32>(1)?,
                row.get::<_, u32>(2)?,
            ))
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?
        )
    }
}
pub mod db;
pub mod noweb;
pub mod safe_writer;

#[cfg(test)]
mod tests;

pub use noweb::ChunkError;

use db::DbError;
use safe_writer::SafeWriterError;
use std::fmt;

#[derive(Debug)]
pub enum WeavebackError {
    Chunk(ChunkError),
    SafeWriter(SafeWriterError),
    Db(DbError),
}

impl fmt::Display for WeavebackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WeavebackError::Chunk(e) => write!(f, "Chunk error: {}", e),
            WeavebackError::SafeWriter(e) => write!(f, "Safe writer error: {}", e),
            WeavebackError::Db(e) => write!(f, "Database error: {}", e),
        }
    }
}

impl std::error::Error for WeavebackError {}

impl From<ChunkError> for WeavebackError {
    fn from(err: ChunkError) -> Self {
        WeavebackError::Chunk(err)
    }
}

impl From<SafeWriterError> for WeavebackError {
    fn from(err: SafeWriterError) -> Self {
        WeavebackError::SafeWriter(err)
    }
}

impl From<DbError> for WeavebackError {
    fn from(err: DbError) -> Self {
        WeavebackError::Db(err)
    }
}

impl From<std::io::Error> for WeavebackError {
    fn from(err: std::io::Error) -> Self {
        WeavebackError::SafeWriter(SafeWriterError::IoError(err))
    }
}

pub use crate::db::{WeavebackDb, NowebMapEntry};
pub use crate::noweb::Clip;
pub use crate::safe_writer::SafeFileWriter;
pub use crate::safe_writer::SafeWriterConfig;
use weaveback_tangle::{WeavebackError, Clip, SafeFileWriter, SafeWriterConfig};
use clap::Parser;
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Write};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "weaveback",
    about = "Expand chunks like noweb - A literate programming tool",
    version
)]
struct Args {
    /// Output file for --chunks [default: stdout]
    #[arg(long)]
    output: Option<PathBuf>,

    /// Names of chunks to extract (comma separated)
    #[arg(long)]
    chunks: Option<String>,

    /// Base directory of generated files
    #[arg(long = "gen", default_value = "gen")]
    gen_dir: PathBuf,

    /// Delimiter used to open a chunk
    #[arg(long, default_value = "<[")]
    open_delim: String,

    /// Delimiter used to close a chunk definition
    #[arg(long, default_value = "]>")]
    close_delim: String,

    /// Delimiter for chunk-end lines
    #[arg(long, default_value = "@")]
    chunk_end: String,

    /// Comment markers (comma separated)
    #[arg(long, default_value = "#,//")]
    comment_markers: String,

    /// Formatter command per file extension, e.g. --formatter rs=rustfmt
    /// Can be repeated: --formatter rs=rustfmt --formatter ts="prettier --write"
    #[arg(long, value_name = "EXT=CMD")]
    formatter: Vec<String>,

    /// Allow @file ~/... chunks to write outside the gen/ directory
    #[arg(long)]
    allow_home: bool,

    /// Show what would be written without writing anything
    #[arg(long)]
    dry_run: bool,

    /// Input files (use - for stdin)
    #[arg(required = true)]
    files: Vec<PathBuf>,
}

fn write_chunks<W: Write>(
    clipper: &mut Clip,
    chunks: &[&str],
    writer: &mut W,
) -> Result<(), WeavebackError> {
    for chunk in chunks {
        clipper.get_chunk(chunk, writer)?;
        writeln!(writer)?;
    }
    Ok(())
}

fn run(args: Args) -> Result<(), WeavebackError> {
    let comment_markers: Vec<String> = args
        .comment_markers
        .split(',')
        .map(|s| s.trim().to_string())
        .collect();

    let formatters: HashMap<String, String> = args
        .formatter
        .iter()
        .filter_map(|s| {
            s.split_once('=')
                .map(|(e, c)| (e.to_string(), c.to_string()))
        })
        .collect();

    let safe_writer = SafeFileWriter::with_config(
        &args.gen_dir,
        SafeWriterConfig {
            formatters,
            allow_home: args.allow_home,
            ..SafeWriterConfig::default()
        },
    )?;
    let mut clipper = Clip::new(
        safe_writer,
        &args.open_delim,
        &args.close_delim,
        &args.chunk_end,
        &comment_markers,
    );

    clipper.read_files(&args.files)?;

    if args.dry_run {
        for path in clipper.list_output_files() {
            println!("{}", path.display());
        }
        return Ok(());
    }

    clipper.write_files()?;

    if let Some(chunks) = args.chunks {
        let chunks: Vec<&str> = chunks.split(',').collect();
        if let Some(output_path) = args.output {
            let mut file = File::create(output_path)?;
            write_chunks(&mut clipper, &chunks, &mut file)?;
        } else {
            let stdout = io::stdout();
            let mut handle = stdout.lock();
            write_chunks(&mut clipper, &chunks, &mut handle)?;
        }
    }

    clipper.finish(std::path::Path::new("weaveback.db"))?;

    Ok(())
}

fn main() {
    env_logger::init();
    let args = Args::parse();

    if let Err(e) = run(args) {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
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
use crate::db::{WeavebackDb, DbError};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Read;
use std::io::{self, BufReader};
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;

#[derive(Debug)]
pub enum SafeWriterError {
    IoError(io::Error),
    DirectoryCreationFailed(PathBuf),
    BackupFailed(PathBuf),
    ModifiedExternally(PathBuf),
    SecurityViolation(String),
    FormatterError(String),
    DbError(DbError),
}

impl std::fmt::Display for SafeWriterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SafeWriterError::IoError(e) => write!(f, "IO error: {}", e),
            SafeWriterError::DirectoryCreationFailed(path) => {
                write!(f, "Failed to create directory: {}", path.display())
            }
            SafeWriterError::BackupFailed(path) => {
                write!(f, "Failed to create backup for: {}", path.display())
            }
            SafeWriterError::ModifiedExternally(path) => {
                write!(f, "File was modified externally: {}", path.display())
            }
            SafeWriterError::SecurityViolation(msg) => write!(f, "Security violation: {}", msg),
            SafeWriterError::FormatterError(msg) => write!(f, "Formatter error: {}", msg),
            SafeWriterError::DbError(e) => write!(f, "Database error: {}", e),
        }
    }
}

impl std::error::Error for SafeWriterError {}

impl From<io::Error> for SafeWriterError {
    fn from(err: io::Error) -> Self {
        SafeWriterError::IoError(err)
    }
}

impl From<DbError> for SafeWriterError {
    fn from(err: DbError) -> Self {
        SafeWriterError::DbError(err)
    }
}
#[derive(Debug, Clone)]
pub struct SafeWriterConfig {
    pub buffer_size: usize,
    pub formatters: HashMap<String, String>, // file-extension → shell command
    /// Allow `@file ~/...` chunks to write outside the gen/ sandbox.
    /// Default `false`: tilde-expanded (absolute) paths are rejected unless
    /// the user explicitly passes `--allow-home`.
    pub allow_home: bool,
}

impl Default for SafeWriterConfig {
    fn default() -> Self {
        SafeWriterConfig {
            buffer_size: 8192,
            formatters: HashMap::new(),
            allow_home: false,
        }
    }
}
pub struct SafeFileWriter {
    gen_base: PathBuf,
    db: WeavebackDb,
    config: SafeWriterConfig,
    /// Staging area: logical file name → temp file on disk.
    /// The NamedTempFile is kept alive here until after_write consumes it.
    staging: HashMap<String, NamedTempFile>,
}

impl SafeFileWriter {
    pub fn new<P: AsRef<Path>>(gen_base: P) -> Result<Self, SafeWriterError> {
        Self::with_config(gen_base, SafeWriterConfig::default())
    }

    pub fn with_config<P: AsRef<Path>>(
        gen_base: P,
        config: SafeWriterConfig,
    ) -> Result<Self, SafeWriterError> {
        fs::create_dir_all(gen_base.as_ref())
            .map_err(|_| SafeWriterError::DirectoryCreationFailed(gen_base.as_ref().to_path_buf()))?;
        let gen_base = gen_base
            .as_ref()
            .canonicalize()
            .map_err(SafeWriterError::IoError)?;

        let db = WeavebackDb::open_temp().map_err(SafeWriterError::DbError)?;

        Ok(SafeFileWriter {
            gen_base,
            db,
            config,
            staging: HashMap::new(),
        })
    }
}
impl SafeFileWriter {
    fn atomic_copy<P: AsRef<Path>>(&self, source: P, destination: P) -> io::Result<()> {
        let destination = destination.as_ref();
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        let temp_path = destination.with_extension("tmp");

        if temp_path.exists() {
            let _ = fs::remove_file(&temp_path);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        {
            let mut source_file = fs::File::open(&source)?;
            let mut temp_file = fs::File::create(&temp_path)?;
            io::copy(&mut source_file, &mut temp_file)?;
            temp_file.sync_all()?;
        }

        std::thread::sleep(std::time::Duration::from_millis(10));
        fs::rename(temp_path, destination)?;
        Ok(())
    }

    fn copy_if_different<P: AsRef<Path>>(
        &self,
        source: P,
        destination: P,
    ) -> Result<(), SafeWriterError> {
        let source = source.as_ref();
        let destination = destination.as_ref();

        if !destination.exists() {
            return self
                .atomic_copy(source, destination)
                .map_err(SafeWriterError::from);
        }

        let are_different = {
            let mut source_file =
                BufReader::with_capacity(self.config.buffer_size, File::open(source)?);
            let mut dest_file =
                BufReader::with_capacity(self.config.buffer_size, File::open(destination)?);

            let mut src_buf = vec![0u8; self.config.buffer_size];
            let mut dst_buf = vec![0u8; self.config.buffer_size];
            loop {
                let src_n = source_file.read(&mut src_buf)?;
                let dst_n = dest_file.read(&mut dst_buf)?;
                if src_n != dst_n || src_buf[..src_n] != dst_buf[..dst_n] {
                    break true;
                }
                if src_n == 0 {
                    break false;
                }
            }
        };

        if are_different {
            eprintln!("file {} changed", destination.display());
            std::thread::sleep(std::time::Duration::from_millis(10));
            self.atomic_copy(source, destination)?;
        }

        Ok(())
    }

    fn run_formatter(&self, command: &str, file: &Path) -> Result<(), SafeWriterError> {
        let parts: Vec<&str> = command.split_whitespace().collect();
        let status = std::process::Command::new(parts[0])
            .args(&parts[1..])
            .arg(file)
            .status()
            .map_err(|e| {
                SafeWriterError::FormatterError(format!("could not run '{}': {}", command, e))
            })?;
        if !status.success() {
            return Err(SafeWriterError::FormatterError(format!(
                "'{}' exited with code {}",
                command,
                status.code().unwrap_or(-1)
            )));
        }
        Ok(())
    }
}
impl SafeFileWriter {
    pub fn before_write<P: AsRef<Path>>(
        &mut self,
        file_name: P,
    ) -> Result<PathBuf, SafeWriterError> {
        validate_filename(file_name.as_ref())?;
        let path = file_name.as_ref();

        let dest_dir = path.parent().unwrap_or_else(|| Path::new(""));
        fs::create_dir_all(self.gen_base.join(dest_dir))
            .map_err(|_| SafeWriterError::DirectoryCreationFailed(self.gen_base.join(dest_dir)))?;

        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let suffix = if ext.is_empty() {
            String::new()
        } else {
            format!(".{ext}")
        };
        let tmp = tempfile::Builder::new()
            .suffix(&suffix)
            .tempfile()
            .map_err(SafeWriterError::IoError)?;
        let tmp_path = tmp.path().to_path_buf();
        self.staging.insert(path.to_string_lossy().into_owned(), tmp);
        Ok(tmp_path)
    }

    pub fn after_write<P: AsRef<Path>>(&mut self, file_name: P) -> Result<(), SafeWriterError> {
        validate_filename(file_name.as_ref())?;
        let key = file_name.as_ref().to_string_lossy().into_owned();
        let tmp = self
            .staging
            .remove(&key)
            .ok_or_else(|| SafeWriterError::BackupFailed(file_name.as_ref().to_path_buf()))?;
        let tmp_path = tmp.path().to_path_buf();
        let output_file = self.gen_base.join(file_name.as_ref());

        // Step 1: run formatter on temp copy if configured.
        let ext = file_name
            .as_ref()
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        if let Some(cmd) = self.config.formatters.get(ext).cloned() {
            self.run_formatter(&cmd, &tmp_path)?;
        }

        // Step 2: content-based modification detection.
        if output_file.is_file() {
            let baseline = self.db.get_baseline(&key)?;
            if let Some(baseline_bytes) = baseline {
                let current = fs::read(&output_file)?;
                if current != baseline_bytes {
                    return Err(SafeWriterError::ModifiedExternally(output_file));
                }
            }
        }

        // Step 3: copy temp → output, skip if identical.
        self.copy_if_different(&tmp_path, &output_file)?;

        // Step 4: update baseline in the temp db.
        let written = fs::read(&tmp_path)
            .map_err(|_| SafeWriterError::BackupFailed(tmp_path.clone()))?;
        self.db
            .set_baseline(&key, &written)
            .map_err(SafeWriterError::DbError)?;

        // tmp is dropped here, deleting the temp file.
        Ok(())
    }
}
impl SafeFileWriter {
    pub fn get_config(&self) -> &SafeWriterConfig {
        &self.config
    }

    pub fn set_config(&mut self, config: SafeWriterConfig) {
        self.config = config;
    }

    pub fn db(&self) -> &WeavebackDb {
        &self.db
    }

    pub fn finish(self, target: &Path) -> Result<(), SafeWriterError> {
        self.db.merge_into(target).map_err(SafeWriterError::DbError)?;
        Ok(())
    }

    pub fn get_gen_base(&self) -> &Path {
        &self.gen_base
    }

    /// Retrieve the stored baseline bytes for a relative path (test helper).
    #[cfg(test)]
    pub fn get_baseline_for_test(&self, path: &str) -> Option<Vec<u8>> {
        self.db.get_baseline(path).ok().flatten()
    }
}
fn validate_filename(path: &Path) -> Result<(), SafeWriterError> {
    use std::path::Component;

    if path.is_absolute() {
        return Err(SafeWriterError::SecurityViolation(format!(
            "Absolute paths are not allowed: {}",
            path.display()
        )));
    }

    let filename = path.to_string_lossy();
    if filename.len() >= 2 {
        let mut chars = filename.chars();
        let first = chars.next().unwrap();
        let second = chars.next().unwrap();
        if second == ':' && first.is_ascii_alphabetic() {
            return Err(SafeWriterError::SecurityViolation(format!(
                "Windows-style absolute paths are not allowed: {}",
                filename
            )));
        }
    }

    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(SafeWriterError::SecurityViolation(format!(
            "Path traversal detected (..): {}",
            path.display()
        )));
    }

    Ok(())
}
