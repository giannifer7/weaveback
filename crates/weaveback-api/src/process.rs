// weaveback-api/src/process.rs
// I'd Really Rather You Didn't edit this generated file.

use std::path::{Path, PathBuf};
use std::collections::{HashMap, HashSet};
use weaveback_macro::evaluator::{EvalConfig, EvalError, Evaluator};
use weaveback_macro::macro_api::{discover_includes_in_string, process_string};
use weaveback_tangle::{Clip, SafeFileWriter, SafeWriterConfig, WeavebackError};

/// Combined error type for a single tangle pass.
#[derive(Debug, thiserror::Error)]
pub enum ProcessError {
    #[error("{0}")]
    Tangle(#[from] WeavebackError),
    #[error("{0}")]
    Macro(#[from] EvalError),
    #[error("{0}")]
    Io(#[from] std::io::Error),
}


/// All arguments needed for one tangle pass.
///
/// Constructed by the CLI layer from parsed `clap` args and passed to
/// `run_single_pass`.
pub struct SinglePassArgs {
    /// Explicit input file paths (used when `directory` is `None`).
    pub inputs: Vec<PathBuf>,
    /// Process all files with matching extensions under this directory.
    pub directory: Option<PathBuf>,
    /// Base directory for resolving relative `inputs` paths.
    pub input_dir: PathBuf,
    /// Output directory for generated files.
    pub gen_dir: PathBuf,
    /// Chunk opening delimiter (default `<<`).
    pub open_delim: String,
    /// Chunk closing delimiter (default `>>`).
    pub close_delim: String,
    /// Chunk end marker (default `@`).
    pub chunk_end: String,
    /// Comma-separated comment markers (default `#,//`).
    pub comment_markers: String,
    /// File extension(s) to scan in `--dir` mode.
    pub ext: Vec<String>,
    /// Skip macro expansion and feed raw source directly to tangle.
    pub no_macros: bool,
    /// Prelude files evaluated before pass inputs in macro-enabled mode.
    pub macro_prelude: Vec<PathBuf>,
    /// Extension assigned to macro-expanded virtual documents before tangling.
    pub expanded_ext: Option<String>,
    /// Directory for expanded `.adoc` intermediates.
    pub expanded_adoc_dir: PathBuf,
    /// Directory for expanded `.md` intermediates.
    pub expanded_md_dir: PathBuf,
    /// Stop after macro expansion and write expanded documents.
    pub macro_only: bool,
    /// Print discovered `@file` chunk names and exit (no writes).
    pub dry_run: bool,
    /// Path to the weaveback SQLite database.
    pub db: PathBuf,
    /// Write a Makefile depfile to this path.
    pub depfile: Option<PathBuf>,
    /// Touch this file after a successful run (stamp target for `make`).
    pub stamp: Option<PathBuf>,
    /// Treat undefined chunk references as errors.
    pub strict: bool,
    /// Warn about defined-but-unused chunks.
    pub warn_unused: bool,
    /// Allow `%%env(NAME)` builtins to read environment variables.
    pub allow_env: bool,
    /// Allow writing generated files outside the home directory.
    pub allow_home: bool,
    /// Overwrite generated files even if they were externally modified.
    pub force_generated: bool,
    /// Macro sigil character (default `%%`).
    pub sigil: char,
    /// Path separator-separated include search paths.
    pub include: String,
    /// Formatter commands per output extension, e.g. `"rs=rustfmt"`.
    pub formatter: Vec<String>,
    /// Skip rebuilding the prose FTS index after this run.
    pub no_fts: bool,
    /// Print macro-expanded text to stderr before tangle processing.
    pub dump_expanded:  bool,
    /// Override project root (defaults to CWD).
    pub project_root:   Option<PathBuf>,
}

impl SinglePassArgs {
    #[cfg(test)]
    pub fn default_for_test() -> Self {
        Self {
            inputs: vec![],
            directory: None,
            input_dir: PathBuf::new(),
            gen_dir: PathBuf::new(),
            open_delim: "<<".to_string(),
            close_delim: ">>".to_string(),
            chunk_end: "@".to_string(),
            comment_markers: "//,#".to_string(),
            ext: vec!["adoc".to_string()],
            no_macros: true,
            macro_prelude: vec![],
            expanded_ext: None,
            expanded_adoc_dir: PathBuf::from("expanded-adoc"),
            expanded_md_dir: PathBuf::from("expanded-md"),
            macro_only: false,
            dry_run: false,
            db: PathBuf::new(),
            depfile: None,
            stamp: None,
            strict: false,
            warn_unused: false,
            allow_env: false,
            allow_home: true,
            force_generated: false,
            sigil: '%',
            include: String::new(),
            formatter: vec![],
            no_fts: true,
            dump_expanded: false,
            project_root: None,
        }
    }
}
/// Recursively collect all files whose extension matches any entry in `exts`.
pub fn find_files(dir: &std::path::Path, exts: &[String], out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            find_files(&path, exts, out)?;
        } else if let Some(e) = path.extension().and_then(|e| e.to_str())
            && exts.iter().any(|x| x == e)
        {
            out.push(path);
        }
    }
    Ok(())
}

fn depfile_escape(p: &std::path::Path) -> String {
    p.to_string_lossy().replace(' ', "\\ ")
}

/// Write a Makefile depfile: `target: dep1 dep2 …`.
pub fn write_depfile(path: &std::path::Path, target: &std::path::Path, deps: &[PathBuf]) -> std::io::Result<()> {
    use std::fmt::Write as FmtWrite;
    let mut out = String::new();
    std::write!(out, "{}:", depfile_escape(target)).unwrap();
    for dep in deps {
        std::write!(out, " {}", depfile_escape(dep)).unwrap();
    }
    out.push('\n');
    std::fs::write(path, out)
}
fn evaluate_macro_preludes(
    evaluator: &mut Evaluator,
    preludes: &[PathBuf],
) -> Result<(), String> {
    for prelude in preludes {
        let content = std::fs::read_to_string(prelude)
            .map_err(|e| format!("{}: {e}", prelude.display()))?;
        process_string(&content, Some(prelude), evaluator)
            .map_err(|e| format!("{}: {e}", prelude.display()))?;
    }
    Ok(())
}

fn with_replaced_extension(path: &Path, expanded_ext: Option<&str>) -> PathBuf {
    let mut out = path.to_path_buf();
    if let Some(ext) = expanded_ext.filter(|ext| !ext.is_empty()) {
        out.set_extension(ext.trim_start_matches('.'));
    }
    out
}

fn expanded_source_key(
    full_path: &Path,
    project_root: &Path,
    expanded_ext: Option<&str>,
) -> String {
    let expanded = with_replaced_extension(full_path, expanded_ext);
    if let Ok(rel) = expanded.strip_prefix(project_root) {
        rel.to_string_lossy().to_string()
    } else {
        expanded.to_string_lossy().to_string()
    }
}

fn expanded_output_path(
    full_path: &Path,
    base_dir: &Path,
    expanded_dir: &Path,
    expanded_ext: Option<&str>,
) -> PathBuf {
    let rel = if let Ok(path) = full_path.strip_prefix(base_dir) {
        path.to_path_buf()
    } else if let Some(name) = full_path.file_name() {
        PathBuf::from(name)
    } else {
        full_path.to_path_buf()
    };
    expanded_dir.join(with_replaced_extension(&rel, expanded_ext))
}

fn write_expanded_document(
    full_path: &Path,
    base_dir: &Path,
    expanded_adoc_dir: &Path,
    expanded_md_dir: &Path,
    expanded_ext: Option<&str>,
    expanded: &[u8],
) -> Result<PathBuf, String> {
    let expanded_dir = match expanded_ext.unwrap_or_default().trim_start_matches('.') {
        "md" | "markdown" => expanded_md_dir,
        _ => expanded_adoc_dir,
    };
    let out_path = expanded_output_path(full_path, base_dir, expanded_dir, expanded_ext);
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&out_path, expanded).map_err(|e| e.to_string())?;
    Ok(out_path)
}
use rayon::prelude::*;

/// Compute the set of `@file …` chunk names that can be skipped this run
/// because none of their contributing source blocks changed.
pub fn compute_skip_set(
    source_contents: &HashMap<String, String>,
    prev_db: &Option<weaveback_tangle::db::WeavebackDb>,
    current_db: &mut weaveback_tangle::db::WeavebackDb,
    gen_dir: &std::path::Path,
) -> HashSet<String> {
    use weaveback_tangle::parse_source_blocks;

    let parsed: Vec<(&String, Vec<_>)> = source_contents
        .par_iter()
        .map(|(path, content)| {
            let ext = std::path::Path::new(path.as_str())
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("");
            (path, parse_source_blocks(content, ext))
        })
        .collect();

    let mut dirty_chunks: HashSet<String> = HashSet::new();

    for (path, new_blocks) in &parsed {
        if let Err(e) = current_db.set_source_blocks(path, new_blocks) {
            eprintln!("warning: set_source_blocks failed for {path}: {e}");
            dirty_chunks.insert("*".to_string());
            continue;
        }

        let prev = prev_db.as_ref();
        let prev_hashes: HashMap<u32, Vec<u8>> = prev
            .and_then(|db| db.get_source_block_hashes(path).ok())
            .unwrap_or_default()
            .into_iter()
            .collect();

        for blk in new_blocks {
            let changed = prev_hashes
                .get(&blk.block_index)
                .map(|old| old.as_slice() != blk.content_hash.as_slice())
                .unwrap_or(true);

            if changed
                && let Some(db) = prev
                && let Ok(chunk_defs) = db.query_chunk_defs_overlapping(path, blk.line_start, blk.line_end) {
                    for def in chunk_defs {
                        dirty_chunks.insert(def.chunk_name.clone());
                    }
            }
        }
    }

    if let Some(db) = prev_db.as_ref() {
        let mut queue: Vec<String> = dirty_chunks.iter().cloned().collect();
        while let Some(chunk) = queue.pop() {
            if let Ok(rev_deps) = db.query_reverse_deps(&chunk) {
                for (from_chunk, _src_file) in rev_deps {
                    if dirty_chunks.insert(from_chunk.clone()) {
                        queue.push(from_chunk);
                    }
                }
            }
        }
    }

    if dirty_chunks.contains("*") {
        return HashSet::new();
    }

    let Some(prev) = prev_db.as_ref() else {
        return HashSet::new();
    };

    let all_file_chunks: Vec<String> = prev
        .list_chunk_defs(None)
        .unwrap_or_default()
        .into_iter()
        .filter(|e| e.chunk_name.starts_with("@file "))
        .map(|e| e.chunk_name)
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();

    let mut skip: HashSet<String> = HashSet::new();
    for name in all_file_chunks {
        if dirty_chunks.contains(&name) {
            continue;
        }
        let out_file = name.strip_prefix("@file ").unwrap_or(&name).trim();
        if prev.get_baseline(out_file).ok().flatten().is_some()
            && gen_dir.join(out_file).exists()
        {
            skip.insert(name);
        }
    }
    skip
}
use weaveback_tangle::db::WeavebackDb;

/// Run one tangle pass with the given arguments.
///
/// Returns `Err` on file I/O errors, macro evaluation failures, or tangle
/// errors.  Caller is responsible for printing a human-readable error.
pub fn run_single_pass(args: SinglePassArgs) -> Result<(), String> {
    let project_root = args.project_root.clone().unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    let _db = WeavebackDb::open(&args.db).map_err(|e| e.to_string())?;
    let pathsep: String = if cfg!(windows) { ";".to_string() } else { ":".to_string() };
    let include_paths: Vec<PathBuf> = args.include.split(&pathsep).map(PathBuf::from).collect();

    let eval_config = EvalConfig {
        sigil: args.sigil,
        include_paths: include_paths.clone(),
        allow_env: args.allow_env,
        ..EvalConfig::default()
    };
    let mut evaluator = Evaluator::new(eval_config.clone());
    if !args.no_macros {
        evaluate_macro_preludes(&mut evaluator, &args.macro_prelude)?;
    }

    let comment_markers: Vec<String> = args
        .comment_markers
        .split(',')
        .map(|s| s.trim().to_string())
        .collect();

    let formatters: HashMap<String, String> = args
        .formatter
        .iter()
        .filter_map(|s| s.split_once('=').map(|(e, c)| (e.to_string(), c.to_string())))
        .collect();

    let safe_writer = SafeFileWriter::with_config(
        &args.gen_dir,
        SafeWriterConfig {
            formatters,
            allow_home: args.allow_home,
            force_generated: args.force_generated,
            ..SafeWriterConfig::default()
        },
    ).map_err(|e| e.to_string())?;
    let mut clip = Clip::new(
        safe_writer,
        &args.open_delim,
        &args.close_delim,
        &args.chunk_end,
        &comment_markers,
    );
    clip.set_strict_undefined(args.strict);
    clip.set_warn_unused(args.warn_unused);

    let (drivers, all_adoc): (Vec<PathBuf>, Vec<PathBuf>) = if let Some(ref dir) = args.directory {
        let mut all = Vec::new();
        find_files(dir, &args.ext, &mut all).map_err(|e| e.to_string())?;
        all.sort();

        let mut included: HashSet<PathBuf> = HashSet::new();
        for adoc in &all {
            if let Ok(text) = std::fs::read_to_string(adoc) {
                let mut disc = Evaluator::new(eval_config.clone());
                if evaluate_macro_preludes(&mut disc, &args.macro_prelude).is_err() {
                    continue;
                }
                if let Ok(paths) = discover_includes_in_string(&text, Some(adoc), &mut disc) {
                    for p in paths {
                        included.insert(p.canonicalize().unwrap_or(p));
                    }
                }
            }
        }
        let drivers = all
            .iter()
            .filter(|f| {
                let canon = f.canonicalize().unwrap_or_else(|_| f.to_path_buf());
                !included.contains(&canon)
            })
            .cloned()
            .collect();
        (drivers, all)
    } else {
        let drivers = args.inputs.iter().map(|p| args.input_dir.join(p)).collect::<Vec<_>>();
        (drivers.clone(), drivers)
    };

    let prev_db = if args.db.exists() {
        weaveback_tangle::db::WeavebackDb::open_read_only(&args.db).ok()
    } else {
        None
    };

    let normalize_path = |p: &PathBuf| -> String {
        if let Ok(rel) = p.strip_prefix(&project_root) {
            rel.to_string_lossy().to_string()
        } else {
            p.to_string_lossy().to_string()
        }
    };
    let expanded_ext = args.expanded_ext.as_deref();

    let mut source_contents: HashMap<String, String> = HashMap::new();
    for full_path in &drivers {
        let content = std::fs::read_to_string(full_path).map_err(|e| e.to_string())?;
        let src_key = if args.no_macros {
            normalize_path(full_path)
        } else {
            expanded_source_key(full_path, &project_root, expanded_ext)
        };

        let tangle_cfg = weaveback_tangle::db::TangleConfig {
            sigil: args.sigil,
            open_delim: args.open_delim.clone(),
            close_delim: args.close_delim.clone(),
            chunk_end: args.chunk_end.clone(),
            comment_markers: comment_markers.clone(),
        };
        clip.db().set_source_config(&src_key, &tangle_cfg).map_err(|e| e.to_string())?;

        if args.no_macros {
            source_contents.insert(src_key.clone(), content.clone());
            clip.read(&content, &src_key);
        } else {
            let expanded = process_string(&content, Some(full_path), &mut evaluator).map_err(|e| e.to_string())?;
            let expanded_str = String::from_utf8_lossy(&expanded);
            if args.dump_expanded {
                eprintln!("=== expanded: {} ===", src_key);
                eprintln!("{}", expanded_str);
                eprintln!("=== end: {} ===", src_key);
            }
            if args.macro_only || args.expanded_ext.is_some() {
                let base_dir = args.directory.as_deref().unwrap_or(&args.input_dir);
                write_expanded_document(
                    full_path,
                    base_dir,
                    &args.expanded_adoc_dir,
                    &args.expanded_md_dir,
                    expanded_ext,
                    expanded.as_slice(),
                )?;
            }
            if args.macro_only {
                continue;
            }
            source_contents.insert(src_key.clone(), expanded_str.to_string());
            clip.read(&expanded_str, &src_key);

            let src_files = evaluator.sources().source_files().to_vec();
            let var_defs = evaluator.drain_var_defs();
            let macro_defs = evaluator.drain_macro_defs();
            for vd in var_defs {
                if let Some(path) = src_files.get(vd.src as usize) {
                    let k = normalize_path(path);
                    clip.db().record_var_def(&vd.var_name, &k, vd.pos, vd.length).map_err(|e| e.to_string())?;
                }
            }
            for md in macro_defs {
                if let Some(path) = src_files.get(md.src as usize) {
                    let k = normalize_path(path);
                    clip.db().record_macro_def(&md.macro_name, &k, md.pos, md.length).map_err(|e| e.to_string())?;
                }
            }
        }
    }

    if args.dry_run {
        for path in clip.list_output_files() {
            println!("{}", path.display());
        }
        return Ok(());
    }

    if args.macro_only {
        return Ok(());
    }

    let skip_set = compute_skip_set(&source_contents, &prev_db, clip.db_mut(), &args.gen_dir);
    clip.write_files_incremental(&skip_set).map_err(|e| e.to_string())?;

    {
        let paths: Vec<PathBuf> = if args.no_macros {
            drivers.clone()
        } else {
            evaluator.source_files().to_vec()
        };
        for path in &paths {
            if let Ok(content) = std::fs::read(path) {
                let key = normalize_path(path);
                clip.db().set_src_snapshot(&key, &content).map_err(|e| e.to_string())?;
            }
        }
    }

    clip.finish(&args.db).map_err(|e| e.to_string())?;

    // Re-open for final configs and FTS rebuild
    if let Ok(mut db) = weaveback_tangle::db::WeavebackDb::open(&args.db) {
        let _ = db.set_run_config("gen_dir", &args.gen_dir.to_string_lossy());
        if !args.no_fts && let Err(e) = db.rebuild_prose_fts(Some(&project_root)) {
            eprintln!("warning: FTS index rebuild failed: {e}");
        }
    }

    if let Some(ref depfile_path) = args.depfile {
        let deps: Vec<PathBuf> = if args.directory.is_some() {
            all_adoc
        } else if args.no_macros {
            drivers
        } else {
            evaluator.source_files().to_vec()
        };
        let stamp_path = args.stamp.clone().unwrap_or_else(|| depfile_path.clone());
        write_depfile(depfile_path, &stamp_path, &deps).map_err(|e| e.to_string())?;
    }

    if let Some(ref stamp_path) = args.stamp {
        std::fs::write(stamp_path, b"").map_err(|e| e.to_string())?;
    }

    Ok(())
}
#[cfg(test)]
mod tests;

