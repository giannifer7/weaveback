// crates/azadi/src/apply_back.rs
//
// `azadi apply-back` — propagate edits from gen/ back to the literate source.
//
// Two-level algorithm (per modified gen/ file):
//
//   Level 1 (noweb):
//     Diff the current gen/ file against the stored baseline (gen_baselines).
//     For each changed output line, look up noweb_map to find the expanded-text
//     source file + line that produced it.
//
//   Level 2 (macro, enabled when eval_config is present):
//     Call perform_trace() to re-evaluate the driver in tracing mode and pinpoint
//     the *original* literal source location (src_file:src_line) and kind:
//       - Literal / MacroBody-without-vars: auto-patch in place
//       - MacroBody-with-vars: report location, skip auto-patch
//       - MacroArg: report call-site, skip auto-patch
//       - VarBinding / Computed: report, skip
//
//   Fuzzy line matching (Rust regex, no external process):
//     When the exact source line is not found at the expected index, search a
//     ±15-line window using a whitespace-normalised regex derived from the
//     expected text.  Avoids false conflicts from trivial reformatting.
//
//   After all patches are applied:
//     Update the baseline so the next `azadi` run won't see ModifiedExternally.
//     If any lines were skipped, the baseline is NOT updated for that file.

use azadi_macros::evaluator::EvalConfig;
use azadi_noweb::db::{AzadiDb, DbError};
use regex::Regex;
use similar::TextDiff;
use std::collections::HashMap;
use std::path::PathBuf;

use crate::lookup;

// ── error type ───────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum ApplyBackError {
    Db(DbError),
    Io(std::io::Error),
    Lookup(lookup::LookupError),
}

impl From<DbError> for ApplyBackError {
    fn from(e: DbError) -> Self { ApplyBackError::Db(e) }
}
impl From<std::io::Error> for ApplyBackError {
    fn from(e: std::io::Error) -> Self { ApplyBackError::Io(e) }
}
impl From<lookup::LookupError> for ApplyBackError {
    fn from(e: lookup::LookupError) -> Self { ApplyBackError::Lookup(e) }
}

impl std::fmt::Display for ApplyBackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApplyBackError::Db(e)     => write!(f, "database error: {e}"),
            ApplyBackError::Io(e)     => write!(f, "I/O error: {e}"),
            ApplyBackError::Lookup(e) => write!(f, "trace lookup error: {e:?}"),
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
    /// When present, enables two-level tracing through macro expansion.
    pub eval_config: Option<EvalConfig>,
}

// ── patch attribution ─────────────────────────────────────────────────────────

/// Where a patch lands and how to apply it.
enum PatchSource {
    /// Line from noweb-level expanded text (no macro attribution available).
    Noweb { src_file: String, src_line: usize },
    /// Literal text from the original literate source — safe to auto-patch.
    Literal { src_file: String, src_line: usize },
    /// Macro body text with no variable references — safe to auto-patch.
    MacroBodyLiteral { src_file: String, src_line: usize, macro_name: String },
    /// Macro body text containing `%(...)` references — report only.
    MacroBodyWithVars { src_file: String, src_line: usize, macro_name: String },
    /// Argument value at a macro call site — report only, no auto-patch.
    MacroArg {
        src_file: String,
        src_line: usize,
        src_col: u32,
        macro_name: String,
        param_name: String,
    },
    /// VarBinding or Computed — report only.
    Unpatchable { src_file: String, src_line: usize, kind_label: String },
}

impl PatchSource {
    fn src_file(&self) -> &str {
        match self {
            PatchSource::Noweb              { src_file, .. }
            | PatchSource::Literal          { src_file, .. }
            | PatchSource::MacroBodyLiteral { src_file, .. }
            | PatchSource::MacroBodyWithVars{ src_file, .. }
            | PatchSource::MacroArg         { src_file, .. }
            | PatchSource::Unpatchable      { src_file, .. } => src_file,
        }
    }
}

struct Patch {
    source: PatchSource,
    /// Indent-stripped baseline gen/ line (what the source *was*).
    old_text: String,
    /// Indent-stripped modified gen/ line (what the source *should become*).
    new_text: String,
}

// ── fuzzy line matching ───────────────────────────────────────────────────────

/// Search `lines` in a ±`window` range around `center` for a unique line that
/// matches a whitespace-normalised regex derived from `needle`.
///
/// Returns the 0-indexed line index on a unique match; `None` if not found or
/// ambiguous.  Uses the `regex` crate — no external processes.
fn fuzzy_find_line(lines: &[String], center: usize, needle: &str, window: usize) -> Option<usize> {
    let trimmed = needle.trim();
    if trimmed.is_empty() { return None; }

    // Build a pattern that tolerates interior whitespace changes.
    // regex::escape produces "\ " for spaces; we split on that and rejoin
    // with \s+ so "foo  bar" matches "foo bar" and vice-versa.
    let escaped = regex::escape(trimmed);
    let parts: Vec<&str> = escaped.split(r"\ ").collect();
    let pattern = parts.join(r"\s+");
    let re = Regex::new(&format!(r"^\s*{}\s*$", pattern)).ok()?;

    let lo = center.saturating_sub(window);
    let hi = (center + window).min(lines.len().saturating_sub(1));
    let mut found: Option<usize> = None;
    for i in lo..=hi {
        if re.is_match(&lines[i]) {
            if found.is_some() { return None; } // ambiguous
            found = Some(i);
        }
    }
    found
}

// ── resolve macro-level patch source ─────────────────────────────────────────

/// Call `perform_trace` for one output line and return a `PatchSource`
/// describing where the patch should land.
///
/// `special_char` is used to detect unresolved macro references in body text.
fn resolve_patch_source(
    rel_path: &str,
    out_line_0: u32,
    db: &AzadiDb,
    gen_dir: &std::path::Path,
    eval_config: &EvalConfig,
    nw_src_file: &str,
    nw_src_line: u32,
    snapshot: Option<&[u8]>,
    special_char: char,
) -> Result<PatchSource, ApplyBackError> {
    let trace = lookup::perform_trace(
        rel_path,
        out_line_0 + 1, // perform_trace takes 1-indexed line
        0,              // col=0 gives first-token attribution for the line
        db,
        gen_dir,
        eval_config.clone(),
    )?;

    let Some(json) = trace else {
        return Ok(PatchSource::Noweb {
            src_file: nw_src_file.to_string(),
            src_line: nw_src_line as usize,
        });
    };

    let obj = json.as_object().unwrap();

    let (src_file, src_line_0) = match (obj.get("src_file"), obj.get("src_line")) {
        (Some(sf), Some(sl)) => {
            let sf = sf.as_str().unwrap_or(nw_src_file).to_string();
            let sl = sl.as_u64().unwrap_or(nw_src_line as u64 + 1) as usize - 1;
            (sf, sl)
        }
        _ => return Ok(PatchSource::Noweb {
            src_file: nw_src_file.to_string(),
            src_line: nw_src_line as usize,
        }),
    };

    let kind = obj.get("kind").and_then(|k| k.as_str()).unwrap_or("Literal");

    match kind {
        "Literal" => Ok(PatchSource::Literal { src_file, src_line: src_line_0 }),

        "MacroBody" => {
            let macro_name = obj.get("macro_name")
                .and_then(|v| v.as_str()).unwrap_or("?").to_string();

            // Check if the source line contains unresolved macro calls.
            // Use the src_snapshots entry for the canonical (at-generation-time) text.
            let snap_line = snapshot.and_then(|bytes| {
                let s = String::from_utf8_lossy(bytes);
                s.lines().nth(src_line_0).map(|l| l.to_string())
            });
            let has_vars = snap_line.as_deref()
                .map_or(true, |l| l.contains(special_char));

            if has_vars {
                Ok(PatchSource::MacroBodyWithVars { src_file, src_line: src_line_0, macro_name })
            } else {
                Ok(PatchSource::MacroBodyLiteral { src_file, src_line: src_line_0, macro_name })
            }
        }

        "MacroArg" => {
            let macro_name  = obj.get("macro_name") .and_then(|v| v.as_str()).unwrap_or("?").to_string();
            let param_name  = obj.get("param_name") .and_then(|v| v.as_str()).unwrap_or("?").to_string();
            let src_col     = obj.get("src_col")    .and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            Ok(PatchSource::MacroArg { src_file, src_line: src_line_0, src_col, macro_name, param_name })
        }

        other => Ok(PatchSource::Unpatchable {
            src_file,
            src_line: src_line_0,
            kind_label: other.to_string(),
        }),
    }
}

// ── inner patch application ───────────────────────────────────────────────────

/// Try to apply one line replacement to `lines` at the expected `src_line`.
/// Falls back to fuzzy search in a ±15-line window.
/// `macro_name` is shown in the log when the patch came from a macro body.
#[allow(clippy::too_many_arguments)]
fn do_patch(
    src_file: &str,
    src_line: &usize,
    patch: &Patch,
    lines: &mut Vec<String>,
    dry_run: bool,
    skipped: &mut usize,
    applied: &mut usize,
    conflicts: &mut usize,
    macro_name: Option<&str>,
) {
    let hint = *src_line;
    let label = match macro_name {
        Some(m) => format!("{}:{} (macro body `{}`)", src_file, hint + 1, m),
        None    => format!("{}:{}", src_file, hint + 1),
    };

    let effective_idx = if hint < lines.len() && lines[hint] == patch.old_text {
        hint
    } else if hint < lines.len() && lines[hint] == patch.new_text {
        println!("  {}: already applied", label);
        return;
    } else {
        match fuzzy_find_line(lines, hint, &patch.old_text, 15) {
            Some(fi) if lines[fi] == patch.new_text => {
                println!("  {}:{}: already applied (fuzzy)", src_file, fi + 1);
                return;
            }
            Some(fi) => fi,
            None => {
                eprintln!(
                    "  CONFLICT {}\n    expected: {:?}\n    current:  {:?}\n    desired:  {:?}",
                    label,
                    patch.old_text,
                    lines.get(hint).map(|s| s.as_str()).unwrap_or("<out of range>"),
                    patch.new_text,
                );
                *conflicts += 1;
                *skipped += 1;
                return;
            }
        }
    };

    if dry_run {
        println!("  [dry-run] {}:{}: {:?} → {:?}", src_file, effective_idx + 1,
                 patch.old_text, patch.new_text);
    } else {
        lines[effective_idx] = patch.new_text.clone();
        println!("  {}: patched", label);
    }
    *applied += 1;
}

// ── apply patches to one source file ─────────────────────────────────────────

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
        match &patch.source {
            PatchSource::MacroBodyWithVars { src_line, macro_name, .. } => {
                eprintln!(
                    "  MANUAL {}:{}: macro body `{}` contains variables — edit manually\n    desired: {:?}",
                    src_file, src_line + 1, macro_name, patch.new_text,
                );
                *skipped += 1;
                continue;
            }

            PatchSource::MacroArg { src_line, src_col, macro_name, param_name, .. } => {
                eprintln!(
                    "  MANUAL {}:{}:{}: argument `{}` of `{}` — edit manually\n    desired: {:?}",
                    src_file, src_line + 1, src_col, param_name, macro_name, patch.new_text,
                );
                *skipped += 1;
                continue;
            }

            PatchSource::Unpatchable { src_line, kind_label, .. } => {
                eprintln!(
                    "  SKIP {}:{}: {} — cannot auto-patch",
                    src_file, src_line + 1, kind_label,
                );
                *skipped += 1;
                continue;
            }

            PatchSource::Noweb { src_line, .. }
            | PatchSource::Literal { src_line, .. } => {
                do_patch(src_file, src_line, patch, &mut lines,
                         dry_run, skipped, &mut applied, &mut conflicts, None);
            }

            PatchSource::MacroBodyLiteral { src_line, macro_name, .. } => {
                do_patch(src_file, src_line, patch, &mut lines,
                         dry_run, skipped, &mut applied, &mut conflicts,
                         Some(macro_name.as_str()));
            }
        }
    }

    if !dry_run && applied > 0 {
        let mut out = lines.join("\n");
        if had_trailing_newline { out.push('\n'); }
        std::fs::write(src_file, out)?;
    }

    if conflicts > 0 {
        eprintln!("  {} conflict(s) in {}", conflicts, src_file);
    }

    Ok(())
}

// ── strip noweb indent ────────────────────────────────────────────────────────

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

    let special_char = opts.eval_config.as_ref().map_or('%', |ec| ec.special_char);

    // Snapshot cache: driver path → bytes (or None).  Populated lazily.
    let mut snapshot_cache: HashMap<String, Option<Vec<u8>>> = HashMap::new();

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

        if current_bytes == *baseline_bytes { continue; }

        any_changed = true;
        println!("Processing {}", rel_path);

        let baseline_str = String::from_utf8_lossy(baseline_bytes);
        let current_str  = String::from_utf8_lossy(&current_bytes);
        let baseline_lines: Vec<&str> = baseline_str.lines().collect();
        let current_lines:  Vec<&str> = current_str.lines().collect();

        // Patches grouped by true source file.
        let mut src_patches: HashMap<String, Vec<Patch>> = HashMap::new();
        let mut skipped = 0usize;

        let diff = TextDiff::from_lines(baseline_str.as_ref(), current_str.as_ref());
        for op in diff.ops() {
            match op {
                similar::DiffOp::Equal { .. } => {}

                similar::DiffOp::Replace { old_index, old_len, new_index, new_len } => {
                    if old_len != new_len {
                        eprintln!(
                            "  skip lines {}-{}: size-changing hunk ({} → {} lines) — edit literate source manually",
                            old_index + 1, old_index + old_len, old_len, new_len,
                        );
                        skipped += old_len;
                        continue;
                    }
                    for i in 0..*old_len {
                        let out_line_0 = (old_index + i) as u32;
                        let old_line = baseline_lines.get(old_index + i).copied().unwrap_or("");
                        let new_line = current_lines .get(new_index  + i).copied().unwrap_or("");

                        match db.get_noweb_entry(rel_path, out_line_0)? {
                            None => {
                                eprintln!("  skip line {}: no source map entry", out_line_0 + 1);
                                skipped += 1;
                            }
                            Some(entry) => {
                                let old_text = strip_indent(old_line, &entry.indent).to_string();
                                let new_text = strip_indent(new_line, &entry.indent).to_string();

                                // Lazy-load snapshot for the source file.
                                let snap = snapshot_cache
                                    .entry(entry.src_file.clone())
                                    .or_insert_with(|| {
                                        db.get_src_snapshot(&entry.src_file).ok().flatten()
                                    })
                                    .as_deref();

                                let source = if let Some(ec) = &opts.eval_config {
                                    resolve_patch_source(
                                        rel_path, out_line_0,
                                        &db, &opts.gen_dir, ec,
                                        &entry.src_file, entry.src_line,
                                        snap, special_char,
                                    )?
                                } else {
                                    PatchSource::Noweb {
                                        src_file: entry.src_file.clone(),
                                        src_line: entry.src_line as usize,
                                    }
                                };

                                let file_key = source.src_file().to_string();
                                src_patches
                                    .entry(file_key)
                                    .or_default()
                                    .push(Patch { source, old_text, new_text });
                            }
                        }
                    }
                }

                similar::DiffOp::Delete { old_index, old_len, .. } => {
                    eprintln!(
                        "  skip lines {}-{}: {} deleted line(s) — remove from literate source manually",
                        old_index + 1, old_index + old_len, old_len,
                    );
                    skipped += old_len;
                }

                similar::DiffOp::Insert { old_index, new_len, .. } => {
                    eprintln!(
                        "  skip {} inserted line(s) after gen/ line {} — add to literate source manually",
                        new_len, old_index,
                    );
                    skipped += new_len;
                }
            }
        }

        // Apply collected patches to each source file.
        for (src_file, patches) in &src_patches {
            apply_patches_to_file(src_file, patches, opts.dry_run, &mut skipped)?;
        }

        if opts.dry_run {
            println!("  [dry-run] would update baseline for {}", rel_path);
        } else if skipped == 0 {
            db.set_baseline(rel_path, &current_bytes)?;
            println!("  baseline updated for {}", rel_path);
        } else {
            println!(
                "  baseline NOT updated for {} ({} line(s) could not be applied)",
                rel_path, skipped,
            );
        }
    }

    if !any_changed {
        println!("No modified gen/ files found.");
    }

    Ok(())
}
