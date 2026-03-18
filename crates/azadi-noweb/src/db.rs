// src/db.rs  —  azadi persistent database (SQLite, WAL mode)
//
// Tables:
//   gen_baselines : path TEXT PK  → content BLOB
//   noweb_map     : (out_file, out_line) → (src_file, chunk_name, src_line, indent)
//   macro_map     : (driver_file, expanded_line) → data BLOB
//   src_snapshots : path TEXT PK  → content BLOB
//   var_defs      : (var_name, src_file, pos) → length INTEGER
//   macro_defs    : (macro_name, src_file, pos) → length INTEGER
//
// Concurrency model:
//   azadi runs write to an in-memory temp db (AzadiDb::open_temp), then
//   merge_into() copies everything into the target file db in a single write
//   transaction.  Because the target uses WAL mode, read-only connections
//   (MCP server, apply-back reads) never block merges and merges never block
//   readers.

use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use std::path::Path;

// ---------------------------------------------------------------------------
// Schema
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// NowebMapEntry
// ---------------------------------------------------------------------------

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct NowebMapEntry {
    /// Path of the source (literate) file containing this chunk definition.
    pub src_file: String,
    /// Name of the chunk that produced this output line.
    pub chunk_name: String,
    /// 0-indexed line number within the source file.
    pub src_line: u32,
    /// Indentation string prepended to this line during expansion.
    pub indent: String,
}

// ---------------------------------------------------------------------------
// AzadiDb
// ---------------------------------------------------------------------------

pub struct AzadiDb {
    conn: Connection,
}

fn apply_schema(conn: &Connection) -> Result<(), DbError> {
    conn.execute_batch(CREATE_SCHEMA).map_err(DbError::Sql)
}

impl AzadiDb {
    /// Open (or create) the persistent database at `path` with WAL mode.
    /// Use this for the target `azadi.db` in read-write contexts.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, DbError> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        apply_schema(&conn)?;
        Ok(Self { conn })
    }

    /// Open the database read-only.  Never blocks concurrent writers.
    /// Use this in the MCP server and for apply-back reads.
    pub fn open_read_only<P: AsRef<Path>>(path: P) -> Result<Self, DbError> {
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        Ok(Self { conn })
    }

    /// Create an in-memory database for use as a write buffer during an azadi
    /// run.  Call `merge_into()` at the end to persist to the target file db.
    /// No temp file, no cleanup needed — the memory is freed on drop.
    pub fn open_temp() -> Result<Self, DbError> {
        let conn = Connection::open_in_memory()?;
        apply_schema(&conn)?;
        Ok(Self { conn })
    }

    // ── gen_baselines ────────────────────────────────────────────────────────

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
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn set_baseline(&self, path: &str, content: &[u8]) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT OR REPLACE INTO gen_baselines (path, content) VALUES (?1, ?2)",
            params![path, content],
        )?;
        Ok(())
    }

    // ── noweb_map ────────────────────────────────────────────────────────────

    /// Write all source-map entries for one output file in a single transaction.
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

    // ── macro_map ────────────────────────────────────────────────────────────

    /// Write pre-serialized source-map entries for the macro map.
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

    // ── merge_into ───────────────────────────────────────────────────────────

    /// Copy all entries from this (in-memory) database into the persistent
    /// file database at `target_path` in a single write transaction.
    /// The target is created and WAL-initialised if it does not yet exist.
    pub fn merge_into(&self, target_path: &Path) -> Result<(), DbError> {
        // Ensure the target file exists with the correct schema and WAL mode.
        {
            let t = Connection::open(target_path)?;
            t.pragma_update(None, "journal_mode", "WAL")?;
            t.pragma_update(None, "synchronous", "NORMAL")?;
            apply_schema(&t)?;
        }

        // Attach the target and copy all tables in a single write transaction.
        // Using string interpolation for ATTACH (parameterised ATTACH is not
        // supported by SQLite); single-quotes in the path are escaped.
        let target_str = target_path.to_string_lossy();
        let escaped = target_str.replace('\'', "''");
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

    // ── src_snapshots ────────────────────────────────────────────────────────

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
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn set_src_snapshot(&self, path: &str, content: &[u8]) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT OR REPLACE INTO src_snapshots (path, content) VALUES (?1, ?2)",
            params![path, content],
        )?;
        Ok(())
    }

    // ── var_defs ─────────────────────────────────────────────────────────────

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
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    // ── macro_defs ────────────────────────────────────────────────────────────

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
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }
}
