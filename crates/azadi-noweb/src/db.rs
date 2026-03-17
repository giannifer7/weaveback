// src/db.rs  —  azadi embedded database (redb-backed)
//
// Tables:
//   gen_baselines  : relative-output-path → file-content bytes
//                    Used by SafeFileWriter for modification detection.
//   noweb_map      : "<out_file>\x00<out_line:010>" → postcard(NowebMapEntry)
//                    Maps each output line back to its source.
//   src_snapshots  : source-path → file-content bytes
//                    Snapshots of input files written at the end of a run.

use redb::{Database, ReadableTable, TableDefinition};
use std::path::Path;

// ---------------------------------------------------------------------------
// Table definitions
// ---------------------------------------------------------------------------

pub const GEN_BASELINES: TableDefinition<&str, &[u8]> = TableDefinition::new("gen_baselines");
pub const NOWEB_MAP: TableDefinition<&str, &[u8]> = TableDefinition::new("noweb_map");
pub const MACRO_MAP: TableDefinition<&str, &[u8]> = TableDefinition::new("macro_map");
pub const SRC_SNAPSHOTS: TableDefinition<&str, &[u8]> = TableDefinition::new("src_snapshots");
/// Maps `"{var_name}\x00{src_file}\x00{pos:010}" → postcard(u32 length)`
/// for every `%set(var_name, ...)` call site recorded during evaluation.
pub const VAR_DEFS: TableDefinition<&str, &[u8]> = TableDefinition::new("var_defs");
/// Maps `"{macro_name}\x00{src_file}\x00{pos:010}" → postcard(u32 length)`
/// for every `%def/%rhaidef/%pydef(name, ...)` call site.
pub const MACRO_DEFS: TableDefinition<&str, &[u8]> = TableDefinition::new("macro_defs");

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum DbError {
    Db(String),
    Serialize(postcard::Error),
}

impl std::fmt::Display for DbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DbError::Db(s) => write!(f, "database error: {s}"),
            DbError::Serialize(e) => write!(f, "serialization error: {e}"),
        }
    }
}

impl std::error::Error for DbError {}

// ---------------------------------------------------------------------------
// NowebMapEntry: one entry per output line in the noweb_map table
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
// Key helpers
// ---------------------------------------------------------------------------

/// Compose a NOWEB_MAP key from output-file path and 0-indexed output line.
pub fn noweb_key(out_file: &str, out_line: u32) -> String {
    format!("{}\x00{:010}", out_file, out_line)
}

/// Compose a MACRO_MAP key from driver-file path and 0-indexed expanded line.
pub fn macro_key(driver_file: &str, expanded_line: u32) -> String {
    format!("{}\x00{:010}", driver_file, expanded_line)
}

/// Compose a VAR_DEFS / MACRO_DEFS key.
pub fn def_key(name: &str, src_file: &str, pos: u32) -> String {
    format!("{}\x00{}\x00{:010}", name, src_file, pos)
}

/// Prefix used to scan all entries for a given name.
pub fn def_prefix(name: &str) -> String {
    format!("{}\x00", name)
}

// ---------------------------------------------------------------------------
// AzadiDb
// ---------------------------------------------------------------------------

fn copy_table(
    rtxn: &redb::ReadTransaction,
    wtxn: &redb::WriteTransaction,
    table: TableDefinition<&str, &[u8]>,
) -> Result<(), DbError> {
    let src = rtxn.open_table(table).map_err(|e| DbError::Db(e.to_string()))?;
    let mut dst = wtxn.open_table(table).map_err(|e| DbError::Db(e.to_string()))?;
    for item in src.iter().map_err(|e| DbError::Db(e.to_string()))? {
        let (k, v) = item.map_err(|e| DbError::Db(e.to_string()))?;
        dst.insert(k.value(), v.value()).map_err(|e| DbError::Db(e.to_string()))?;
    }
    Ok(())
}

pub struct AzadiDb {
    db: Database,
}

impl AzadiDb {
    /// Open (or create) the database at `path`.  All three tables are
    /// initialised on first open.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, DbError> {
        let db = Database::create(path).map_err(|e| DbError::Db(e.to_string()))?;

        // Ensure every table exists so later read transactions never fail with
        // "table not found".
        let wtxn = db.begin_write().map_err(|e| DbError::Db(e.to_string()))?;
        wtxn.open_table(GEN_BASELINES)
            .map_err(|e| DbError::Db(e.to_string()))?;
        wtxn.open_table(NOWEB_MAP)
            .map_err(|e| DbError::Db(e.to_string()))?;
        wtxn.open_table(MACRO_MAP)
            .map_err(|e| DbError::Db(e.to_string()))?;
        wtxn.open_table(SRC_SNAPSHOTS)
            .map_err(|e| DbError::Db(e.to_string()))?;
        wtxn.open_table(VAR_DEFS)
            .map_err(|e| DbError::Db(e.to_string()))?;
        wtxn.open_table(MACRO_DEFS)
            .map_err(|e| DbError::Db(e.to_string()))?;
        wtxn.commit().map_err(|e| DbError::Db(e.to_string()))?;

        Ok(Self { db })
    }

    // ── gen_baselines ────────────────────────────────────────────────────────

    /// Read the stored baseline for `path` (relative output path).
    pub fn get_baseline(&self, path: &str) -> Result<Option<Vec<u8>>, DbError> {
        let rtxn = self.db.begin_read().map_err(|e| DbError::Db(e.to_string()))?;
        let table = rtxn
            .open_table(GEN_BASELINES)
            .map_err(|e| DbError::Db(e.to_string()))?;
        Ok(table
            .get(path)
            .map_err(|e| DbError::Db(e.to_string()))?
            .map(|v| v.value().to_vec()))
    }

    /// List all (relative-output-path, bytes) pairs in gen_baselines.
    pub fn list_baselines(&self) -> Result<Vec<(String, Vec<u8>)>, DbError> {
        let rtxn = self.db.begin_read().map_err(|e| DbError::Db(e.to_string()))?;
        let table = rtxn
            .open_table(GEN_BASELINES)
            .map_err(|e| DbError::Db(e.to_string()))?;
        let mut result = Vec::new();
        for item in table.iter().map_err(|e| DbError::Db(e.to_string()))? {
            let (k, v) = item.map_err(|e| DbError::Db(e.to_string()))?;
            result.push((k.value().to_string(), v.value().to_vec()));
        }
        Ok(result)
    }

    /// Store `content` as the baseline for `path`.
    pub fn set_baseline(&self, path: &str, content: &[u8]) -> Result<(), DbError> {
        let wtxn = self.db.begin_write().map_err(|e| DbError::Db(e.to_string()))?;
        {
            let mut table = wtxn
                .open_table(GEN_BASELINES)
                .map_err(|e| DbError::Db(e.to_string()))?;
            table
                .insert(path, content)
                .map_err(|e| DbError::Db(e.to_string()))?;
        }
        wtxn.commit().map_err(|e| DbError::Db(e.to_string()))?;
        Ok(())
    }

    // ── noweb_map ────────────────────────────────────────────────────────────

    /// Write all source-map entries for one output file in a single transaction.
    /// `entries` is a slice of `(0-indexed output line, NowebMapEntry)`.
    pub fn set_noweb_entries(
        &self,
        out_file: &str,
        entries: &[(u32, NowebMapEntry)],
    ) -> Result<(), DbError> {
        if entries.is_empty() {
            return Ok(());
        }
        let wtxn = self.db.begin_write().map_err(|e| DbError::Db(e.to_string()))?;
        {
            let mut table = wtxn
                .open_table(NOWEB_MAP)
                .map_err(|e| DbError::Db(e.to_string()))?;
            for (line, entry) in entries {
                let key = noweb_key(out_file, *line);
                let bytes = postcard::to_allocvec(entry).map_err(DbError::Serialize)?;
                table
                    .insert(key.as_str(), bytes.as_slice())
                    .map_err(|e| DbError::Db(e.to_string()))?;
            }
        }
        wtxn.commit().map_err(|e| DbError::Db(e.to_string()))?;
        Ok(())
    }

    /// Look up a single entry in the noweb_map.
    pub fn get_noweb_entry(&self, out_file: &str, out_line: u32) -> Result<Option<NowebMapEntry>, DbError> {
        let rtxn = self.db.begin_read().map_err(|e| DbError::Db(e.to_string()))?;
        let table = rtxn
            .open_table(NOWEB_MAP)
            .map_err(|e| DbError::Db(e.to_string()))?;
        let key = noweb_key(out_file, out_line);
        let val = table.get(key.as_str()).map_err(|e| DbError::Db(e.to_string()))?;
        if let Some(v) = val {
            let entry: NowebMapEntry = postcard::from_bytes(v.value()).map_err(DbError::Serialize)?;
            Ok(Some(entry))
        } else {
            Ok(None)
        }
    }

    // ── macro_map ────────────────────────────────────────────────────────────

    /// Write pre-serialized source-map entries for the macro map in a single transaction.
    /// `entries` is a slice of `(expanded_line, serialized_bytes)`.
    pub fn set_macro_map_entries(
        &self,
        driver_file: &str,
        entries: &[(u32, Vec<u8>)],
    ) -> Result<(), DbError> {
        if entries.is_empty() {
            return Ok(());
        }
        let wtxn = self.db.begin_write().map_err(|e| DbError::Db(e.to_string()))?;
        {
            let mut table = wtxn
                .open_table(MACRO_MAP)
                .map_err(|e| DbError::Db(e.to_string()))?;
            for (line, bytes) in entries {
                let key = macro_key(driver_file, *line);
                table
                    .insert(key.as_str(), bytes.as_slice())
                    .map_err(|e| DbError::Db(e.to_string()))?;
            }
        }
        wtxn.commit().map_err(|e| DbError::Db(e.to_string()))?;
        Ok(())
    }

    /// Look up raw macro_map bytes for a driver file and expanded line.
    pub fn get_macro_map_bytes(&self, driver_file: &str, expanded_line: u32) -> Result<Option<Vec<u8>>, DbError> {
        let rtxn = self.db.begin_read().map_err(|e| DbError::Db(e.to_string()))?;
        let table = rtxn
            .open_table(MACRO_MAP)
            .map_err(|e| DbError::Db(e.to_string()))?;
        let key = macro_key(driver_file, expanded_line);
        let val = table.get(key.as_str()).map_err(|e| DbError::Db(e.to_string()))?;
        Ok(val.map(|v| v.value().to_vec()))
    }

    /// Merge all entries from this database into the database at `target_path`.
    /// Retries if the target is temporarily locked by another process.
    /// Later-written entries win on key conflicts.
    pub fn merge_into(&self, target_path: &Path) -> Result<(), DbError> {
        let target = loop {
            match Database::create(target_path) {
                Ok(db) => break db,
                Err(redb::DatabaseError::DatabaseAlreadyOpen) => {
                    std::thread::sleep(std::time::Duration::from_millis(20));
                }
                Err(e) => return Err(DbError::Db(e.to_string())),
            }
        };

        // Ensure all tables exist in target.
        let wtxn = target.begin_write().map_err(|e| DbError::Db(e.to_string()))?;
        wtxn.open_table(GEN_BASELINES).map_err(|e| DbError::Db(e.to_string()))?;
        wtxn.open_table(NOWEB_MAP).map_err(|e| DbError::Db(e.to_string()))?;
        wtxn.open_table(MACRO_MAP).map_err(|e| DbError::Db(e.to_string()))?;
        wtxn.open_table(SRC_SNAPSHOTS).map_err(|e| DbError::Db(e.to_string()))?;
        wtxn.open_table(VAR_DEFS).map_err(|e| DbError::Db(e.to_string()))?;
        wtxn.open_table(MACRO_DEFS).map_err(|e| DbError::Db(e.to_string()))?;
        wtxn.commit().map_err(|e| DbError::Db(e.to_string()))?;

        let rtxn = self.db.begin_read().map_err(|e| DbError::Db(e.to_string()))?;
        let wtxn = target.begin_write().map_err(|e| DbError::Db(e.to_string()))?;
        {
            copy_table(&rtxn, &wtxn, GEN_BASELINES)?;
            copy_table(&rtxn, &wtxn, NOWEB_MAP)?;
            copy_table(&rtxn, &wtxn, MACRO_MAP)?;
            copy_table(&rtxn, &wtxn, SRC_SNAPSHOTS)?;
            copy_table(&rtxn, &wtxn, VAR_DEFS)?;
            copy_table(&rtxn, &wtxn, MACRO_DEFS)?;
        }
        wtxn.commit().map_err(|e| DbError::Db(e.to_string()))?;
        Ok(())
    }

    // ── src_snapshots ────────────────────────────────────────────────────────

    /// Retrieve the snapshot for `path` (source file path), if stored.
    pub fn get_src_snapshot(&self, path: &str) -> Result<Option<Vec<u8>>, DbError> {
        let rtxn = self.db.begin_read().map_err(|e| DbError::Db(e.to_string()))?;
        let table = rtxn
            .open_table(SRC_SNAPSHOTS)
            .map_err(|e| DbError::Db(e.to_string()))?;
        Ok(table
            .get(path)
            .map_err(|e| DbError::Db(e.to_string()))?
            .map(|v| v.value().to_vec()))
    }

    /// List all stored source snapshots as `(path, bytes)` pairs.
    pub fn list_src_snapshots(&self) -> Result<Vec<(String, Vec<u8>)>, DbError> {
        let rtxn = self.db.begin_read().map_err(|e| DbError::Db(e.to_string()))?;
        let table = rtxn
            .open_table(SRC_SNAPSHOTS)
            .map_err(|e| DbError::Db(e.to_string()))?;
        let mut out = Vec::new();
        for entry in table.iter().map_err(|e| DbError::Db(e.to_string()))? {
            let (k, v) = entry.map_err(|e| DbError::Db(e.to_string()))?;
            out.push((k.value().to_string(), v.value().to_vec()));
        }
        Ok(out)
    }

    /// Snapshot `content` under `path` (source file path).
    pub fn set_src_snapshot(&self, path: &str, content: &[u8]) -> Result<(), DbError> {
        let wtxn = self.db.begin_write().map_err(|e| DbError::Db(e.to_string()))?;
        {
            let mut table = wtxn
                .open_table(SRC_SNAPSHOTS)
                .map_err(|e| DbError::Db(e.to_string()))?;
            table
                .insert(path, content)
                .map_err(|e| DbError::Db(e.to_string()))?;
        }
        wtxn.commit().map_err(|e| DbError::Db(e.to_string()))?;
        Ok(())
    }

    // ── var_defs ─────────────────────────────────────────────────────────────

    /// Record a `%set(var_name, ...)` call site.
    /// `pos` and `length` are absolute byte offsets in `src_file`.
    pub fn record_var_def(&self, var_name: &str, src_file: &str, pos: u32, length: u32) -> Result<(), DbError> {
        let key = def_key(var_name, src_file, pos);
        let val = postcard::to_allocvec(&length).map_err(DbError::Serialize)?;
        let wtxn = self.db.begin_write().map_err(|e| DbError::Db(e.to_string()))?;
        {
            let mut table = wtxn.open_table(VAR_DEFS).map_err(|e| DbError::Db(e.to_string()))?;
            table.insert(key.as_str(), val.as_slice()).map_err(|e| DbError::Db(e.to_string()))?;
        }
        wtxn.commit().map_err(|e| DbError::Db(e.to_string()))?;
        Ok(())
    }

    /// Return all `(src_file, pos, length)` entries for `var_name`.
    pub fn query_var_defs(&self, var_name: &str) -> Result<Vec<(String, u32, u32)>, DbError> {
        let prefix = def_prefix(var_name);
        let rtxn = self.db.begin_read().map_err(|e| DbError::Db(e.to_string()))?;
        let table = rtxn.open_table(VAR_DEFS).map_err(|e| DbError::Db(e.to_string()))?;
        let mut out = Vec::new();
        for item in table.iter().map_err(|e| DbError::Db(e.to_string()))? {
            let (k, v) = item.map_err(|e| DbError::Db(e.to_string()))?;
            let key = k.value();
            if !key.starts_with(&prefix) { continue; }
            // key = "{var_name}\x00{src_file}\x00{pos:010}"
            let rest = &key[prefix.len()..];
            if let Some(sep) = rest.rfind('\x00') {
                let src_file = &rest[..sep];
                let pos: u32 = rest[sep + 1..].parse().unwrap_or(0);
                let length: u32 = postcard::from_bytes(v.value()).map_err(DbError::Serialize)?;
                out.push((src_file.to_string(), pos, length));
            }
        }
        Ok(out)
    }

    // ── macro_defs ────────────────────────────────────────────────────────────

    /// Record a `%def/%rhaidef/%pydef(macro_name, ...)` call site.
    pub fn record_macro_def(&self, macro_name: &str, src_file: &str, pos: u32, length: u32) -> Result<(), DbError> {
        let key = def_key(macro_name, src_file, pos);
        let val = postcard::to_allocvec(&length).map_err(DbError::Serialize)?;
        let wtxn = self.db.begin_write().map_err(|e| DbError::Db(e.to_string()))?;
        {
            let mut table = wtxn.open_table(MACRO_DEFS).map_err(|e| DbError::Db(e.to_string()))?;
            table.insert(key.as_str(), val.as_slice()).map_err(|e| DbError::Db(e.to_string()))?;
        }
        wtxn.commit().map_err(|e| DbError::Db(e.to_string()))?;
        Ok(())
    }

    /// Return all `(src_file, pos, length)` entries for `macro_name`.
    pub fn query_macro_defs(&self, macro_name: &str) -> Result<Vec<(String, u32, u32)>, DbError> {
        let prefix = def_prefix(macro_name);
        let rtxn = self.db.begin_read().map_err(|e| DbError::Db(e.to_string()))?;
        let table = rtxn.open_table(MACRO_DEFS).map_err(|e| DbError::Db(e.to_string()))?;
        let mut out = Vec::new();
        for item in table.iter().map_err(|e| DbError::Db(e.to_string()))? {
            let (k, v) = item.map_err(|e| DbError::Db(e.to_string()))?;
            let key = k.value();
            if !key.starts_with(&prefix) { continue; }
            let rest = &key[prefix.len()..];
            if let Some(sep) = rest.rfind('\x00') {
                let src_file = &rest[..sep];
                let pos: u32 = rest[sep + 1..].parse().unwrap_or(0);
                let length: u32 = postcard::from_bytes(v.value()).map_err(DbError::Serialize)?;
                out.push((src_file.to_string(), pos, length));
            }
        }
        Ok(out)
    }
}
