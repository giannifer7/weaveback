// src/tests/utils.rs
use crate::safe_writer::SafeWriterConfig;
use crate::{AzadiError, SafeFileWriter};
use std::{fs, io::Write, path::PathBuf};
use tempfile::TempDir;

pub(crate) fn create_test_writer() -> (TempDir, SafeFileWriter) {
    let temp = TempDir::new().unwrap();
    let config = SafeWriterConfig {
        buffer_size: 8192,
        formatters: std::collections::HashMap::new(),
    };
    let writer = SafeFileWriter::with_config(temp.path().join("gen"), config).unwrap();
    (temp, writer)
}

pub(crate) fn write_file(
    writer: &mut SafeFileWriter,
    path: &PathBuf,
    content: &str,
) -> Result<(), AzadiError> {
    let private_path = writer.before_write(path)?;
    {
        let mut file = fs::File::create(&private_path)?;
        write!(file, "{}", content)?;
    }
    Ok(writer.after_write(path)?)
}
