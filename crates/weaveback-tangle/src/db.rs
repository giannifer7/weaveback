use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use std::path::Path;

const CREATE_SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS files (
    id   INTEGER PRIMARY KEY,
    path TEXT    NOT NULL UNIQUE
) STRICT;

CREATE TABLE IF NOT EXISTS gen_baselines (
    path    TEXT PRIMARY KEY NOT NULL,
    content BLOB NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS noweb_map (
    out_file   INTEGER NOT NULL REFERENCES files(id),
    out_line   INTEGER NOT NULL,
    src_file   INTEGER NOT NULL REFERENCES files(id),
    chunk_name TEXT    NOT NULL,
    src_line   INTEGER NOT NULL,
    indent     TEXT    NOT NULL,
    confidence TEXT    NOT NULL DEFAULT 'exact',
    PRIMARY KEY (out_file, out_line)
) STRICT, WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS macro_map (
    driver_file   INTEGER NOT NULL REFERENCES files(id),
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
    src_file INTEGER NOT NULL REFERENCES files(id),
    pos      INTEGER NOT NULL,
    length   INTEGER NOT NULL,
    PRIMARY KEY (var_name, src_file, pos)
) STRICT, WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS macro_defs (
    macro_name TEXT    NOT NULL,
    src_file   INTEGER NOT NULL REFERENCES files(id),
    pos        INTEGER NOT NULL,
    length     INTEGER NOT NULL,
    PRIMARY KEY (macro_name, src_file, pos)
) STRICT, WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS chunk_deps (
    from_chunk TEXT    NOT NULL,
    to_chunk   TEXT    NOT NULL,
    src_file   INTEGER NOT NULL REFERENCES files(id),
    PRIMARY KEY (from_chunk, to_chunk, src_file)
) STRICT, WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS chunk_defs (
    src_file    INTEGER NOT NULL REFERENCES files(id),
    chunk_name  TEXT    NOT NULL,
    nth         INTEGER NOT NULL DEFAULT 0,
    def_start   INTEGER NOT NULL,
    def_end     INTEGER NOT NULL,
    PRIMARY KEY (src_file, chunk_name, nth)
) STRICT, WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS literate_source_config (
    src_file        INTEGER NOT NULL REFERENCES files(id),
    special_char    TEXT    NOT NULL,
    open_delim      TEXT    NOT NULL,
    close_delim     TEXT    NOT NULL,
    chunk_end       TEXT    NOT NULL,
    comment_markers TEXT    NOT NULL,
    PRIMARY KEY (src_file)
) STRICT, WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS run_config (
    key   TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
) STRICT, WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS source_blocks (
    src_file    INTEGER NOT NULL REFERENCES files(id),
    block_index INTEGER NOT NULL,
    block_type  TEXT    NOT NULL,
    line_start  INTEGER NOT NULL,
    line_end    INTEGER NOT NULL,
    content_hash BLOB   NOT NULL,
    PRIMARY KEY (src_file, block_index)
) STRICT, WITHOUT ROWID;

CREATE INDEX IF NOT EXISTS idx_chunk_deps_to ON chunk_deps(to_chunk);
CREATE INDEX IF NOT EXISTS idx_noweb_map_src ON noweb_map(src_file, src_line);

CREATE VIRTUAL TABLE IF NOT EXISTS prose_fts USING fts5(
    content,
    src_file  UNINDEXED,
    block_type UNINDEXED,
    line_start UNINDEXED,
    line_end   UNINDEXED,
    tokenize  = 'porter unicode61'
);
";
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DbError {
    #[error("database error: {0}")]
    Sql(#[from] rusqlite::Error),
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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TangleConfig {
    pub special_char: char,
    pub open_delim: String,
    pub close_delim: String,
    pub chunk_end: String,
    pub comment_markers: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct NowebMapEntry {
    pub src_file: String,
    pub chunk_name: String,
    pub src_line: u32,
    pub indent: String,
    pub confidence: Confidence,
}

/// One parsed logical block stored in `source_blocks`.
#[derive(Debug, Clone)]
pub struct StoredBlockInfo {
    pub block_index:  u32,
    pub block_type:   String,
    pub line_start:   u32,
    pub line_end:     u32,
    pub content_hash: Vec<u8>,
}

/// Location of a chunk definition within a literate source file.
/// `def_start` is the 1-indexed line of the open marker (`// <<name>>=`).
/// `def_end`   is the 1-indexed line of the close marker (`// @@`).
#[derive(Debug, Clone)]
pub struct ChunkDefEntry {
    pub src_file:   String,
    pub chunk_name: String,
    pub nth:        u32,
    pub def_start:  u32,
    pub def_end:    u32,
}
pub struct WeavebackDb {
    conn: Connection,
}

/// Intern a file path: insert if not present, return the row id.
fn intern_file(conn: &Connection, path: &str) -> Result<i64, DbError> {
    conn.execute("INSERT OR IGNORE INTO files (path) VALUES (?1)", params![path])?;
    Ok(conn.query_row(
        "SELECT id FROM files WHERE path = ?1",
        params![path],
        |row| row.get(0),
    )?)
}

/// Detect whether the db uses the pre-file-ID schema (noweb_map.out_file is TEXT).
fn needs_file_id_migration(conn: &Connection) -> Result<bool, DbError> {
    let col_type: Option<String> = conn.query_row(
        "SELECT type FROM pragma_table_info('noweb_map') WHERE name='out_file'",
        [],
        |row| row.get(0),
    ).optional()?;
    Ok(col_type.as_deref() == Some("TEXT"))
}

fn apply_schema(conn: &Connection) -> Result<(), DbError> {
    // If the db was created with the old TEXT-based file columns, drop those
    // tables so CREATE_SCHEMA recreates them with integer IDs.  The db is a
    // disposable build artefact; gen_baselines and src_snapshots (which store
    // the per-file baselines and source snapshots) are left intact.
    if needs_file_id_migration(conn)? {
        conn.execute_batch("
            DROP TABLE IF EXISTS noweb_map;
            DROP TABLE IF EXISTS macro_map;
            DROP TABLE IF EXISTS var_defs;
            DROP TABLE IF EXISTS macro_defs;
            DROP TABLE IF EXISTS chunk_deps;
            DROP TABLE IF EXISTS chunk_defs;
            DROP TABLE IF EXISTS literate_source_config;
            DROP TABLE IF EXISTS source_blocks;
        ")?;
    }

    conn.execute_batch(CREATE_SCHEMA).map_err(DbError::Sql)?;
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
        // Pre-intern all file paths before opening the transaction.
        let out_file_id = intern_file(&self.conn, out_file)?;
        let mut src_ids: std::collections::HashMap<&str, i64> = Default::default();
        for (_, e) in entries {
            if let std::collections::hash_map::Entry::Vacant(v) =
                src_ids.entry(e.src_file.as_str())
            {
                v.insert(intern_file(&self.conn, &e.src_file)?);
            }
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
                    out_file_id,
                    *line,
                    src_ids[e.src_file.as_str()],
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
                "SELECT f_src.path, nm.chunk_name, nm.src_line, nm.indent, nm.confidence
                 FROM noweb_map nm
                 JOIN files f_out ON f_out.id = nm.out_file
                 JOIN files f_src ON f_src.id = nm.src_file
                 WHERE f_out.path = ?1 AND nm.out_line = ?2",
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
    /// Write direct chunk→chunk dependency edges.
    /// Each tuple is `(from_chunk, to_chunk, src_file)`.
    /// Deletes all existing edges for each source file in `deps` before
    /// reinserting, so stale edges from renamed chunk references are removed.
    pub fn set_chunk_deps(
        &mut self,
        deps: &[(String, String, String)],
    ) -> Result<(), DbError> {
        if deps.is_empty() {
            return Ok(());
        }
        // Pre-intern all src_files.
        let mut src_ids: std::collections::HashMap<&str, i64> = Default::default();
        for (_, _, src) in deps {
            if let std::collections::hash_map::Entry::Vacant(v) = src_ids.entry(src.as_str()) {
                v.insert(intern_file(&self.conn, src)?);
            }
        }
        let tx = self.conn.transaction()?;
        {
            let mut unique_ids: Vec<i64> = src_ids.values().copied().collect();
            unique_ids.sort_unstable();
            unique_ids.dedup();
            let mut del = tx.prepare_cached(
                "DELETE FROM chunk_deps WHERE src_file = ?1",
            )?;
            for id in &unique_ids {
                del.execute(params![id])?;
            }
            let mut ins = tx.prepare_cached(
                "INSERT INTO chunk_deps (from_chunk, to_chunk, src_file)
                 VALUES (?1, ?2, ?3)",
            )?;
            for (from, to, src) in deps {
                ins.execute(params![from, to, src_ids[src.as_str()]])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Return `(to_chunk, src_file)` pairs for all chunks that `chunk_name`
    /// directly references.
    pub fn query_chunk_deps(
        &self,
        chunk_name: &str,
    ) -> Result<Vec<(String, String)>, DbError> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT cd.to_chunk, f.path
             FROM chunk_deps cd JOIN files f ON f.id = cd.src_file
             WHERE cd.from_chunk = ?1",
        )?;
        let rows = stmt.query_map(params![chunk_name], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        rows.collect::<Result<_, _>>().map_err(DbError::Sql)
    }

    /// Return `(from_chunk, src_file)` pairs for all chunks that directly
    /// reference `chunk_name` (reverse / "what uses this?" query).
    pub fn query_reverse_deps(
        &self,
        chunk_name: &str,
    ) -> Result<Vec<(String, String)>, DbError> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT cd.from_chunk, f.path
             FROM chunk_deps cd JOIN files f ON f.id = cd.src_file
             WHERE cd.to_chunk = ?1",
        )?;
        let rows = stmt.query_map(params![chunk_name], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        rows.collect::<Result<_, _>>().map_err(DbError::Sql)
    }

    /// Return every `(from_chunk, to_chunk, src_file)` triple stored in the
    /// graph, ordered by `from_chunk` then `to_chunk`.  Used by
    /// `weaveback graph` to export the full DOT representation.
    pub fn query_all_chunk_deps(&self) -> Result<Vec<(String, String, String)>, DbError> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT cd.from_chunk, cd.to_chunk, f.path
             FROM chunk_deps cd JOIN files f ON f.id = cd.src_file
             ORDER BY cd.from_chunk, cd.to_chunk",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        rows.collect::<Result<_, _>>().map_err(DbError::Sql)
    }

    /// Return the distinct output files that contain lines attributed to
    /// `chunk_name`.  Used by `weaveback impact` to map terminal chunks to
    /// the `gen/` files they affect.
    pub fn query_chunk_output_files(
        &self,
        chunk_name: &str,
    ) -> Result<Vec<String>, DbError> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT DISTINCT f.path
             FROM noweb_map nm JOIN files f ON f.id = nm.out_file
             WHERE nm.chunk_name = ?1 ORDER BY f.path",
        )?;
        let rows = stmt.query_map(params![chunk_name], |row| row.get::<_, String>(0))?;
        rows.collect::<Result<_, _>>().map_err(DbError::Sql)
    }
}
impl WeavebackDb {
    pub fn set_chunk_defs(&mut self, entries: &[ChunkDefEntry]) -> Result<(), DbError> {
        if entries.is_empty() {
            return Ok(());
        }
        // Pre-intern all src_files.
        let mut src_ids: std::collections::HashMap<&str, i64> = Default::default();
        for e in entries {
            if let std::collections::hash_map::Entry::Vacant(v) =
                src_ids.entry(e.src_file.as_str())
            {
                v.insert(intern_file(&self.conn, &e.src_file)?);
            }
        }
        let tx = self.conn.transaction()?;
        {
            let mut unique_ids: Vec<i64> = src_ids.values().copied().collect();
            unique_ids.sort_unstable();
            unique_ids.dedup();
            let mut del = tx.prepare_cached(
                "DELETE FROM chunk_defs WHERE src_file = ?1",
            )?;
            for id in &unique_ids {
                del.execute(params![id])?;
            }
            let mut stmt = tx.prepare_cached(
                "INSERT INTO chunk_defs (src_file, chunk_name, nth, def_start, def_end)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            for e in entries {
                stmt.execute(params![
                    src_ids[e.src_file.as_str()],
                    e.chunk_name,
                    e.nth,
                    e.def_start,
                    e.def_end
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn get_chunk_def(
        &self,
        src_file: &str,
        chunk_name: &str,
        nth: u32,
    ) -> Result<Option<ChunkDefEntry>, DbError> {
        Ok(self
            .conn
            .query_row(
                "SELECT f.path, cdef.chunk_name, cdef.nth, cdef.def_start, cdef.def_end
                 FROM chunk_defs cdef JOIN files f ON f.id = cdef.src_file
                 WHERE f.path = ?1 AND cdef.chunk_name = ?2 AND cdef.nth = ?3",
                params![src_file, chunk_name, nth],
                |row| {
                    Ok(ChunkDefEntry {
                        src_file:   row.get(0)?,
                        chunk_name: row.get(1)?,
                        nth:        row.get::<_, u32>(2)?,
                        def_start:  row.get::<_, u32>(3)?,
                        def_end:    row.get::<_, u32>(4)?,
                    })
                },
            )
            .optional()?)
    }

    pub fn list_chunk_defs(&self, src_file: Option<&str>) -> Result<Vec<ChunkDefEntry>, DbError> {
        fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ChunkDefEntry> {
            Ok(ChunkDefEntry {
                src_file:   row.get(0)?,
                chunk_name: row.get(1)?,
                nth:        row.get::<_, u32>(2)?,
                def_start:  row.get::<_, u32>(3)?,
                def_end:    row.get::<_, u32>(4)?,
            })
        }
        if let Some(f) = src_file {
            let mut stmt = self.conn.prepare(
                "SELECT f.path, cdef.chunk_name, cdef.nth, cdef.def_start, cdef.def_end
                 FROM chunk_defs cdef JOIN files f ON f.id = cdef.src_file
                 WHERE f.path = ?1 ORDER BY f.path, cdef.def_start",
            )?;
            Ok(stmt.query_map(params![f], map_row)?.collect::<Result<Vec<_>, _>>()?)
        } else {
            let mut stmt = self.conn.prepare(
                "SELECT f.path, cdef.chunk_name, cdef.nth, cdef.def_start, cdef.def_end
                 FROM chunk_defs cdef JOIN files f ON f.id = cdef.src_file
                 ORDER BY f.path, cdef.def_start",
            )?;
            Ok(stmt.query_map([], map_row)?.collect::<Result<Vec<_>, _>>()?)
        }
    }

    pub fn find_chunk_defs_by_name(&self, chunk_name: &str) -> Result<Vec<ChunkDefEntry>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT f.path, cdef.chunk_name, cdef.nth, cdef.def_start, cdef.def_end
             FROM chunk_defs cdef JOIN files f ON f.id = cdef.src_file
             WHERE cdef.chunk_name = ?1 ORDER BY f.path, cdef.nth",
        )?;
        Ok(stmt.query_map(params![chunk_name], |row| {
            Ok(ChunkDefEntry {
                src_file:   row.get(0)?,
                chunk_name: row.get(1)?,
                nth:        row.get::<_, u32>(2)?,
                def_start:  row.get::<_, u32>(3)?,
                def_end:    row.get::<_, u32>(4)?,
            })
        })?.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn list_all_chunk_defs(&self) -> Result<Vec<ChunkDefEntry>, DbError> {
        self.list_chunk_defs(None)
    }

    pub fn list_all_chunk_deps(&self) -> Result<Vec<(String, String, String)>, DbError> {
        self.query_all_chunk_deps()
    }

    /// Return all chunk definitions in `src_file` whose line range overlaps
    /// `[line_start, line_end]`.  Used by incremental-build skip-set computation.
    pub fn query_chunk_defs_overlapping(
        &self,
        src_file: &str,
        line_start: u32,
        line_end: u32,
    ) -> Result<Vec<ChunkDefEntry>, DbError> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT f.path, cdef.chunk_name, cdef.nth, cdef.def_start, cdef.def_end
             FROM chunk_defs cdef JOIN files f ON f.id = cdef.src_file
             WHERE f.path = ?1
               AND cdef.def_start <= ?3
               AND cdef.def_end   >= ?2",
        )?;
        let rows = stmt.query_map(params![src_file, line_start, line_end], |row| {
            Ok(ChunkDefEntry {
                src_file:   row.get(0)?,
                chunk_name: row.get(1)?,
                nth:        row.get::<_, u32>(2)?,
                def_start:  row.get::<_, u32>(3)?,
                def_end:    row.get::<_, u32>(4)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
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
        let file_id = intern_file(&self.conn, driver_file)?;
        let tx = self.conn.transaction()?;
        {
            let mut stmt = tx.prepare_cached(
                "INSERT OR REPLACE INTO macro_map (driver_file, expanded_line, data)
                 VALUES (?1, ?2, ?3)",
            )?;
            for (line, bytes) in entries {
                stmt.execute(params![file_id, *line, bytes.as_slice()])?;
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
                "SELECT mm.data FROM macro_map mm JOIN files f ON f.id = mm.driver_file
                 WHERE f.path = ?1 AND mm.expanded_line = ?2",
                params![driver_file, expanded_line],
                |row| row.get(0),
            )
            .optional()?)
    }
}
impl WeavebackDb {
    pub fn set_source_config(
        &self,
        src_file: &str,
        cfg: &TangleConfig,
    ) -> Result<(), DbError> {
        let file_id = intern_file(&self.conn, src_file)?;
        self.conn.execute(
            "INSERT OR REPLACE INTO literate_source_config
             (src_file, special_char, open_delim, close_delim, chunk_end, comment_markers)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                file_id,
                cfg.special_char.to_string(),
                cfg.open_delim,
                cfg.close_delim,
                cfg.chunk_end,
                cfg.comment_markers.join(",")
            ],
        )?;
        Ok(())
    }

    pub fn get_source_config(&self, src_file: &str) -> Result<Option<TangleConfig>, DbError> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT lsc.special_char, lsc.open_delim, lsc.close_delim,
                    lsc.chunk_end, lsc.comment_markers
             FROM literate_source_config lsc JOIN files f ON f.id = lsc.src_file
             WHERE f.path = ?1",
        )?;
        Ok(stmt.query_row(params![src_file], |row| {
            let sc_str: String = row.get(0)?;
            let special_char = sc_str.chars().next().unwrap_or('%');
            let cm_str: String = row.get(4)?;
            let comment_markers = cm_str.split(',').map(|s| s.to_string()).collect();
            Ok(TangleConfig {
                special_char,
                open_delim: row.get(1)?,
                close_delim: row.get(2)?,
                chunk_end: row.get(3)?,
                comment_markers,
            })
        }).optional()?)
    }

    /// Returns the (out_file, out_line) for a given literate source location.
    pub fn get_output_location(
        &self,
        src_file: &str,
        src_line: u32,
    ) -> Result<Option<(String, u32)>, DbError> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT f_out.path, nm.out_line FROM noweb_map nm
             JOIN files f_out ON f_out.id = nm.out_file
             JOIN files f_src ON f_src.id = nm.src_file
             WHERE f_src.path = ?1 AND nm.src_line = ?2
             LIMIT 1",
        )?;
        Ok(stmt.query_row(params![src_file, src_line], |row| {
            Ok((row.get(0)?, row.get(1)?))
        }).optional()?)
    }

    pub fn set_run_config(&self, key: &str, value: &str) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT OR REPLACE INTO run_config (key, value) VALUES (?1, ?2)",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn get_run_config(&self, key: &str) -> Result<Option<String>, DbError> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT value FROM run_config WHERE key = ?1",
        )?;
        Ok(stmt.query_row(params![key], |row| row.get(0)).optional()?)
    }

    /// Returns all (src_line, out_file, out_line) mappings for a given literate source file.
    pub fn get_all_output_mappings(
        &self,
        src_file: &str,
    ) -> Result<Vec<(u32, String, u32)>, DbError> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT nm.src_line, f_out.path, nm.out_line FROM noweb_map nm
             JOIN files f_out ON f_out.id = nm.out_file
             JOIN files f_src ON f_src.id = nm.src_file
             WHERE f_src.path = ?1",
        )?;
        let rows = stmt.query_map(params![src_file], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?;
        let mut res = Vec::new();
        for row in rows {
            res.push(row?);
        }
        Ok(res)
    }
}
impl WeavebackDb {
    pub fn set_source_blocks(
        &mut self,
        src_file: &str,
        blocks: &[crate::block_parser::SourceBlockEntry],
    ) -> Result<(), DbError> {
        let file_id = intern_file(&self.conn, src_file)?;
        let tx = self.conn.transaction()?;
        {
            let mut del = tx.prepare_cached(
                "DELETE FROM source_blocks WHERE src_file = ?1",
            )?;
            del.execute(params![file_id])?;
            let mut ins = tx.prepare_cached(
                "INSERT INTO source_blocks
                 (src_file, block_index, block_type, line_start, line_end, content_hash)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?;
            for b in blocks {
                ins.execute(params![
                    file_id,
                    b.block_index,
                    b.block_type,
                    b.line_start,
                    b.line_end,
                    b.content_hash.as_slice()
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn get_source_block_hashes(
        &self,
        src_file: &str,
    ) -> Result<Vec<(u32, Vec<u8>)>, DbError> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT sb.block_index, sb.content_hash
             FROM source_blocks sb JOIN files f ON f.id = sb.src_file
             WHERE f.path = ?1
             ORDER BY sb.block_index",
        )?;
        let rows = stmt.query_map(params![src_file], |row| {
            Ok((row.get::<_, u32>(0)?, row.get::<_, Vec<u8>>(1)?))
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn query_blocks_overlapping_range(
        &self,
        src_file: &str,
        line_start: u32,
        line_end: u32,
    ) -> Result<Vec<StoredBlockInfo>, DbError> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT sb.block_index, sb.block_type, sb.line_start, sb.line_end, sb.content_hash
             FROM source_blocks sb JOIN files f ON f.id = sb.src_file
             WHERE f.path = ?1
               AND sb.line_start <= ?3
               AND sb.line_end   >= ?2",
        )?;
        let rows = stmt.query_map(params![src_file, line_start, line_end], |row| {
            Ok(StoredBlockInfo {
                block_index:  row.get::<_, u32>(0)?,
                block_type:   row.get(1)?,
                line_start:   row.get::<_, u32>(2)?,
                line_end:     row.get::<_, u32>(3)?,
                content_hash: row.get::<_, Vec<u8>>(4)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }
}
/// Escape a string for use inside a SQLite single-quoted string literal.
fn sqlite_string_literal(s: &str) -> String {
    s.replace('\'', "''")
}

impl WeavebackDb {
    /// Merge all data from this (typically per-file temp) db into `target_path`.
    ///
    /// File paths are interned independently in each database, so their integer
    /// IDs may differ.  The merge remaps IDs via the shared `files.path` strings:
    /// source file paths are inserted into the target's `files` table first, then
    /// each data table is inserted with subquery-based ID translation.
    pub fn merge_into(&self, target_path: &Path) -> Result<(), DbError> {
        {
            let t = Connection::open(target_path)?;
            t.busy_timeout(std::time::Duration::from_millis(200))?;
            t.pragma_update(None, "journal_mode", "WAL")?;
            t.pragma_update(None, "synchronous", "NORMAL")?;
            t.pragma_update(None, "foreign_keys", "ON")?;
            apply_schema(&t)?;
        }

        self.conn.busy_timeout(std::time::Duration::from_millis(200))?;
        let target_str = target_path.to_string_lossy();
        let escaped = sqlite_string_literal(&target_str);
        self.conn
            .execute_batch(&format!("ATTACH DATABASE '{escaped}' AS target"))?;

        let result = (|| -> rusqlite::Result<()> {
            self.conn.execute_batch("BEGIN IMMEDIATE;")?;

            // Ensure every source file path exists in the target's files table.
            self.conn.execute_batch(
                "INSERT OR IGNORE INTO target.files (path) SELECT path FROM files;"
            )?;

            // Tables without file IDs: simple copy.
            self.conn.execute_batch(
                "INSERT OR REPLACE INTO target.gen_baselines SELECT * FROM gen_baselines;
                 INSERT OR REPLACE INTO target.src_snapshots  SELECT * FROM src_snapshots;
                 INSERT OR REPLACE INTO target.run_config     SELECT * FROM run_config;"
            )?;

            // Tables with file IDs: remap via path lookup in target.files.
            self.conn.execute_batch("
                INSERT OR REPLACE INTO target.noweb_map
                SELECT
                    (SELECT t.id FROM target.files t
                     WHERE t.path = (SELECT path FROM files WHERE id = nm.out_file)),
                    nm.out_line,
                    (SELECT t.id FROM target.files t
                     WHERE t.path = (SELECT path FROM files WHERE id = nm.src_file)),
                    nm.chunk_name, nm.src_line, nm.indent, nm.confidence
                FROM noweb_map nm;
            ")?;

            self.conn.execute_batch("
                INSERT OR REPLACE INTO target.macro_map
                SELECT
                    (SELECT t.id FROM target.files t
                     WHERE t.path = (SELECT path FROM files WHERE id = mm.driver_file)),
                    mm.expanded_line, mm.data
                FROM macro_map mm;
            ")?;

            self.conn.execute_batch("
                INSERT OR REPLACE INTO target.var_defs
                SELECT vd.var_name,
                    (SELECT t.id FROM target.files t
                     WHERE t.path = (SELECT path FROM files WHERE id = vd.src_file)),
                    vd.pos, vd.length
                FROM var_defs vd;
            ")?;

            self.conn.execute_batch("
                INSERT OR REPLACE INTO target.macro_defs
                SELECT md.macro_name,
                    (SELECT t.id FROM target.files t
                     WHERE t.path = (SELECT path FROM files WHERE id = md.src_file)),
                    md.pos, md.length
                FROM macro_defs md;
            ")?;

            self.conn.execute_batch("
                INSERT OR REPLACE INTO target.chunk_deps
                SELECT cd.from_chunk, cd.to_chunk,
                    (SELECT t.id FROM target.files t
                     WHERE t.path = (SELECT path FROM files WHERE id = cd.src_file))
                FROM chunk_deps cd;
            ")?;

            self.conn.execute_batch("
                INSERT OR REPLACE INTO target.chunk_defs
                SELECT
                    (SELECT t.id FROM target.files t
                     WHERE t.path = (SELECT path FROM files WHERE id = cdef.src_file)),
                    cdef.chunk_name, cdef.nth, cdef.def_start, cdef.def_end
                FROM chunk_defs cdef;
            ")?;

            self.conn.execute_batch("
                INSERT OR REPLACE INTO target.literate_source_config
                SELECT
                    (SELECT t.id FROM target.files t
                     WHERE t.path = (SELECT path FROM files WHERE id = lsc.src_file)),
                    lsc.special_char, lsc.open_delim, lsc.close_delim,
                    lsc.chunk_end, lsc.comment_markers
                FROM literate_source_config lsc;
            ")?;

            self.conn.execute_batch("
                INSERT OR REPLACE INTO target.source_blocks
                SELECT
                    (SELECT t.id FROM target.files t
                     WHERE t.path = (SELECT path FROM files WHERE id = sb.src_file)),
                    sb.block_index, sb.block_type, sb.line_start, sb.line_end, sb.content_hash
                FROM source_blocks sb;
            ")?;

            self.conn.execute_batch("COMMIT;")?;
            Ok(())
        })();

        if result.is_err() {
            let _ = self.conn.execute_batch("ROLLBACK;");
        } else {
            let _ = self.conn.execute_batch("VACUUM;");
        }
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
        let file_id = intern_file(&self.conn, src_file)?;
        self.conn.execute(
            "INSERT OR REPLACE INTO var_defs (var_name, src_file, pos, length)
             VALUES (?1, ?2, ?3, ?4)",
            params![var_name, file_id, pos, length],
        )?;
        Ok(())
    }

    pub fn query_var_defs(&self, var_name: &str) -> Result<Vec<(String, u32, u32)>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT f.path, vd.pos, vd.length
             FROM var_defs vd JOIN files f ON f.id = vd.src_file
             WHERE vd.var_name = ?1",
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
        let file_id = intern_file(&self.conn, src_file)?;
        self.conn.execute(
            "INSERT OR REPLACE INTO macro_defs (macro_name, src_file, pos, length)
             VALUES (?1, ?2, ?3, ?4)",
            params![macro_name, file_id, pos, length],
        )?;
        Ok(())
    }

    pub fn query_macro_defs(
        &self,
        macro_name: &str,
    ) -> Result<Vec<(String, u32, u32)>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT f.path, md.pos, md.length
             FROM macro_defs md JOIN files f ON f.id = md.src_file
             WHERE md.macro_name = ?1",
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
/// A single result from `search_prose`.
#[derive(Debug, Clone)]
pub struct FtsResult {
    pub src_file:   String,
    pub block_type: String,
    pub line_start: u32,
    pub line_end:   u32,
    /// Short excerpt with matched terms wrapped in `**...**`.
    pub snippet:    String,
}

impl WeavebackDb {
    /// Rebuild the `prose_fts` index from `src_snapshots` + `source_blocks`.
    /// Drops and re-inserts all rows so the index is always consistent.
    pub fn rebuild_prose_fts(&mut self) -> Result<(), DbError> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM prose_fts", [])?;

        // Snapshot paths may be stored as "./rel", "rel", or absolute.
        // The files table uses plain relative paths.  Normalise here.
        let cwd = std::env::current_dir().unwrap_or_default();
        let normalise_path = move |raw: String| -> String {
            let p = std::path::Path::new(&raw);
            // Absolute path → try to make relative to cwd.
            if p.is_absolute() {
                if let Ok(rel) = p.strip_prefix(&cwd) {
                    return rel.to_string_lossy().into_owned();
                }
                return raw;
            }
            // Strip leading "./" component.
            raw.strip_prefix("./").map(str::to_owned).unwrap_or(raw)
        };

        // Load all snapshots; query block metadata per file.
        // Deduplicate after path normalisation: the same source file may be
        // stored under both `./rel/path` and `rel/path` from different passes.
        let snapshots: Vec<(String, String)> = {
            let mut stmt = tx.prepare(
                "SELECT path, content FROM src_snapshots",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?))
            })?;
            let mut seen = std::collections::HashSet::new();
            rows.filter_map(|r| r.ok())
                .filter_map(|(path, bytes)| {
                    let path = normalise_path(path);
                    String::from_utf8(bytes).ok().map(|s| (path, s))
                })
                .filter(|(path, _)| seen.insert(path.clone()))
                .collect()
        };

        for (path, source) in &snapshots {
            let lines: Vec<&str> = source.lines().collect();
            let mut stmt = tx.prepare_cached(
                "SELECT DISTINCT sb.block_type, sb.line_start, sb.line_end
                 FROM source_blocks sb JOIN files f ON f.id = sb.src_file
                 WHERE f.path = ?1
                   AND sb.block_type IN ('section', 'para')
                 ORDER BY sb.line_start",
            )?;
            let blocks: Vec<(String, u32, u32)> = stmt
                .query_map(params![path], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, u32>(1)?,
                        row.get::<_, u32>(2)?,
                    ))
                })?
                .filter_map(|r| r.ok())
                .collect();

            let mut ins = tx.prepare_cached(
                "INSERT INTO prose_fts (content, src_file, block_type, line_start, line_end)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            for (btype, start, end) in blocks {
                let lo = (start as usize).saturating_sub(1);
                let hi = (end as usize).min(lines.len());
                if lo >= hi { continue; }
                let content = lines[lo..hi].join("\n");
                if !content.trim().is_empty() {
                    ins.execute(params![content, path, btype, start, end])?;
                }
            }
        }

        tx.commit()?;
        Ok(())
    }

    /// BM25-ranked full-text search over literate source prose.
    pub fn search_prose(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<FtsResult>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT src_file, block_type, line_start, line_end,
                    snippet(prose_fts, 0, '**', '**', '…', 16)
             FROM prose_fts
             WHERE prose_fts MATCH ?1
             ORDER BY rank
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![query, limit as i64], |row| {
            Ok(FtsResult {
                src_file:   row.get(0)?,
                block_type: row.get(1)?,
                line_start: row.get(2)?,
                line_end:   row.get(3)?,
                snippet:    row.get(4)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }
}
