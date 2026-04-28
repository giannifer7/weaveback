# Noweb Path Safety

Tilde expansion and output path safety validation.

### Path safety

Two free functions gate output paths before any content is written.

`path_is_safe` rejects literal absolute paths, Windows-style drive paths, and
`..` traversal components.  It runs on every `@file` chunk name at parse time.

`expand_tilde` replaces a leading `~` with `$HOME` on Unix.  A tilde-expanded
path resolves to an absolute path outside `gen/` — it therefore bypasses
`path_is_safe` (which would reject it) and goes instead through the
`allow_home` gate in `ChunkWriter::write_chunk`.

```rust
// <[noweb-path-utils]>=
fn expand_tilde(path: &str) -> String {
    if path == "~" {
        return std::env::var("HOME").unwrap_or_else(|_| path.to_string());
    }
    if let Some(rest) = path.strip_prefix("~/") {
        let home = std::env::var("HOME").unwrap_or_else(|_| "~".to_string());
        return format!("{}/{}", home, rest);
    }
    path.to_string()
}

fn path_is_safe(path: &str) -> Result<(), SafeWriterError> {
    let p = Path::new(path);
    if p.is_absolute() {
        return Err(SafeWriterError::SecurityViolation(
            "Absolute paths are not allowed".to_string(),
        ));
    }
    if p.to_string_lossy().contains(':') {
        return Err(SafeWriterError::SecurityViolation(
            "Windows-style paths are not allowed".to_string(),
        ));
    }
    if p.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(SafeWriterError::SecurityViolation(
            "Path traversal is not allowed".to_string(),
        ));
    }
    Ok(())
}
// @
```

