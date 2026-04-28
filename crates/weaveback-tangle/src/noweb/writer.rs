// weaveback-tangle/src/noweb/writer.rs
// I'd Really Rather You Didn't edit this generated file.

pub struct ChunkWriter<'a> {
    safe_file_writer: &'a mut SafeFileWriter,
}

impl<'a> ChunkWriter<'a> {
    pub fn new(sw: &'a mut SafeFileWriter) -> Self {
        Self {
            safe_file_writer: sw,
        }
    }

    /// Write a single `@file` chunk.  Returns the final on-disk bytes (after
    /// any configured formatter has run) so the caller can use them for
    /// source-map remapping without an extra read.  Returns `None` for
    /// absolute-path chunks (written directly, not through `SafeFileWriter`).
    pub fn write_chunk(
        &mut self,
        chunk_name: &str,
        content: &[String],
    ) -> Result<Option<Vec<u8>>, WeavebackError> {
        if !chunk_name.starts_with("@file ") {
            return Ok(None);
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
            Ok(None)
        } else {
            let final_path = self.safe_file_writer.before_write(path_str)?;
            let mut f = fs::File::create(&final_path)?;
            for line in content {
                f.write_all(line.as_bytes())?;
            }
            let written = self.safe_file_writer.after_write(path_str)?;
            Ok(Some(written))
        }
    }
}

