# DB Merge

Atomic temp-database merge into the persistent target database.

## merge_into

`merge_into` copies all tables from the in-memory database into the persistent
file database in a single write transaction.  The target is created and
WAL-initialized if it does not yet exist.

Because file paths are interned independently in each in-memory database their
integer IDs may differ.  The merge resolves this by first copying all `files`
rows into the target (`INSERT OR IGNORE`), then remapping each data table's ID
columns via subquery lookups: for every row, each file-ID column is translated
to the corresponding target ID by joining through the shared `path` string.

Tables without file-ID columns (`gen_baselines`, `src_snapshots`, `run_config`)
are copied with a simple `SELECT *`.

Some tables are authoritative snapshots for a touched source or output file,
not append-only logs. Before reinserting current rows, the merge therefore
deletes stale target rows for:

* output files touched in this run (`noweb_map`, keyed by `gen_baselines.path`)
* source files read in this run also clear their previous `noweb_map` rows,
  because an old bad output-path key may survive even when the source-side
  mapping is otherwise current
* source files read in this run (`chunk_defs`, `chunk_deps`,
  `literate_source_config`, `source_blocks`, `var_defs`, `macro_defs`,
  keyed by `src_snapshots.path`)

The source-file delete uses a suffix-aware match (`absolute == absolute` or
`absolute ends_with "/relative"`) because older databases may still contain
repo-relative `files.path` rows while `src_snapshots` stores absolute paths.

Without this delete phase, old rows survive when a generated file shrinks or a
source file stops defining a chunk/edge, which pollutes impact analysis and
trace lookups.

The `BEGIN IMMEDIATE`, inserts, and `COMMIT` are issued as separate
`execute_batch` calls so that an explicit `ROLLBACK` can be sent on any
failure.  `DETACH` always runs, even on error, to avoid leaking the attachment.

SQLite's `ATTACH DATABASE` does not support parameterized paths, so the path
is string-interpolated.  The `sqlite_string_literal` helper encapsulates
single-quote escaping to prevent injection.

```rust
// <[db-merge]>=
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

            // Snapshot tables need a replacement semantics for touched files.
            self.conn.execute_batch("
                DELETE FROM target.noweb_map
                 WHERE out_file IN (
                    SELECT t.id
                      FROM target.files t
                      JOIN gen_baselines gb ON gb.path = t.path
                 )
                    OR src_file IN (
                    SELECT t.id
                      FROM target.files t
                      JOIN src_snapshots ss
                        ON t.path = ss.path
                        OR ss.path LIKE ('%/' || t.path)
                 );

                DELETE FROM target.chunk_defs
                 WHERE src_file IN (
                    SELECT t.id
                      FROM target.files t
                      JOIN src_snapshots ss
                        ON t.path = ss.path
                        OR ss.path LIKE ('%/' || t.path)
                 );

                DELETE FROM target.chunk_deps
                 WHERE src_file IN (
                    SELECT t.id
                      FROM target.files t
                      JOIN src_snapshots ss
                        ON t.path = ss.path
                        OR ss.path LIKE ('%/' || t.path)
                 );

                DELETE FROM target.literate_source_config
                 WHERE src_file IN (
                    SELECT t.id
                      FROM target.files t
                      JOIN src_snapshots ss
                        ON t.path = ss.path
                        OR ss.path LIKE ('%/' || t.path)
                 );

                DELETE FROM target.source_blocks
                 WHERE src_file IN (
                    SELECT t.id
                      FROM target.files t
                      JOIN src_snapshots ss
                        ON t.path = ss.path
                        OR ss.path LIKE ('%/' || t.path)
                 );

                DELETE FROM target.var_defs
                 WHERE src_file IN (
                    SELECT t.id
                      FROM target.files t
                      JOIN src_snapshots ss
                        ON t.path = ss.path
                        OR ss.path LIKE ('%/' || t.path)
                 );

                DELETE FROM target.macro_defs
                 WHERE src_file IN (
                    SELECT t.id
                      FROM target.files t
                      JOIN src_snapshots ss
                        ON t.path = ss.path
                        OR ss.path LIKE ('%/' || t.path)
                 );
            ")?;

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
                    lsc.sigil, lsc.open_delim, lsc.close_delim,
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
// @
```

