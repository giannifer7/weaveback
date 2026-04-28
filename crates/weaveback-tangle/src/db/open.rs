// weaveback-tangle/src/db/open.rs
// I'd Really Rather You Didn't edit this generated file.

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

/// Detect whether `prose_fts` is missing the `tags` column (pre-0.9.2 schema).
fn needs_prose_fts_tags_migration(conn: &Connection) -> Result<bool, DbError> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('prose_fts') WHERE name='tags'",
        [],
        |row| row.get(0),
    ).unwrap_or(0);
    Ok(count == 0)
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

    // FTS5 virtual tables cannot be altered; drop and recreate when the schema
    // gains a new column.  rebuild_prose_fts always repopulates from source.
    if needs_prose_fts_tags_migration(conn)? {
        conn.execute("DROP TABLE IF EXISTS prose_fts", [])?;
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

