// weaveback-tangle/src/db/baselines.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;

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

