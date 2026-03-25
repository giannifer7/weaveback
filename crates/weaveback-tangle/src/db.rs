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
    confidence TEXT    NOT NULL DEFAULT 'exact',
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

CREATE INDEX IF NOT EXISTS idx_var_defs_name   ON var_defs(var_name);
CREATE INDEX IF NOT EXISTS idx_macro_defs_name ON macro_defs(macro_name);
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

/// How reliably a post-formatter output line was traced back to its source.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Confidence {
    /// Diff Equal match — the line survived formatting unchanged.
    #[default]
    Exact,
    /// Matched by normalised content hash — survives reordering (e.g. import sorting).
    HashMatch,
    /// Attribution inherited from the nearest attributed neighbour (gap-fill).
    Inferred,
}

impl Confidence {
    pub fn as_str(&self) -> &'static str {
        match self {
            Confidence::Exact     => "exact",
            Confidence::HashMatch => "hash_match",
            Confidence::Inferred  => "inferred",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "hash_match" => Confidence::HashMatch,
            "inferred"   => Confidence::Inferred,
            _            => Confidence::Exact,
        }
    }
}

#[derive(Debug, Clone)]
pub struct NowebMapEntry {
    pub src_file: String,
    pub chunk_name: String,
    pub src_line: u32,
    pub indent: String,
    pub confidence: Confidence,
}
pub struct WeavebackDb {
    conn: Connection,
}

fn apply_schema(conn: &Connection) -> Result<(), DbError> {
    conn.execute_batch(CREATE_SCHEMA).map_err(DbError::Sql)?;
    // Migration: add the confidence column to databases created before it
    // existed.  SQLite errors if the column is already there; we ignore that.
    let _ = conn.execute_batch(
        "ALTER TABLE noweb_map ADD COLUMN confidence TEXT NOT NULL DEFAULT 'exact';"
    );
    Ok(())
}

impl WeavebackDb {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, DbError> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        apply_schema(&conn)?;
        Ok(Self { conn })
    }

    pub fn open_read_only<P: AsRef<Path>>(path: P) -> Result<Self, DbError> {
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?;
        Ok(Self { conn })
    }

    pub fn open_temp() -> Result<Self, DbError> {
        let conn = Connection::open_in_memory()?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
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
        &mut self,
        out_file: &str,
        entries: &[(u32, NowebMapEntry)],
    ) -> Result<(), DbError> {
        if entries.is_empty() {
            return Ok(());
        }
        let tx = self.conn.transaction()?;
        {
            let mut stmt = tx.prepare_cached(
                "INSERT OR REPLACE INTO noweb_map
                 (out_file, out_line, src_file, chunk_name, src_line, indent, confidence)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )?;
            for (line, e) in entries {
                stmt.execute(params![
                    out_file,
                    *line,
                    e.src_file,
                    e.chunk_name,
                    e.src_line,
                    e.indent,
                    e.confidence.as_str()
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
                "SELECT src_file, chunk_name, src_line, indent, confidence
                 FROM noweb_map WHERE out_file = ?1 AND out_line = ?2",
                params![out_file, out_line],
                |row| {
                    Ok(NowebMapEntry {
                        src_file: row.get(0)?,
                        chunk_name: row.get(1)?,
                        src_line: row.get::<_, u32>(2)?,
                        indent: row.get(3)?,
                        confidence: row
                            .get::<_, String>(4)
                            .map(|s| Confidence::parse(&s))
                            .unwrap_or_default(),
                    })
                },
            )
            .optional()?)
    }
}
impl WeavebackDb {
    pub fn set_macro_map_entries(
        &mut self,
        driver_file: &str,
        entries: &[(u32, Vec<u8>)],
    ) -> Result<(), DbError> {
        if entries.is_empty() {
            return Ok(());
        }
        let tx = self.conn.transaction()?;
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

        let result = (|| -> rusqlite::Result<()> {
            self.conn.execute_batch("BEGIN IMMEDIATE;")?;
            self.conn.execute_batch(
                "INSERT OR REPLACE INTO target.gen_baselines SELECT * FROM gen_baselines;
                 INSERT OR REPLACE INTO target.noweb_map     SELECT * FROM noweb_map;
                 INSERT OR REPLACE INTO target.macro_map     SELECT * FROM macro_map;
                 INSERT OR REPLACE INTO target.src_snapshots SELECT * FROM src_snapshots;
                 INSERT OR REPLACE INTO target.var_defs      SELECT * FROM var_defs;
                 INSERT OR REPLACE INTO target.macro_defs    SELECT * FROM macro_defs;",
            )?;
            self.conn.execute_batch("COMMIT;")?;
            Ok(())
        })();

        if result.is_err() {
            let _ = self.conn.execute_batch("ROLLBACK;");
        }
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
