use crate::db::{WeavebackDb, DbError};
use shlex;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Read;
use std::io::{self, BufReader};
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SafeWriterError {
    #[error("IO error: {0}")]
    IoError(#[from] io::Error),
    #[error("Failed to create directory: {0}")]
    DirectoryCreationFailed(PathBuf),
    #[error("Failed to create backup for: {0}")]
    BackupFailed(PathBuf),
    #[error("File was modified externally: {0}")]
    ModifiedExternally(PathBuf),
    #[error("Security violation: {0}")]
    SecurityViolation(String),
    #[error("Formatter error: {0}")]
    FormatterError(String),
    #[error("Database error: {0}")]
    DbError(#[from] DbError),
}
#[derive(Debug, Clone)]
pub struct SafeWriterConfig {
    pub buffer_size: usize,
    pub formatters: HashMap<String, String>, // file-extension → shell command
    /// Allow `@file ~/...` chunks to write outside the gen/ sandbox.
    /// Default `false`: tilde-expanded (absolute) paths are rejected unless
    /// the user explicitly passes `--allow-home`.
    pub allow_home: bool,
    /// Override modification detection for generated files and always rewrite
    /// them from the current literate source.
    pub force_generated: bool,
}

impl Default for SafeWriterConfig {
    fn default() -> Self {
        SafeWriterConfig {
            buffer_size: 8192,
            formatters: HashMap::new(),
            allow_home: false,
            force_generated: false,
        }
    }
}
pub struct SafeFileWriter {
    gen_base: PathBuf,
    db: WeavebackDb,
    config: SafeWriterConfig,
    /// Staging area: logical file name → temp file on disk.
    /// The NamedTempFile is kept alive here until after_write consumes it.
    staging: HashMap<String, NamedTempFile>,
}

impl SafeFileWriter {
    pub fn new<P: AsRef<Path>>(gen_base: P) -> Result<Self, SafeWriterError> {
        Self::with_config(gen_base, SafeWriterConfig::default())
    }

    pub fn with_config<P: AsRef<Path>>(
        gen_base: P,
        config: SafeWriterConfig,
    ) -> Result<Self, SafeWriterError> {
        fs::create_dir_all(gen_base.as_ref())
            .map_err(|_| SafeWriterError::DirectoryCreationFailed(gen_base.as_ref().to_path_buf()))?;
        let gen_base = gen_base
            .as_ref()
            .canonicalize()
            .map_err(SafeWriterError::IoError)?;

        let db = WeavebackDb::open_temp().map_err(SafeWriterError::DbError)?;

        Ok(SafeFileWriter {
            gen_base,
            db,
            config,
            staging: HashMap::new(),
        })
    }
}
impl SafeFileWriter {
    fn atomic_copy<P: AsRef<Path>>(&self, source: P, destination: P) -> io::Result<()> {
        let destination = destination.as_ref();
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        let temp_path = destination.with_extension("tmp");

        if temp_path.exists() {
            let _ = fs::remove_file(&temp_path);
        }

        {
            let mut source_file = fs::File::open(&source)?;
            let mut temp_file = fs::File::create(&temp_path)?;
            io::copy(&mut source_file, &mut temp_file)?;
            temp_file.sync_all()?;
        }

        fs::rename(temp_path, destination)?;
        Ok(())
    }

    fn copy_if_different<P: AsRef<Path>>(
        &self,
        source: P,
        destination: P,
    ) -> Result<(), SafeWriterError> {
        let source = source.as_ref();
        let destination = destination.as_ref();

        if !destination.exists() {
            return self
                .atomic_copy(source, destination)
                .map_err(SafeWriterError::from);
        }

        let are_different = {
            let mut source_file =
                BufReader::with_capacity(self.config.buffer_size, File::open(source)?);
            let mut dest_file =
                BufReader::with_capacity(self.config.buffer_size, File::open(destination)?);

            let mut src_buf = vec![0u8; self.config.buffer_size];
            let mut dst_buf = vec![0u8; self.config.buffer_size];
            loop {
                let src_n = source_file.read(&mut src_buf)?;
                let dst_n = dest_file.read(&mut dst_buf)?;
                if src_n != dst_n || src_buf[..src_n] != dst_buf[..dst_n] {
                    break true;
                }
                if src_n == 0 {
                    break false;
                }
            }
        };

        if are_different {
            eprintln!("file {} changed", destination.display());
            self.atomic_copy(source, destination)?;
        }

        Ok(())
    }

    fn run_formatter(&self, command: &str, file: &Path) -> Result<(), SafeWriterError> {
        let parts = shlex::split(command).ok_or_else(|| {
            SafeWriterError::FormatterError(format!(
                "could not parse formatter command: '{}'", command
            ))
        })?;
        if parts.is_empty() {
            return Err(SafeWriterError::FormatterError(
                "formatter command is empty".to_string(),
            ));
        }
        let status = std::process::Command::new(&parts[0])
            .args(&parts[1..])
            .arg(file)
            .status()
            .map_err(|e| {
                SafeWriterError::FormatterError(format!("could not run '{}': {}", command, e))
            })?;
        if !status.success() {
            return Err(SafeWriterError::FormatterError(format!(
                "'{}' exited with code {}",
                command,
                status.code().unwrap_or(-1)
            )));
        }
        Ok(())
    }

    fn trim_trailing_whitespace(&self, path: &Path) -> io::Result<()> {
        let content = fs::read(path)?;
        if let Ok(text) = std::str::from_utf8(&content) {
            let ends_with_newline = content.last() == Some(&b'\n');
            let mut result = Vec::with_capacity(content.len());
            for line in text.lines() {
                result.extend_from_slice(
                    line.trim_end_matches([' ', '\t', '\r']).as_bytes()
                );
                result.push(b'\n');
            }
            if !ends_with_newline && result.last() == Some(&b'\n') {
                result.pop();
            }
            if result.len() == 1 && result[0] == b'\n' && content.is_empty() {
                result.clear();
            }
            fs::write(path, result)?;
        }
        Ok(())
    }
}
impl SafeFileWriter {
    pub fn before_write<P: AsRef<Path>>(
        &mut self,
        file_name: P,
    ) -> Result<PathBuf, SafeWriterError> {
        validate_filename(file_name.as_ref())?;
        let path = file_name.as_ref();

        let dest_dir = path.parent().unwrap_or_else(|| Path::new(""));
        fs::create_dir_all(self.gen_base.join(dest_dir))
            .map_err(|_| SafeWriterError::DirectoryCreationFailed(self.gen_base.join(dest_dir)))?;

        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let suffix = if ext.is_empty() {
            String::new()
        } else {
            format!(".{ext}")
        };
        let tmp = tempfile::Builder::new()
            .suffix(&suffix)
            .tempfile()
            .map_err(SafeWriterError::IoError)?;
        let tmp_path = tmp.path().to_path_buf();
        self.staging.insert(path.to_string_lossy().into_owned(), tmp);
        Ok(tmp_path)
    }

    /// Run the post-write pipeline and return the final (possibly formatted)
    /// file content as bytes.  The caller can use these bytes directly for
    /// source-map remapping without re-reading the output file from disk.
    pub fn after_write<P: AsRef<Path>>(&mut self, file_name: P) -> Result<Vec<u8>, SafeWriterError> {
        validate_filename(file_name.as_ref())?;
        let key = file_name.as_ref().to_string_lossy().into_owned();
        let tmp = self
            .staging
            .remove(&key)
            .ok_or_else(|| SafeWriterError::BackupFailed(file_name.as_ref().to_path_buf()))?;
        let tmp_path = tmp.path().to_path_buf();
        let output_file = self.gen_base.join(file_name.as_ref());

        // Step 1: run formatter on temp copy if configured.
        let ext = file_name
            .as_ref()
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        let mut formatted = false;
        if let Some(cmd) = self.config.formatters.get(ext).cloned() {
            let pre_size = fs::metadata(&tmp_path).map(|m| m.len()).unwrap_or(0);
            self.run_formatter(&cmd, &tmp_path)?;
            if pre_size > 0 {
                let post_size = fs::metadata(&tmp_path).map(|m| m.len()).unwrap_or(0);
                if post_size == 0 {
                    return Err(SafeWriterError::FormatterError(format!(
                        "formatter '{cmd}' produced an empty file (input was {pre_size} bytes)"
                    )));
                }
            }
            formatted = true;
        }

        if !formatted {
            self.trim_trailing_whitespace(&tmp_path)?;
        }

        // Step 2: content-based modification detection.
        // When a stored baseline exists, compare the on-disk file against it:
        // any difference means the file was hand-edited since the last tangle.
        // When no baseline exists (fresh checkout or reset db), compare against
        // what tangle is about to write: in a consistent literate project the
        // committed generated file should match the committed .adoc, so any
        // difference still indicates a hand-edit.
        if output_file.is_file() && !self.config.force_generated {
            let current = fs::read(&output_file)?;
            let reference = match self.db.get_baseline(&key)? {
                Some(b) => b,
                None => fs::read(&tmp_path)?,
            };
            if current != reference {
                return Err(SafeWriterError::ModifiedExternally(output_file));
            }
        }

        // Step 3: copy temp → output.
        // Normally skip the copy when content is identical (keeps build-system
        // timestamps stable).  When force_generated is set we always overwrite —
        // that is the whole point of the flag.
        if self.config.force_generated {
            self.atomic_copy(&tmp_path, &output_file)
                .map_err(SafeWriterError::from)?;
        } else {
            self.copy_if_different(&tmp_path, &output_file)?;
        }

        // Step 4: read the (possibly formatted) temp content for the baseline
        // and return it to the caller so they don't need a second disk read.
        let written = fs::read(&tmp_path)
            .map_err(|_| SafeWriterError::BackupFailed(tmp_path.clone()))?;
        self.db
            .set_baseline(&key, &written)
            .map_err(SafeWriterError::DbError)?;

        // tmp is dropped here, deleting the temp file.
        Ok(written)
    }
}
impl SafeFileWriter {
    pub fn get_config(&self) -> &SafeWriterConfig {
        &self.config
    }

    pub fn set_config(&mut self, config: SafeWriterConfig) {
        self.config = config;
    }

    pub fn db(&self) -> &WeavebackDb {
        &self.db
    }

    pub fn db_mut(&mut self) -> &mut WeavebackDb {
        &mut self.db
    }

    pub fn finish(self, target: &Path) -> Result<(), SafeWriterError> {
        self.db.merge_into(target).map_err(SafeWriterError::DbError)?;
        Ok(())
    }

    pub fn get_gen_base(&self) -> &Path {
        &self.gen_base
    }

    /// Retrieve the stored baseline bytes for a relative path (test helper).
    #[cfg(test)]
    pub fn get_baseline_for_test(&self, path: &str) -> Option<Vec<u8>> {
        self.db.get_baseline(path).ok().flatten()
    }
}
fn validate_filename(path: &Path) -> Result<(), SafeWriterError> {
    use std::path::Component;

    if path.is_absolute() {
        return Err(SafeWriterError::SecurityViolation(format!(
            "Absolute paths are not allowed: {}",
            path.display()
        )));
    }

    let filename = path.to_string_lossy();
    if filename.len() >= 2 {
        let mut chars = filename.chars();
        let first = chars.next().unwrap();
        let second = chars.next().unwrap();
        if second == ':' && first.is_ascii_alphabetic() {
            return Err(SafeWriterError::SecurityViolation(format!(
                "Windows-style absolute paths are not allowed: {}",
                filename
            )));
        }
    }

    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(SafeWriterError::SecurityViolation(format!(
            "Path traversal detected (..): {}",
            path.display()
        )));
    }

    Ok(())
}
