// weaveback-tangle/src/db/config.rs
// I'd Really Rather You Didn't edit this generated file.

impl WeavebackDb {
    pub fn set_source_config(
        &self,
        src_file: &str,
        cfg: &TangleConfig,
    ) -> Result<(), DbError> {
        let file_id = intern_file(&self.conn, src_file)?;
        self.conn.execute(
            "INSERT OR REPLACE INTO literate_source_config
             (src_file, sigil, open_delim, close_delim, chunk_end, comment_markers)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                file_id,
                cfg.sigil.to_string(),
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
            "SELECT lsc.sigil, lsc.open_delim, lsc.close_delim,
                    lsc.chunk_end, lsc.comment_markers
             FROM literate_source_config lsc JOIN files f ON f.id = lsc.src_file
             WHERE f.path = ?1",
        )?;
        Ok(stmt.query_row(params![src_file], |row| {
            let sc_str: String = row.get(0)?;
            let sigil = sc_str.chars().next().unwrap_or('%');
            let cm_str: String = row.get(4)?;
            let comment_markers = cm_str.split(',').map(|s| s.to_string()).collect();
            Ok(TangleConfig {
                sigil,
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

