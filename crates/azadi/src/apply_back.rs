// crates/azadi/src/apply_back.rs
//
// `azadi apply-back` — propagate edits from gen/ back to the literate source.
//
// Algorithm (per modified gen/ file):
//   1. Diff the current gen/ file against the stored baseline (gen_baselines).
//   2. For each changed output line, look up noweb_map to find the source file
//      and line that produced it.
//   3. Compare the current literate-source line against the original chunk text.
//      - Match   → apply the change.
//      - Already new → skip (idempotent).
//      - Neither  → conflict, warn and skip.
//   4. Write patched source files.
//   5. Update the baseline so the next `azadi` run sees no ModifiedExternally.
//
// Limitations (v1 — noweb level only):
//   - Lines whose gen/ counterpart has no map entry (e.g. chunk-reference lines
//     that expand to sub-chunks) are skipped.
//   - Size-changing hunks (Delete / Insert / Replace with length mismatch) are
//     skipped with a message; edit the literate source manually for those.
//   - Macro-expanded content: if a chunk line came from a %def/%rhaidef body the
//     snapshot won't match and the patch will be reported as a conflict (safe).

use azadi_noweb::db::{AzadiDb, DbError};
use similar::TextDiff;
use std::collections::HashMap;
use std::path::PathBuf;

// ── error type ───────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum ApplyBackError {
    Db(DbError),
    Io(std::io::Error),
}

impl From<DbError> for ApplyBackError {
    fn from(e: DbError) -> Self {
        ApplyBackError::Db(e)
    }
}

impl From<std::io::Error> for ApplyBackError {
    fn from(e: std::io::Error) -> Self {
        ApplyBackError::Io(e)
    }
}

impl std::fmt::Display for ApplyBackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApplyBackError::Db(e) => write!(f, "database error: {e}"),
            ApplyBackError::Io(e) => write!(f, "I/O error: {e}"),
        }
    }
}

// ── options ──────────────────────────────────────────────────────────────────

pub struct ApplyBackOptions {
    pub db_path: PathBuf,
    pub gen_dir: PathBuf,
    pub dry_run: bool,
    /// Relative paths within gen/ to process; empty = all modified files.
    pub files: Vec<String>,
}

// ── internal ─────────────────────────────────────────────────────────────────

/// A pending patch: replace `src_line` (0-indexed) in a source file.
struct Patch {
    src_line: usize,
    /// Original chunk text (baseline gen/ line with indent stripped).
    old_chunk_line: String,
    /// Desired chunk text (modified gen/ line with indent stripped).
    new_chunk_line: String,
}

fn strip_indent<'a>(line: &'a str, indent: &str) -> &'a str {
    line.strip_prefix(indent).unwrap_or(line)
}

// ── main entry point ─────────────────────────────────────────────────────────

pub fn run_apply_back(opts: ApplyBackOptions) -> Result<(), ApplyBackError> {
    if !opts.db_path.exists() {
        eprintln!(
            "Database not found at {}. Run azadi on your source files first.",
            opts.db_path.display()
        );
        return Ok(());
    }

    let db = AzadiDb::open(&opts.db_path)?;

    let baselines: Vec<(String, Vec<u8>)> = if opts.files.is_empty() {
        db.list_baselines()?
    } else {
        opts.files
            .iter()
            .filter_map(|f| db.get_baseline(f).ok().flatten().map(|b| (f.clone(), b)))
            .collect()
    };

    let mut any_changed = false;

    for (rel_path, baseline_bytes) in &baselines {
        let gen_path = opts.gen_dir.join(rel_path);
        let current_bytes = match std::fs::read(&gen_path) {
            Ok(b) => b,
            Err(_) => {
                eprintln!("  skip {}: file not found in gen/", rel_path);
                continue;
            }
        };

        if current_bytes == *baseline_bytes {
            continue; // unchanged
        }

        any_changed = true;
        println!("Processing {}", rel_path);

        let baseline_str = String::from_utf8_lossy(baseline_bytes);
        let current_str = String::from_utf8_lossy(&current_bytes);
        let baseline_lines: Vec<&str> = baseline_str.lines().collect();
        let current_lines: Vec<&str> = current_str.lines().collect();

        // Collect patches grouped by source file.
        let mut src_patches: HashMap<String, Vec<Patch>> = HashMap::new();
        let mut skipped = 0usize;

        let diff = TextDiff::from_lines(baseline_str.as_ref(), current_str.as_ref());
        for op in diff.ops() {
            match op {
                similar::DiffOp::Equal { .. } => {}

                similar::DiffOp::Replace {
                    old_index,
                    old_len,
                    new_index,
                    new_len,
                } => {
                    if old_len != new_len {
                        eprintln!(
                            "  skip lines {}-{}: size-changing hunk ({} → {} lines) — \
                             edit the literate source manually",
                            old_index + 1,
                            old_index + old_len,
                            old_len,
                            new_len,
                        );
                        skipped += old_len;
                        continue;
                    }
                    for i in 0..*old_len {
                        let out_line_0 = (old_index + i) as u32;
                        let old_line = baseline_lines.get(old_index + i).copied().unwrap_or("");
                        let new_line = current_lines.get(new_index + i).copied().unwrap_or("");

                        match db.get_noweb_entry(rel_path, out_line_0)? {
                            None => {
                                eprintln!(
                                    "  skip line {}: no source map entry",
                                    out_line_0 + 1
                                );
                                skipped += 1;
                            }
                            Some(entry) => {
                                let old_chunk =
                                    strip_indent(old_line, &entry.indent).to_string();
                                let new_chunk =
                                    strip_indent(new_line, &entry.indent).to_string();
                                src_patches
                                    .entry(entry.src_file.clone())
                                    .or_default()
                                    .push(Patch {
                                        src_line: entry.src_line as usize,
                                        old_chunk_line: old_chunk,
                                        new_chunk_line: new_chunk,
                                    });
                            }
                        }
                    }
                }

                similar::DiffOp::Delete { old_index, old_len, .. } => {
                    eprintln!(
                        "  skip lines {}-{}: {} deleted line(s) — \
                         remove from the literate source manually",
                        old_index + 1,
                        old_index + old_len,
                        old_len,
                    );
                    skipped += old_len;
                }

                similar::DiffOp::Insert { old_index, new_len, .. } => {
                    eprintln!(
                        "  skip {} inserted line(s) after gen/ line {} — \
                         add to the literate source manually",
                        new_len,
                        old_index,
                    );
                    skipped += new_len;
                }
            }
        }

        // Apply collected patches to each source file.
        for (src_file, patches) in &src_patches {
            apply_patches_to_file(src_file, patches, opts.dry_run, &mut skipped)?;
        }

        // Update the baseline so the next `azadi` run won't see ModifiedExternally.
        if opts.dry_run {
            println!("  [dry-run] would update baseline for {}", rel_path);
        } else if skipped == 0 {
            db.set_baseline(rel_path, &current_bytes)?;
            println!("  baseline updated for {}", rel_path);
        } else {
            println!(
                "  baseline NOT updated for {} ({} line(s) could not be applied)",
                rel_path, skipped
            );
        }
    }

    if !any_changed {
        println!("No modified gen/ files found.");
    }

    Ok(())
}

/// Read `src_file`, apply `patches`, and write it back (unless `dry_run`).
///
/// Uses `old_chunk_line` as the expected source-line text.  If the source was
/// also modified since the last `azadi` run the comparison will fail and the
/// patch is reported as a conflict.
fn apply_patches_to_file(
    src_file: &str,
    patches: &[Patch],
    dry_run: bool,
    skipped: &mut usize,
) -> Result<(), ApplyBackError> {
    let content = std::fs::read_to_string(src_file)?;
    let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
    let had_trailing_newline = content.ends_with('\n');

    let mut applied = 0;
    let mut conflicts = 0;

    for patch in patches {
        let idx = patch.src_line;
        if idx >= lines.len() {
            eprintln!(
                "  skip {}:{}: line index out of range (file has {} lines)",
                src_file,
                idx + 1,
                lines.len()
            );
            *skipped += 1;
            continue;
        }

        let current = &lines[idx];
        if current == &patch.old_chunk_line {
            // Source unchanged since last run → safe to apply.
            if dry_run {
                println!(
                    "  [dry-run] {}:{}: {:?} → {:?}",
                    src_file,
                    idx + 1,
                    patch.old_chunk_line,
                    patch.new_chunk_line,
                );
            } else {
                lines[idx] = patch.new_chunk_line.clone();
                println!("  {}:{}: patched", src_file, idx + 1);
            }
            applied += 1;
        } else if current == &patch.new_chunk_line {
            // Already the desired content (idempotent).
            println!("  {}:{}: already applied", src_file, idx + 1);
        } else {
            // The literate source was modified independently → conflict.
            eprintln!(
                "  CONFLICT {}:{}\n    expected: {:?}\n    current:  {:?}\n    desired:  {:?}",
                src_file,
                idx + 1,
                patch.old_chunk_line,
                current,
                patch.new_chunk_line,
            );
            conflicts += 1;
            *skipped += 1;
        }
    }

    if !dry_run && applied > 0 {
        let mut out = lines.join("\n");
        if had_trailing_newline {
            out.push('\n');
        }
        std::fs::write(src_file, out)?;
    }

    if conflicts > 0 {
        eprintln!("  {} conflict(s) in {}", conflicts, src_file);
    }

    Ok(())
}
