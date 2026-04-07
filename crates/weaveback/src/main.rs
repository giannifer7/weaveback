use weaveback_agent_core::{Workspace as AgentWorkspace, WorkspaceConfig as AgentWorkspaceConfig};
use weaveback_macro::{
    evaluator::{EvalConfig, EvalError, Evaluator},
    macro_api::process_string,
};
use weaveback_tangle::{WeavebackError, Clip, SafeFileWriter, SafeWriterConfig};
use weaveback_core::PathResolver;
use clap::Parser;
use rayon::prelude::*;
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;

mod apply_back;
mod cli_generated;
mod lint;
mod lookup;
mod mcp;
mod semantic;
mod serve;
mod tag;

use cli_generated::{Args, Cli, Commands, LspCommands};

fn default_pathsep() -> String {
    if cfg!(windows) {
        ";".to_string()
    } else {
        ":".to_string()
    }
}

// CLI declarations live in cli_generated.rs, emitted from cli-spec.adoc.

use thiserror::Error;

#[derive(Debug, Error)]
enum Error {
    #[error("{0}")]
    Macro(#[from] EvalError),
    #[error("{0}")]
    Noweb(#[from] WeavebackError),
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

impl From<weaveback_tangle::db::DbError> for Error {
    fn from(e: weaveback_tangle::db::DbError) -> Self {
        Error::Noweb(WeavebackError::Db(e))
    }
}

/// Recursively collect all files whose extension matches any entry in `exts` under `dir`.
fn find_files(dir: &Path, exts: &[String], out: &mut Vec<PathBuf>) -> std::io::Result<()> {
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

/// Escape a path for use in a Makefile depfile (spaces → `\ `).
fn depfile_escape(p: &Path) -> String {
    p.to_string_lossy().replace(' ', "\\ ")
}

/// Write a Makefile depfile.  `target` is the stamp; `deps` are all inputs.
fn write_depfile(path: &Path, target: &Path, deps: &[PathBuf]) -> std::io::Result<()> {
    use std::fmt::Write as FmtWrite;
    let mut out = String::new();
    write!(out, "{}:", depfile_escape(target)).unwrap();
    for dep in deps {
        write!(out, " {}", depfile_escape(dep)).unwrap();
    }
    out.push('\n');
    std::fs::write(path, out)
}

/// Compute the set of `@file …` chunk names that can be skipped this run.
///
/// A chunk is skippable when every source block that overlaps its definition
/// line range has an unchanged BLAKE3 hash compared to the previous run's
/// database.  We only skip chunks that already have a `gen_baseline` in the
/// previous db (i.e., have been written at least once before) — that way a
/// first-ever run always writes everything.
///
/// The algorithm:
/// 1. For each driver file, parse its source into blocks and compare hashes
///    against the previous run's `source_blocks` table.  Collect (file, block)
///    pairs whose hash changed.
/// 2. Store the new block hashes in the current-run db (always, so the next run
///    has an up-to-date baseline).
/// 3. For each changed block, find all chunk defs overlapping its line range
///    (from the previous run's `chunk_defs` table), and mark those chunks dirty.
/// 4. BFS backward through `chunk_deps` (reverse deps) to transitively mark
///    any `@file` chunk that depends on a dirty chunk as dirty too.
/// 5. Return the complement: all `@file` chunks not in the dirty set that also
///    have a gen_baseline *and whose output file exists on disk*.  The existence
///    check prevents a deleted (or never-written) output file from being silently
///    skipped just because the database has a stale baseline for it.
fn compute_skip_set(
    source_contents: &HashMap<String, String>,
    prev_db: &Option<weaveback_tangle::db::WeavebackDb>,
    current_db: &mut weaveback_tangle::db::WeavebackDb,
    gen_dir: &std::path::Path,
) -> HashSet<String> {
    use weaveback_tangle::parse_source_blocks;

    // Step 1: parse all source blocks in parallel (pure, BLAKE3-heavy).
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

    // Step 2: store new hashes (sequential — requires &mut db) and collect
    // dirty chunk names by comparing against the previous run's hashes.
    let mut dirty_chunks: HashSet<String> = HashSet::new();

    for (path, new_blocks) in &parsed {
        // Store new block hashes into the current-run db (always).
        if let Err(e) = current_db.set_source_blocks(path, new_blocks) {
            eprintln!("warning: set_source_blocks failed for {path}: {e}");
            // If we can't record blocks, treat the whole file as dirty.
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
                .unwrap_or(true); // new block (no prior hash) ⇒ changed

            if changed
                && let Some(db) = prev
                && let Ok(chunk_defs) = db.query_chunk_defs_overlapping(path, blk.line_start, blk.line_end) {
                    for def in chunk_defs {
                        dirty_chunks.insert(def.chunk_name.clone());
                    }
            }
        }
    }

    // Step 4: BFS backward through chunk_deps to find transitively dirty chunks.
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

    // If any entry is "*" (failed to record blocks for some file), skip nothing.
    if dirty_chunks.contains("*") {
        return HashSet::new();
    }

    // Step 5: collect skippable @file chunks.
    let Some(prev) = prev_db.as_ref() else {
        return HashSet::new();
    };

    let all_file_chunks: Vec<String> = prev
        .list_chunk_defs(None)
        .unwrap_or_default()
        .into_iter()
        .filter(|e| e.chunk_name.starts_with("@file "))
        .map(|e| e.chunk_name)
        .collect::<std::collections::BTreeSet<_>>() // deduplicate
        .into_iter()
        .collect();

    let mut skip: HashSet<String> = HashSet::new();
    for name in all_file_chunks {
        if dirty_chunks.contains(&name) {
            continue;
        }
        // Only skip if the file was written before (has a baseline) AND still
        // exists on disk.  Without the existence check, a deleted output file
        // would never be regenerated because the stale db baseline makes it
        // look up-to-date.
        let out_file = name.strip_prefix("@file ").unwrap_or(&name).trim();
        if prev.get_baseline(out_file).ok().flatten().is_some()
            && gen_dir.join(out_file).exists()
        {
            skip.insert(name);
        }
    }
    skip
}

fn run(args: Args) -> Result<(), Error> {
    if args.inputs.is_empty() && args.directory.is_none() {
        use clap::CommandFactory;
        Cli::command().print_help().unwrap();
        println!();
        std::process::exit(0);
    }

    let pathsep = default_pathsep();
    let include_paths: Vec<PathBuf> = args.include.split(&pathsep).map(PathBuf::from).collect();

    let eval_config = EvalConfig {
        sigil: args.sigil,
        include_paths: include_paths.clone(),
        discovery_mode: false,
        allow_env: args.allow_env,
    };
    let mut evaluator = Evaluator::new(eval_config.clone());

    let comment_markers: Vec<String> = args
        .comment_markers
        .split(',')
        .map(|s| s.trim().to_string())
        .collect();

    let formatters: HashMap<String, String> = args
        .formatter
        .iter()
        .filter_map(|s| {
            s.split_once('=')
                .map(|(e, c)| (e.to_string(), c.to_string()))
        })
        .collect();

    let safe_writer = SafeFileWriter::with_config(
        &args.gen_dir,
        SafeWriterConfig {
            formatters,
            allow_home: args.allow_home,
            force_generated: args.force_generated,
            ..SafeWriterConfig::default()
        },
    )
    .map_err(|e| Error::Noweb(e.into()))?;
    let mut clip = Clip::new(
        safe_writer,
        &args.open_delim,
        &args.close_delim,
        &args.chunk_end,
        &comment_markers,
    );
    clip.set_strict_undefined(args.strict);
    clip.set_warn_unused(args.warn_unused);

    // Determine the set of driver files to process and all source files for the depfile.
    let (drivers, all_adoc): (Vec<PathBuf>, Vec<PathBuf>) = if let Some(ref dir) = args.directory {
        let mut all = Vec::new();
        find_files(dir, &args.ext, &mut all).map_err(Error::Io)?;
        all.sort();

        // Discovery pass: evaluate each file with discovery_mode=true so that
        // %include/%import resolve their path arguments fully (handling %if,
        // computed paths, etc.) but do not recurse into the included file.
        let discovery_config = EvalConfig {
            discovery_mode: true,
            ..eval_config.clone()
        };
        let mut included: HashSet<PathBuf> = HashSet::new();
        for adoc in &all {
            if let Ok(text) = std::fs::read_to_string(adoc) {
                let mut disc = Evaluator::new(discovery_config.clone());
                if process_string(&text, Some(adoc), &mut disc).is_ok() {
                    for p in disc.take_discovered_includes() {
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
        let drivers = args
            .inputs
            .iter()
            .map(|p| args.input_dir.join(p))
            .collect::<Vec<_>>();
        (drivers.clone(), drivers)
    };

    // Open the previous run's db (read-only) so we can compare block hashes.
    let prev_db = if args.db.exists() {
        weaveback_tangle::db::WeavebackDb::open_read_only(&args.db).ok()
    } else {
        None
    };

    // Phase 1: process each driver and feed result to noweb.
    // Collect original (pre-expansion) source content for block-hash comparison.
    let mut source_contents: HashMap<String, String> = HashMap::new();
    for full_path in &drivers {
        let content = std::fs::read_to_string(full_path)?;
        source_contents.insert(full_path.to_string_lossy().into_owned(), content.clone());

        // Record the configuration used for this source file.
        let tangle_cfg = weaveback_tangle::db::TangleConfig {
            sigil: args.sigil,
            open_delim: args.open_delim.clone(),
            close_delim: args.close_delim.clone(),
            chunk_end: args.chunk_end.clone(),
            comment_markers: comment_markers.clone(),
        };
        clip.db().set_source_config(&full_path.to_string_lossy(), &tangle_cfg)?;

        if args.no_macros {
            // Skip macro expansion: feed the raw file directly to the tangle pass.
            clip.read(&content, &full_path.to_string_lossy());
        } else {
            let expanded = weaveback_macro::macro_api::process_string(
                &content,
                Some(full_path),
                &mut evaluator,
            )?;
            let expanded_str = String::from_utf8_lossy(&expanded);
            if args.dump_expanded {
                eprintln!("=== expanded: {} ===", full_path.display());
                eprintln!("{}", expanded_str);
                eprintln!("=== end: {} ===", full_path.display());
            }
            clip.read(&expanded_str, &full_path.to_string_lossy());

            // Record %set and %def positions into the db.
            let src_files = evaluator.sources().source_files().to_vec();
            let var_defs = evaluator.drain_var_defs();
            let macro_defs = evaluator.drain_macro_defs();
            (|| -> Result<(), weaveback_tangle::WeavebackError> {
                for vd in var_defs {
                    if let Some(path) = src_files.get(vd.src as usize) {
                        clip.db().record_var_def(&vd.var_name, &path.to_string_lossy(), vd.pos, vd.length)?;
                    }
                }
                for md in macro_defs {
                    if let Some(path) = src_files.get(md.src as usize) {
                        clip.db().record_macro_def(&md.macro_name, &path.to_string_lossy(), md.pos, md.length)?;
                    }
                }
                Ok(())
            })()?;
        }
    }

    // Phase 2: write all @file chunks (or just list them if --dry-run).
    if args.dry_run {
        for path in clip.list_output_files() {
            println!("{}", path.display());
        }
        return Ok(());
    }

    // Compute which @file chunks can be skipped because none of their source
    // blocks changed since the last run.  Store the new block hashes first so
    // the next run can compare against this run's content.
    let skip_set = compute_skip_set(&source_contents, &prev_db, clip.db_mut(), &args.gen_dir);
    clip.write_files_incremental(&skip_set)?;

    // Phase 3: snapshot all source files read this run.
    (|| -> Result<(), weaveback_tangle::WeavebackError> {
        let paths: Vec<PathBuf> = if args.no_macros {
            drivers.clone()
        } else {
            evaluator.source_files().to_vec()
        };
        for path in &paths {
            if let Ok(content) = std::fs::read(path) {
                let key = path.to_string_lossy();
                clip.db().set_src_snapshot(key.as_ref(), &content)?;
            }
        }
        Ok(())
    })()?;

    // Phase 4: merge temp db into the db file.
    clip.finish(&args.db)?;

    // Persist gen_dir and (unless suppressed) rebuild FTS on the final merged db.
    if let Ok(mut db) = weaveback_tangle::db::WeavebackDb::open(&args.db) {
        let _ = db.set_run_config("gen_dir", &args.gen_dir.to_string_lossy());
        if !args.no_fts && let Err(e) = db.rebuild_prose_fts() {
            eprintln!("warning: FTS index rebuild failed: {e}");
        }
    }

    // Write depfile if requested.
    if let Some(ref depfile_path) = args.depfile {
        let deps: Vec<PathBuf> = if args.directory.is_some() {
            all_adoc
        } else if args.no_macros {
            drivers
        } else {
            evaluator.source_files().to_vec()
        };
        let stamp_path = args.stamp.clone().unwrap_or_else(|| depfile_path.clone());
        write_depfile(depfile_path, &stamp_path, &deps).map_err(Error::Io)?;
    }

    // Touch stamp file if requested.
    if let Some(ref stamp_path) = args.stamp {
        std::fs::write(stamp_path, b"").map_err(Error::Io)?;
    }

    Ok(())
}

fn build_eval_config(args: &Args) -> weaveback_macro::evaluator::EvalConfig {
    let pathsep = default_pathsep();
    let include_paths: Vec<std::path::PathBuf> = args.include.split(&pathsep).map(std::path::PathBuf::from).collect();
    weaveback_macro::evaluator::EvalConfig {
        sigil: args.sigil,
        include_paths,
        discovery_mode: false,
        allow_env: args.allow_env,
    }
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Some(Commands::Trace { out_file, line, col }) => {
            let eval_config = build_eval_config(&cli.args);
            run_trace(out_file, line, col, cli.args.db, cli.args.gen_dir, eval_config)
        }
        Some(Commands::Attribute {
            scan_stdin,
            summary,
            locations,
        }) => {
            let eval_config = build_eval_config(&cli.args);
            run_attribute(
                scan_stdin,
                summary,
                locations,
                cli.args.db,
                cli.args.gen_dir,
                eval_config,
            )
        }
        Some(Commands::Cargo {
            diagnostics_only,
            args,
        }) => {
            let eval_config = build_eval_config(&cli.args);
            run_cargo_annotated(
                args,
                diagnostics_only,
                cli.args.db,
                cli.args.gen_dir,
                eval_config,
            )
        }
        Some(Commands::Where { out_file, line }) => {
            run_where(out_file, line, cli.args.db, cli.args.gen_dir)
        }
        Some(Commands::Tangle { config, force_generated }) => {
            run_tangle_all(&config, force_generated)
        }
                Some(Commands::Tag { config, backend, model, endpoint, batch_size }) => {
            run_tag_only(&config, backend, model, endpoint, batch_size, cli.args.db)
        }
        Some(Commands::Mcp) => {
            let eval_config = build_eval_config(&cli.args);
            mcp::run_mcp(cli.args.db, cli.args.gen_dir, eval_config)
        }
        Some(Commands::ApplyBack { files, dry_run }) => {
            let eval_config = build_eval_config(&cli.args);
            let opts = apply_back::ApplyBackOptions {
                db_path: cli.args.db,
                gen_dir: cli.args.gen_dir,
                dry_run,
                files,
                eval_config: Some(eval_config),
            };
            apply_back::run_apply_back(opts, &mut std::io::stdout()).map_err(|e| Error::Io(std::io::Error::other(e.to_string())))
        }
        Some(Commands::Impact { chunk }) => {
            run_impact(chunk, cli.args.db)
        }
        Some(Commands::Graph { chunk }) => {
            run_graph(chunk, cli.args.db)
        }
        Some(Commands::Search { query, limit }) => {
            run_search(query, limit, cli.args.db)
        }
        Some(Commands::Lint { paths, strict, rule, json }) => {
            lint::run_lint(paths, strict, rule, json)
                .map_err(|e| Error::Io(std::io::Error::other(e)))
        }
        Some(Commands::Coverage {
            summary,
            top_sources,
            top_sections,
            lcov_file,
        }) => {
            run_coverage(
                summary,
                top_sources,
                top_sections,
                lcov_file,
                cli.args.db,
                cli.args.gen_dir,
            )
        }
        Some(Commands::Tags { file }) => {
            run_tags(file, cli.args.db)
        }
        Some(Commands::Lsp { lsp_cmd, lsp_lang, cmd }) => {
            let eval_config = build_eval_config(&cli.args);
            run_lsp(cmd, cli.args.db, cli.args.gen_dir, eval_config, lsp_cmd, lsp_lang)
        }
                Some(Commands::Serve { port, html, open_delim, close_delim, chunk_end, comment_markers, ai_backend, ai_model, ai_endpoint, watch }) => {
            let backend = match ai_backend.as_str() {
                "anthropic" => serve::AiBackend::Anthropic,
                "gemini"    => serve::AiBackend::Gemini,
                "ollama"    => serve::AiBackend::Ollama,
                "openai"    => serve::AiBackend::OpenAi,
                _           => serve::AiBackend::ClaudeCli,
            };
            let tangle_cfg = serve::TangleConfig {
                open_delim,
                close_delim,
                chunk_end,
                comment_markers: comment_markers.split(',').map(|s| s.trim().to_string()).collect(),
                ai_backend: backend,
                ai_model,
                ai_endpoint,
            };
            serve::run_serve(port, html, tangle_cfg, watch)
                .map_err(|e| Error::Io(std::io::Error::other(e)))
        }
        None => run(cli.args),
    };

    if let Err(e) = result {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}

fn open_db(db_path: &Path) -> Result<weaveback_tangle::db::WeavebackDb, Error> {
    if !db_path.exists() {
        return Err(Error::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("Database not found at {}. Run weaveback on your source files first.", db_path.display()),
        )));
    }
    Ok(weaveback_tangle::db::WeavebackDb::open_read_only(db_path)?)
}

fn parse_generated_location(spec: &str) -> Result<(String, u32, u32), Error> {
    let mut parts = spec.rsplitn(3, ':');
    let last = parts.next().ok_or_else(|| {
        Error::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "location must be FILE:LINE or FILE:LINE:COL",
        ))
    })?;
    let middle = parts.next().ok_or_else(|| {
        Error::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "location must be FILE:LINE or FILE:LINE:COL",
        ))
    })?;

    if let Some(file) = parts.next() {
        let line = middle.parse::<u32>().map_err(|e| {
            Error::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("invalid line in location `{spec}`: {e}"),
            ))
        })?;
        let col = last.parse::<u32>().map_err(|e| {
            Error::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("invalid column in location `{spec}`: {e}"),
            ))
        })?;
        Ok((file.to_string(), line, col))
    } else {
        let line = last.parse::<u32>().map_err(|e| {
            Error::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("invalid line in location `{spec}`: {e}"),
            ))
        })?;
        Ok((middle.to_string(), line, 1))
    }
}

fn scan_generated_locations(text: &str) -> Vec<String> {
    fn normalize_scanned_location(token: &str) -> Option<String> {
        let trimmed = token
            .trim_matches(|c: char| matches!(c, '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>' | '"' | '\'' | ',' | ';'))
            .trim_end_matches(['.', ':', '!', '?']);
        if trimmed.is_empty() {
            return None;
        }
        Some(trimmed.to_string())
    }

    let pattern = regex::Regex::new(r"(?P<loc>(?:[A-Za-z]:)?[^\s]+:\d+(?::\d+)?)")
        .expect("valid location regex");
    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for captures in pattern.captures_iter(text) {
        let Some(loc) = captures
            .name("loc")
            .and_then(|m| normalize_scanned_location(m.as_str()))
        else {
            continue;
        };
        if parse_generated_location(&loc).is_ok() && seen.insert(loc.clone()) {
            out.push(loc);
        }
    }
    out
}

fn run_where(out_file: String, line: u32, db_path: PathBuf, gen_dir: PathBuf) -> Result<(), Error> {
    let db = open_db(&db_path)?;
    let project_root = std::env::current_dir().unwrap_or_default();
    let resolver = PathResolver::new(project_root, gen_dir);

    match lookup::perform_where(&out_file, line, &db, &resolver) {
        Ok(Some(json)) => {
            println!("{}", serde_json::to_string_pretty(&json).unwrap());
            Ok(())
        }
        Ok(None) => {
            eprintln!("No mapping found for {}:{}", out_file, line);
            Ok(())
        }
        Err(lookup::LookupError::InvalidInput(msg)) => {
            Err(Error::Io(std::io::Error::new(std::io::ErrorKind::InvalidInput, msg)))
        }
        Err(lookup::LookupError::Db(e)) => Err(Error::Noweb(WeavebackError::Db(e))),
        Err(lookup::LookupError::Io(e)) => Err(Error::Io(e)),
    }
}

fn run_attribute(
    scan_stdin: bool,
    summary: bool,
    mut locations: Vec<String>,
    db_path: PathBuf,
    gen_dir: PathBuf,
    eval_config: weaveback_macro::evaluator::EvalConfig,
) -> Result<(), Error> {
    if scan_stdin {
        let mut input = String::new();
        std::io::stdin()
            .read_to_string(&mut input)
            .map_err(Error::Io)?;
        locations.extend(scan_generated_locations(&input));
        locations.sort();
        locations.dedup();
    }
    if locations.is_empty() {
        return Err(Error::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "at least one location is required (or use --scan-stdin)",
        )));
    }
    if !summary && !scan_stdin && locations.len() == 1 {
        let (out_file, line, col) = parse_generated_location(&locations[0])?;
        return run_trace(out_file, line, col, db_path, gen_dir, eval_config);
    }

    let db = open_db(&db_path)?;
    let project_root = std::env::current_dir().unwrap_or_default();
    let resolver = PathResolver::new(project_root, gen_dir);
    let mut results = Vec::new();

    for location in locations {
        let (out_file, line, col) = parse_generated_location(&location)?;
        match lookup::perform_trace(&out_file, line, col, &db, &resolver, eval_config.clone()) {
            Ok(Some(json)) => results.push(json!({
                "location": location,
                "ok": true,
                "trace": json,
            })),
            Ok(None) => results.push(json!({
                "location": location,
                "ok": false,
                "trace": serde_json::Value::Null,
            })),
            Err(lookup::LookupError::InvalidInput(msg)) => {
                return Err(Error::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    msg,
                )));
            }
            Err(lookup::LookupError::Db(e)) => return Err(Error::Noweb(WeavebackError::Db(e))),
            Err(lookup::LookupError::Io(e)) => return Err(Error::Io(e)),
        }
    }

    if summary {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "count": results.len(),
                "ok_count": results.iter().filter(|r| r["ok"].as_bool() == Some(true)).count(),
                "miss_count": results.iter().filter(|r| r["ok"].as_bool() != Some(true)).count(),
                "results": results,
                "weaveback_source_summary": build_location_attribution_summary(&results),
            }))
            .unwrap()
        );
    } else {
        println!("{}", serde_json::to_string_pretty(&results).unwrap());
    }
    Ok(())
}

fn parse_lcov_records(text: &str) -> Vec<(String, u32, u64)> {
    let mut current_file: Option<String> = None;
    let mut out = Vec::new();

    for line in text.lines() {
        if let Some(path) = line.strip_prefix("SF:") {
            current_file = Some(path.to_string());
            continue;
        }
        if line == "end_of_record" {
            current_file = None;
            continue;
        }
        let Some(rest) = line.strip_prefix("DA:") else {
            continue;
        };
        let Some(file) = current_file.as_ref() else {
            continue;
        };
        let mut parts = rest.split(',');
        let Some(line_no) = parts.next().and_then(|s| s.parse::<u32>().ok()) else {
            continue;
        };
        let Some(hit_count) = parts.next().and_then(|s| s.parse::<u64>().ok()) else {
            continue;
        };
        out.push((file.clone(), line_no, hit_count));
    }

    out
}

fn build_coverage_summary(
    records: &[(String, u32, u64)],
    db: &weaveback_tangle::db::WeavebackDb,
    project_root: &Path,
    resolver: &PathResolver,
) -> serde_json::Value {
    #[derive(Default)]
    struct SectionSummary {
        total_lines: usize,
        covered_lines: usize,
        missed_lines: usize,
        chunks: std::collections::BTreeSet<String>,
        generated_lines: Vec<serde_json::Value>,
        prose: Option<String>,
        range: Option<serde_json::Value>,
        breadcrumb: Vec<String>,
    }

    #[derive(Default)]
    struct SourceSummary {
        total_lines: usize,
        covered_lines: usize,
        missed_lines: usize,
        chunks: std::collections::BTreeSet<String>,
        sections: std::collections::BTreeMap<String, SectionSummary>,
    }

    #[derive(Default)]
    struct UnattributedSummary {
        total_lines: usize,
        covered_lines: usize,
        missed_lines: usize,
        has_noweb_entries: bool,
        mapped_line_start: Option<u32>,
        mapped_line_end: Option<u32>,
        generated_lines: Vec<serde_json::Value>,
    }

    let mut grouped: std::collections::BTreeMap<String, SourceSummary> =
        std::collections::BTreeMap::new();
    let mut unattributed_grouped: std::collections::BTreeMap<String, UnattributedSummary> =
        std::collections::BTreeMap::new();
    let mut unattributed = Vec::new();
    let mut attributed_count = 0usize;
    let mut noweb_cache: std::collections::HashMap<
        String,
        std::collections::HashMap<u32, weaveback_tangle::db::NowebMapEntry>,
    > = std::collections::HashMap::new();
    let mut source_cache: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    let mut section_cache: std::collections::HashMap<String, Vec<(u32, u32, serde_json::Value)>> =
        std::collections::HashMap::new();

    for (file_name, line_no, hit_count) in records {
        let noweb_map = if let Some(entries) = noweb_cache.get(file_name) {
            entries
        } else {
            let loaded = find_noweb_entries_for_generated_file(db, file_name, project_root)
                .unwrap_or_default()
                .into_iter()
                .collect::<std::collections::HashMap<_, _>>();
            noweb_cache.entry(file_name.clone()).or_insert(loaded)
        };

        let Some(entry) = line_no
            .checked_sub(1)
            .and_then(|line_0| noweb_map.get(&line_0))
        else {
            let covered = *hit_count > 0;
            let mapped_line_start = noweb_map.keys().min().copied().map(|line_0| line_0 + 1);
            let mapped_line_end = noweb_map.keys().max().copied().map(|line_0| line_0 + 1);
            let generated_line = json!({
                "generated_file": file_name,
                "generated_line": line_no,
                "hit_count": hit_count,
                "covered": covered,
                "has_noweb_entries": !noweb_map.is_empty(),
                "mapped_line_start": mapped_line_start,
                "mapped_line_end": mapped_line_end,
            });
            unattributed.push(generated_line.clone());
            let file = unattributed_grouped.entry(file_name.clone()).or_default();
            file.total_lines += 1;
            if covered {
                file.covered_lines += 1;
            } else {
                file.missed_lines += 1;
            }
            file.has_noweb_entries |= !noweb_map.is_empty();
            file.mapped_line_start = match (file.mapped_line_start, mapped_line_start) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (None, b) => b,
                (a, None) => a,
            };
            file.mapped_line_end = match (file.mapped_line_end, mapped_line_end) {
                (Some(a), Some(b)) => Some(a.max(b)),
                (None, b) => b,
                (a, None) => a,
            };
            file.generated_lines.push(generated_line);
            continue;
        };

        let src_file = entry.src_file.clone();
        let src_line = (entry.src_line + 1) as u64;

        let context = if let Some(cached) = section_cache
            .get(&src_file)
            .and_then(|sections| {
                sections.iter().find_map(|(start, end, value)| {
                    if src_line >= *start as u64 && src_line <= *end as u64 {
                        Some(value.clone())
                    } else {
                        None
                    }
                })
            }) {
            cached
        } else {
            let src_content = if let Some(text) = source_cache.get(&src_file) {
                text.clone()
            } else {
                let Ok(text) = lookup::load_source_text(&src_file, db, resolver) else {
                    let covered = *hit_count > 0;
                    let mapped_line_start = noweb_map.keys().min().copied().map(|line_0| line_0 + 1);
                    let mapped_line_end = noweb_map.keys().max().copied().map(|line_0| line_0 + 1);
                    let generated_line = json!({
                        "generated_file": file_name,
                        "generated_line": line_no,
                        "hit_count": hit_count,
                        "covered": covered,
                        "has_noweb_entries": !noweb_map.is_empty(),
                        "mapped_line_start": mapped_line_start,
                        "mapped_line_end": mapped_line_end,
                    });
                    unattributed.push(generated_line.clone());
                    let file = unattributed_grouped.entry(file_name.clone()).or_default();
                    file.total_lines += 1;
                    if covered {
                        file.covered_lines += 1;
                    } else {
                        file.missed_lines += 1;
                    }
                    file.has_noweb_entries |= !noweb_map.is_empty();
                    file.mapped_line_start = match (file.mapped_line_start, mapped_line_start) {
                        (Some(a), Some(b)) => Some(a.min(b)),
                        (None, b) => b,
                        (a, None) => a,
                    };
                    file.mapped_line_end = match (file.mapped_line_end, mapped_line_end) {
                        (Some(a), Some(b)) => Some(a.max(b)),
                        (None, b) => b,
                        (a, None) => a,
                    };
                    file.generated_lines.push(generated_line);
                    continue;
                };
                source_cache.insert(src_file.clone(), text.clone());
                text
            };
            let value = lookup::build_source_context_value(&src_content, src_line as usize);
            let start = value
                .get("source_section_range")
                .and_then(|v| v.get("start_line"))
                .and_then(|v| v.as_u64())
                .unwrap_or(src_line) as u32;
            let end = value
                .get("source_section_range")
                .and_then(|v| v.get("end_line"))
                .and_then(|v| v.as_u64())
                .unwrap_or(src_line) as u32;
            section_cache
                .entry(src_file.clone())
                .or_default()
                .push((start, end, value.clone()));
            value
        };

        attributed_count += 1;
        let mut trace = json!({
            "generated_file": file_name,
            "generated_line": line_no,
            "chunk": entry.chunk_name,
            "expanded_file": src_file,
            "expanded_line": src_line,
            "indent": entry.indent,
            "confidence": entry.confidence.as_str(),
        });
        if let (Some(trace_obj), Some(ctx_obj)) = (trace.as_object_mut(), context.as_object()) {
            trace_obj.extend(ctx_obj.clone());
        }

        let breadcrumb = trace
            .get("source_section_breadcrumb")
            .and_then(|v| v.as_array())
            .map(|parts| {
                parts
                    .iter()
                    .filter_map(|part| part.as_str().map(ToOwned::to_owned))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let section_key = if breadcrumb.is_empty() {
            "<unknown>".to_string()
        } else {
            breadcrumb.join(" / ")
        };
        let covered = *hit_count > 0;
        let chunk = trace
            .get("chunk")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let generated_line = json!({
            "generated_file": file_name,
            "generated_line": line_no,
            "hit_count": hit_count,
            "covered": covered,
            "chunk": if chunk.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(chunk.clone()) },
        });

        let source = grouped.entry(src_file).or_default();
        source.total_lines += 1;
        if covered {
            source.covered_lines += 1;
        } else {
            source.missed_lines += 1;
        }
        if !chunk.is_empty() {
            source.chunks.insert(chunk.clone());
        }

        let section = source.sections.entry(section_key).or_default();
        section.total_lines += 1;
        if covered {
            section.covered_lines += 1;
        } else {
            section.missed_lines += 1;
        }
        if !chunk.is_empty() {
            section.chunks.insert(chunk);
        }
        section.generated_lines.push(generated_line);
        if section.prose.is_none() {
            section.prose = trace
                .get("source_section_prose")
                .and_then(|v| v.as_str())
                .map(ToOwned::to_owned);
        }
        if section.range.is_none() {
            section.range = trace.get("source_section_range").cloned();
        }
        if section.breadcrumb.is_empty() {
            section.breadcrumb = breadcrumb;
        }
    }

    let mut sources = grouped
        .into_iter()
        .map(|(src_file, source)| {
            let mut sections = source
                .sections
                .into_values()
                .map(|section| {
                    json!({
                        "source_section_breadcrumb": section.breadcrumb,
                        "source_section_range": section.range.unwrap_or(serde_json::Value::Null),
                        "source_section_prose": section.prose.unwrap_or_default(),
                        "total_lines": section.total_lines,
                        "covered_lines": section.covered_lines,
                        "missed_lines": section.missed_lines,
                        "chunks": section.chunks.into_iter().collect::<Vec<_>>(),
                        "generated_lines": section.generated_lines,
                    })
                })
                .collect::<Vec<_>>();
            sections.sort_by(|a, b| {
                let am = a["missed_lines"].as_u64().unwrap_or(0);
                let bm = b["missed_lines"].as_u64().unwrap_or(0);
                bm.cmp(&am).then_with(|| {
                    let an = a["source_section_breadcrumb"]
                        .as_array()
                        .map(|parts| {
                            parts
                                .iter()
                                .filter_map(|part| part.as_str())
                                .collect::<Vec<_>>()
                                .join(" / ")
                        })
                        .unwrap_or_default();
                    let bn = b["source_section_breadcrumb"]
                        .as_array()
                        .map(|parts| {
                            parts
                                .iter()
                                .filter_map(|part| part.as_str())
                                .collect::<Vec<_>>()
                                .join(" / ")
                        })
                        .unwrap_or_default();
                    an.cmp(&bn)
                })
            });

            json!({
                "src_file": src_file,
                "total_lines": source.total_lines,
                "covered_lines": source.covered_lines,
                "missed_lines": source.missed_lines,
                "chunks": source.chunks.into_iter().collect::<Vec<_>>(),
                "sections": sections,
            })
        })
        .collect::<Vec<_>>();
    sources.sort_by(|a, b| {
        let am = a["missed_lines"].as_u64().unwrap_or(0);
        let bm = b["missed_lines"].as_u64().unwrap_or(0);
        bm.cmp(&am).then_with(|| {
            let af = a["src_file"].as_str().unwrap_or_default();
            let bf = b["src_file"].as_str().unwrap_or_default();
            af.cmp(bf)
        })
    });

    let mut unattributed_files = unattributed_grouped
        .into_iter()
        .map(|(generated_file, summary)| {
            json!({
                "generated_file": generated_file,
                "total_lines": summary.total_lines,
                "covered_lines": summary.covered_lines,
                "missed_lines": summary.missed_lines,
                "has_noweb_entries": summary.has_noweb_entries,
                "mapped_line_start": summary.mapped_line_start,
                "mapped_line_end": summary.mapped_line_end,
                "generated_lines": summary.generated_lines,
            })
        })
        .collect::<Vec<_>>();
    unattributed_files.sort_by(|a, b| {
        let am = a["missed_lines"].as_u64().unwrap_or(0);
        let bm = b["missed_lines"].as_u64().unwrap_or(0);
        bm.cmp(&am).then_with(|| {
            let af = a["generated_file"].as_str().unwrap_or_default();
            let bf = b["generated_file"].as_str().unwrap_or_default();
            af.cmp(bf)
        })
    });

    json!({
        "line_records": records.len(),
        "attributed_records": attributed_count,
        "unattributed_records": unattributed.len(),
        "sources": sources,
        "unattributed": unattributed,
        "unattributed_files": unattributed_files,
    })
}

fn find_noweb_entries_for_generated_file(
    db: &weaveback_tangle::db::WeavebackDb,
    file_name: &str,
    project_root: &Path,
) -> Option<Vec<(u32, weaveback_tangle::db::NowebMapEntry)>> {
    let mut candidates = Vec::new();
    candidates.push(file_name.to_string());
    let file_path = Path::new(file_name);
    if let Ok(rel) = file_path.strip_prefix(project_root) {
        let rel = rel.to_string_lossy().replace('\\', "/");
        if !candidates.contains(&rel) {
            candidates.push(rel);
        }
    }

    for candidate in candidates {
        if let Ok(entries) = db.get_noweb_entries_for_file_by_suffix(&candidate)
            && !entries.is_empty()
        {
            return Some(entries);
        }
        let parts: Vec<&str> = candidate.split('/').filter(|part| !part.is_empty()).collect();
        for start in 1..parts.len() {
            let suffix = parts[start..].join("/");
            if !suffix.contains('/') {
                break;
            }
            if let Ok(entries) = db.get_noweb_entries_for_file_by_suffix(&suffix)
                && !entries.is_empty()
            {
                return Some(entries);
            }
        }
    }

    None
}

fn print_coverage_summary_to_writer(
    summary: &serde_json::Value,
    top_sources: usize,
    top_sections: usize,
    mut out: impl Write,
) -> std::io::Result<()> {
    writeln!(
        out,
        "Coverage by source: {} attributed / {} total line records",
        summary["attributed_records"].as_u64().unwrap_or(0),
        summary["line_records"].as_u64().unwrap_or(0)
    )?;

    if let Some(sources) = summary["sources"].as_array() {
        for source in sources.iter().take(top_sources) {
            let src_file = source["src_file"].as_str().unwrap_or("<unknown>");
            let covered = source["covered_lines"].as_u64().unwrap_or(0);
            let missed = source["missed_lines"].as_u64().unwrap_or(0);
            let total = source["total_lines"].as_u64().unwrap_or(0);
            let pct = if total == 0 {
                0.0
            } else {
                100.0 * covered as f64 / total as f64
            };
            writeln!(out, "{src_file}: {covered}/{total} covered ({pct:.1}%), {missed} missed")?;
            if let Some(sections) = source["sections"].as_array() {
                for section in sections.iter().take(top_sections) {
                    let breadcrumb = section["source_section_breadcrumb"]
                        .as_array()
                        .map(|parts| {
                            parts
                                .iter()
                                .filter_map(|part| part.as_str())
                                .collect::<Vec<_>>()
                                .join(" / ")
                        })
                        .unwrap_or_else(|| "<unknown>".to_string());
                    let covered = section["covered_lines"].as_u64().unwrap_or(0);
                    let missed = section["missed_lines"].as_u64().unwrap_or(0);
                    let total = section["total_lines"].as_u64().unwrap_or(0);
                    let pct = if total == 0 {
                        0.0
                    } else {
                        100.0 * covered as f64 / total as f64
                    };
                    writeln!(
                        out,
                        "  {breadcrumb}: {covered}/{total} covered ({pct:.1}%), {missed} missed"
                    )?;
                }
            }
        }
    }

    let unattributed = summary["unattributed_records"].as_u64().unwrap_or(0);
    if unattributed > 0 {
        writeln!(out, "Unattributed line records: {unattributed}")?;
        if let Some(files) = summary["unattributed_files"].as_array() {
            for file in files.iter().take(top_sources) {
                let generated_file = file["generated_file"].as_str().unwrap_or("<unknown>");
                let covered = file["covered_lines"].as_u64().unwrap_or(0);
                let missed = file["missed_lines"].as_u64().unwrap_or(0);
                let total = file["total_lines"].as_u64().unwrap_or(0);
                let pct = if total == 0 {
                    0.0
                } else {
                    100.0 * covered as f64 / total as f64
                };
                writeln!(
                    out,
                    "  {generated_file}: {covered}/{total} covered ({pct:.1}%), {missed} missed"
                )?;
                if file["has_noweb_entries"].as_bool().unwrap_or(false) {
                    let start = file["mapped_line_start"].as_u64().unwrap_or(0);
                    let end = file["mapped_line_end"].as_u64().unwrap_or(0);
                    writeln!(out, "    partial mapping: mapped lines {start}-{end}")?;
                } else {
                    writeln!(out, "    no noweb mapping recorded for this file")?;
                }
            }
        }
    }
    Ok(())
}

fn build_coverage_summary_view(
    summary: &serde_json::Value,
    top_sources: usize,
    top_sections: usize,
) -> serde_json::Value {
    let mut value = summary.clone();
    let top_sources_value = summary["sources"]
        .as_array()
        .map(|sources| {
            serde_json::Value::Array(
                sources
                    .iter()
                    .take(top_sources)
                    .map(|source| {
                        let mut source = source.clone();
                        if let Some(obj) = source.as_object_mut()
                            && let Some(sections) =
                                obj.get("sections").and_then(|v| v.as_array()).cloned()
                        {
                            obj.insert(
                                "sections".to_string(),
                                serde_json::Value::Array(
                                    sections.into_iter().take(top_sections).collect(),
                                ),
                            );
                        }
                        source
                    })
                    .collect(),
            )
        })
        .unwrap_or_else(|| serde_json::Value::Array(Vec::new()));

    if let Some(obj) = value.as_object_mut() {
        obj.insert(
            "summary_view".to_string(),
            json!({
                "top_sources": top_sources,
                "top_sections": top_sections,
                "sources": top_sources_value,
                "unattributed_records": summary["unattributed_records"].clone(),
                "unattributed_files": summary["unattributed_files"]
                    .as_array()
                    .map(|files| serde_json::Value::Array(files.iter().take(top_sources).cloned().collect()))
                    .unwrap_or_else(|| serde_json::Value::Array(Vec::new())),
                "line_records": summary["line_records"].clone(),
                "attributed_records": summary["attributed_records"].clone(),
            }),
        );
    }
    value
}

fn run_coverage(
    summary_only: bool,
    top_sources: usize,
    top_sections: usize,
    lcov_file: PathBuf,
    db_path: PathBuf,
    gen_dir: PathBuf,
) -> Result<(), Error> {
    let text = std::fs::read_to_string(&lcov_file).map_err(Error::Io)?;
    let records = parse_lcov_records(&text);
    let db = open_db(&db_path)?;
    let project_root = std::env::current_dir().unwrap_or_default();
    let resolver = PathResolver::new(project_root.clone(), gen_dir);
    let summary = build_coverage_summary(&records, &db, &project_root, &resolver);
    if summary_only {
        print_coverage_summary_to_writer(&summary, top_sources, top_sections, &mut std::io::stdout())
            .map_err(Error::Io)?;
    } else {
        let value = build_coverage_summary_view(&summary, top_sources, top_sections);
        println!("{}", serde_json::to_string_pretty(&value).unwrap());
    }
    Ok(())
}

#[derive(Debug, serde::Deserialize)]
struct CargoMessageEnvelope {
    reason: String,
    message: Option<CargoDiagnostic>,
}

#[derive(Debug, serde::Deserialize)]
struct CargoDiagnostic {
    spans: Vec<CargoDiagnosticSpan>,
}

#[derive(Debug, serde::Deserialize)]
struct CargoDiagnosticSpan {
    file_name: String,
    line_start: u32,
    column_start: u32,
    is_primary: bool,
}

fn collect_cargo_attributions(
    diagnostic: &CargoDiagnostic,
    db: Option<&weaveback_tangle::db::WeavebackDb>,
    project_root: &Path,
    resolver: &PathResolver,
    eval_config: &EvalConfig,
) -> Vec<serde_json::Value> {
    let Some(db) = db else {
        return Vec::new();
    };
    let mut records = Vec::new();
    let mut seen = HashSet::new();

    for span in diagnostic.spans.iter().filter(|span| span.is_primary) {
        let Some(trace) = trace_generated_location(
            &span.file_name,
            span.line_start,
            span.column_start,
            db,
            project_root,
            resolver,
            eval_config,
        ) else {
            continue;
        };

        let dedupe_key = serde_json::to_string(&trace).unwrap_or_default();
        if seen.insert(dedupe_key) {
            records.push(trace);
        }
    }

    records
}

fn collect_cargo_span_attributions(
    diagnostic: &CargoDiagnostic,
    db: Option<&weaveback_tangle::db::WeavebackDb>,
    project_root: &Path,
    resolver: &PathResolver,
    eval_config: &EvalConfig,
) -> Vec<serde_json::Value> {
    let Some(db) = db else {
        return Vec::new();
    };
    let mut records = Vec::new();
    let mut seen = HashSet::new();

    for span in &diagnostic.spans {
        let Some(trace) = trace_generated_location(
            &span.file_name,
            span.line_start,
            span.column_start,
            db,
            project_root,
            resolver,
            eval_config,
        ) else {
            continue;
        };

        let record = json!({
            "generated_file": span.file_name,
            "generated_line": span.line_start,
            "generated_col": span.column_start,
            "is_primary": span.is_primary,
            "trace": trace,
        });
        let dedupe_key = serde_json::to_string(&record).unwrap_or_default();
        if seen.insert(dedupe_key) {
            records.push(record);
        }
    }

    records
}

fn trace_generated_location(
    file_name: &str,
    line: u32,
    col: u32,
    db: &weaveback_tangle::db::WeavebackDb,
    project_root: &Path,
    resolver: &PathResolver,
    eval_config: &EvalConfig,
) -> Option<serde_json::Value> {
    if let Ok(Some(value)) =
        lookup::perform_trace(file_name, line, col, db, resolver, eval_config.clone())
    {
        return Some(value);
    }

    let file_path = Path::new(file_name);
    let rel = file_path
        .strip_prefix(project_root)
        .ok()
        .map(|path| path.to_string_lossy().replace('\\', "/"))?;
    lookup::perform_trace(&rel, line, col, db, resolver, eval_config.clone())
        .ok()
        .flatten()
}

fn build_cargo_attribution_summary(
    span_attributions: &[serde_json::Value],
) -> serde_json::Value {
    #[derive(Default)]
    struct SectionSummary {
        count: usize,
        chunks: std::collections::BTreeSet<String>,
        generated_spans: Vec<serde_json::Value>,
        prose: Option<String>,
        range: Option<serde_json::Value>,
        breadcrumb: Vec<String>,
    }

    #[derive(Default)]
    struct SourceSummary {
        count: usize,
        chunks: std::collections::BTreeSet<String>,
        sections: std::collections::BTreeMap<String, SectionSummary>,
    }

    let mut grouped: std::collections::BTreeMap<String, SourceSummary> =
        std::collections::BTreeMap::new();

    for record in span_attributions {
        let Some(trace) = record.get("trace") else {
            continue;
        };
        let Some(src_file) = trace
            .get("src_file")
            .and_then(|v| v.as_str())
            .or_else(|| trace.get("expanded_file").and_then(|v| v.as_str()))
        else {
            continue;
        };
        let chunk = trace
            .get("chunk")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let breadcrumb = trace
            .get("source_section_breadcrumb")
            .and_then(|v| v.as_array())
            .map(|parts| {
                parts
                    .iter()
                    .filter_map(|part| part.as_str().map(ToOwned::to_owned))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let section_key = if breadcrumb.is_empty() {
            "<unknown>".to_string()
        } else {
            breadcrumb.join(" / ")
        };
        let generated_span = json!({
            "generated_file": record.get("generated_file").cloned().unwrap_or(serde_json::Value::Null),
            "generated_line": record.get("generated_line").cloned().unwrap_or(serde_json::Value::Null),
            "generated_col": record.get("generated_col").cloned().unwrap_or(serde_json::Value::Null),
            "is_primary": record.get("is_primary").cloned().unwrap_or(serde_json::Value::Bool(false)),
            "chunk": if chunk.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(chunk.clone()) },
        });
        let entry = grouped
            .entry(src_file.to_string())
            .or_default();
        entry.count += 1;
        if !chunk.is_empty() {
            entry.chunks.insert(chunk.clone());
        }
        let section = entry.sections.entry(section_key).or_default();
        section.count += 1;
        if !chunk.is_empty() {
            section.chunks.insert(chunk);
        }
        section.generated_spans.push(generated_span);
        if section.prose.is_none() {
            section.prose = trace
                .get("source_section_prose")
                .and_then(|v| v.as_str())
                .map(ToOwned::to_owned);
        }
        if section.range.is_none() {
            section.range = trace.get("source_section_range").cloned();
        }
        if section.breadcrumb.is_empty() {
            section.breadcrumb = breadcrumb;
        }
    }

    json!({
        "count": span_attributions.len(),
        "sources": grouped
            .into_iter()
            .map(|(src_file, summary)| json!({
                "src_file": src_file,
                "count": summary.count,
                "chunks": summary.chunks.into_iter().collect::<Vec<_>>(),
                "sections": summary
                    .sections
                    .into_values()
                    .map(|section| json!({
                        "count": section.count,
                        "chunks": section.chunks.into_iter().collect::<Vec<_>>(),
                        "generated_spans": section.generated_spans,
                        "source_section_breadcrumb": section.breadcrumb,
                        "source_section_range": section.range.unwrap_or(serde_json::Value::Null),
                        "source_section_prose": section.prose.unwrap_or_default(),
                    }))
                    .collect::<Vec<_>>(),
            }))
            .collect::<Vec<_>>(),
    })
}

fn build_location_attribution_summary(records: &[serde_json::Value]) -> serde_json::Value {
    #[derive(Default)]
    struct SectionSummary {
        count: usize,
        chunks: std::collections::BTreeSet<String>,
        locations: Vec<String>,
        prose: Option<String>,
        range: Option<serde_json::Value>,
        breadcrumb: Vec<String>,
    }

    #[derive(Default)]
    struct SourceSummary {
        count: usize,
        chunks: std::collections::BTreeSet<String>,
        sections: std::collections::BTreeMap<String, SectionSummary>,
    }

    let mut grouped: std::collections::BTreeMap<String, SourceSummary> =
        std::collections::BTreeMap::new();

    for record in records.iter().filter(|record| record["ok"].as_bool() == Some(true)) {
        let Some(trace) = record.get("trace") else {
            continue;
        };
        let Some(src_file) = trace
            .get("src_file")
            .and_then(|v| v.as_str())
            .or_else(|| trace.get("expanded_file").and_then(|v| v.as_str()))
        else {
            continue;
        };
        let chunk = trace
            .get("chunk")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let breadcrumb = trace
            .get("source_section_breadcrumb")
            .and_then(|v| v.as_array())
            .map(|parts| {
                parts
                    .iter()
                    .filter_map(|part| part.as_str().map(ToOwned::to_owned))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let section_key = if breadcrumb.is_empty() {
            "<unknown>".to_string()
        } else {
            breadcrumb.join(" / ")
        };

        let source = grouped.entry(src_file.to_string()).or_default();
        source.count += 1;
        if !chunk.is_empty() {
            source.chunks.insert(chunk.clone());
        }

        let section = source.sections.entry(section_key).or_default();
        section.count += 1;
        if !chunk.is_empty() {
            section.chunks.insert(chunk);
        }
        if let Some(location) = record.get("location").and_then(|v| v.as_str()) {
            section.locations.push(location.to_string());
        }
        if section.prose.is_none() {
            section.prose = trace
                .get("source_section_prose")
                .and_then(|v| v.as_str())
                .map(ToOwned::to_owned);
        }
        if section.range.is_none() {
            section.range = trace.get("source_section_range").cloned();
        }
        if section.breadcrumb.is_empty() {
            section.breadcrumb = breadcrumb;
        }
    }

    json!({
        "count": records.iter().filter(|record| record["ok"].as_bool() == Some(true)).count(),
        "sources": grouped
            .into_iter()
            .map(|(src_file, summary)| {
                let mut sections = summary
                    .sections
                    .into_values()
                    .map(|section| json!({
                        "count": section.count,
                        "chunks": section.chunks.into_iter().collect::<Vec<_>>(),
                        "locations": section.locations,
                        "source_section_breadcrumb": section.breadcrumb,
                        "source_section_range": section.range.unwrap_or(serde_json::Value::Null),
                        "source_section_prose": section.prose.unwrap_or_default(),
                    }))
                    .collect::<Vec<_>>();
                sections.sort_by(|a, b| {
                    let ac = a["count"].as_u64().unwrap_or(0);
                    let bc = b["count"].as_u64().unwrap_or(0);
                    bc.cmp(&ac)
                });
                json!({
                    "src_file": src_file,
                    "count": summary.count,
                    "chunks": summary.chunks.into_iter().collect::<Vec<_>>(),
                    "sections": sections,
                })
            })
            .collect::<Vec<_>>(),
    })
}

fn emit_augmented_cargo_message(
    original_line: &str,
    attributions: Vec<serde_json::Value>,
    span_attributions: Vec<serde_json::Value>,
    mut out: impl Write,
) -> std::io::Result<()> {
    let mut value: serde_json::Value = match serde_json::from_str(original_line) {
        Ok(value) => value,
        Err(_) => {
            writeln!(out, "{original_line}")?;
            return Ok(());
        }
    };
    if let Some(obj) = value.as_object_mut() {
        obj.insert(
            "weaveback_attributions".to_string(),
            serde_json::Value::Array(attributions),
        );
        obj.insert(
            "weaveback_span_attributions".to_string(),
            serde_json::Value::Array(span_attributions.clone()),
        );
        obj.insert(
            "weaveback_source_summary".to_string(),
            build_cargo_attribution_summary(&span_attributions),
        );
    }
    serde_json::to_writer(&mut out, &value)?;
    writeln!(out)?;
    Ok(())
}

fn emit_cargo_summary_message(
    compiler_message_count: usize,
    span_attributions: &[serde_json::Value],
    mut out: impl Write,
) -> std::io::Result<()> {
    serde_json::to_writer(
        &mut out,
        &json!({
            "reason": "weaveback-summary",
            "compiler_message_count": compiler_message_count,
            "generated_span_count": span_attributions.len(),
            "weaveback_source_summary": build_cargo_attribution_summary(span_attributions),
        }),
    )?;
    writeln!(out)?;
    Ok(())
}

fn collect_text_attributions(
    text: &str,
    db: Option<&weaveback_tangle::db::WeavebackDb>,
    project_root: &Path,
    resolver: &PathResolver,
    eval_config: &EvalConfig,
) -> Vec<serde_json::Value> {
    let Some(db) = db else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for location in scan_generated_locations(text) {
        let Ok((out_file, line, col)) = parse_generated_location(&location) else {
            continue;
        };
        let Some(trace) = trace_generated_location(
            &out_file,
            line,
            col,
            db,
            project_root,
            resolver,
            eval_config,
        ) else {
            out.push(json!({
                "location": location,
                "ok": false,
                "trace": serde_json::Value::Null,
            }));
            continue;
        };
        out.push(json!({
            "location": location,
            "ok": true,
            "trace": trace,
        }));
    }
    out
}

fn emit_text_attribution_message(
    stream: &str,
    line: &str,
    attributions: Vec<serde_json::Value>,
    mut out: impl Write,
) -> std::io::Result<()> {
    serde_json::to_writer(
        &mut out,
        &json!({
            "reason": "weaveback-text-attribution",
            "stream": stream,
            "text": line,
            "weaveback_attributions": attributions,
            "weaveback_source_summary": build_location_attribution_summary(
                &attributions
            ),
        }),
    )?;
    writeln!(out)?;
    Ok(())
}

fn run_cargo_annotated(
    cargo_args: Vec<String>,
    diagnostics_only: bool,
    db_path: PathBuf,
    gen_dir: PathBuf,
    eval_config: EvalConfig,
) -> Result<(), Error> {
    let project_root = std::env::current_dir().unwrap_or_default();
    let mut stdout_out = std::io::stdout().lock();
    run_cargo_annotated_to_writer(
        cargo_args,
        diagnostics_only,
        db_path,
        gen_dir,
        eval_config,
        &project_root,
        &mut stdout_out,
    )
}

fn run_cargo_annotated_to_writer(
    mut cargo_args: Vec<String>,
    diagnostics_only: bool,
    db_path: PathBuf,
    gen_dir: PathBuf,
    eval_config: EvalConfig,
    project_root: &Path,
    mut out: impl Write,
) -> Result<(), Error> {
    if cargo_args.is_empty() {
        cargo_args.push("check".to_string());
    }
    if !cargo_args
        .iter()
        .any(|arg| arg.starts_with("--message-format"))
    {
        let message_format = "--message-format=json-diagnostic-rendered-ansi".to_string();
        if let Some(idx) = cargo_args.iter().position(|arg| arg == "--") {
            cargo_args.insert(idx, message_format);
        } else {
            cargo_args.push(message_format);
        }
    }

    let resolver = PathResolver::new(project_root.to_path_buf(), gen_dir);
    let db = if db_path.exists() {
        Some(weaveback_tangle::db::WeavebackDb::open_read_only(&db_path)?)
    } else {
        None
    };

    let mut child = Command::new("cargo")
        .args(&cargo_args)
        .current_dir(project_root)
        .stdin(Stdio::inherit())
        .stderr(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(Error::Io)?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| Error::Io(std::io::Error::other("failed to capture cargo stdout")))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| Error::Io(std::io::Error::other("failed to capture cargo stderr")))?;
    let reader = BufReader::new(stdout);
    let err_reader = BufReader::new(stderr);
    let mut compiler_message_count = 0usize;
    let mut all_span_records = Vec::new();
    let (stderr_tx, stderr_rx) = mpsc::channel::<Result<String, std::io::Error>>();

    thread::spawn(move || {
        for line in err_reader.lines() {
            let _ = stderr_tx.send(line);
        }
    });

    for line in reader.lines() {
        let line = line.map_err(Error::Io)?;
        let Ok(envelope) = serde_json::from_str::<CargoMessageEnvelope>(&line) else {
            let attributions =
                collect_text_attributions(&line, db.as_ref(), project_root, &resolver, &eval_config);
            if !attributions.is_empty() {
                emit_text_attribution_message("stdout", &line, attributions, &mut out)
                    .map_err(Error::Io)?;
            } else if !diagnostics_only {
                writeln!(out, "{line}").map_err(Error::Io)?;
            }
            continue;
        };

        if envelope.reason == "compiler-message"
            && let Some(diagnostic) = envelope.message
        {
            compiler_message_count += 1;
            let records =
                collect_cargo_attributions(
                    &diagnostic,
                    db.as_ref(),
                    project_root,
                    &resolver,
                    &eval_config,
                );
            let span_records = collect_cargo_span_attributions(
                &diagnostic,
                db.as_ref(),
                project_root,
                &resolver,
                &eval_config,
            );
            all_span_records.extend(span_records.iter().cloned());
            emit_augmented_cargo_message(&line, records, span_records, &mut out)
                .map_err(Error::Io)?;
        } else if !diagnostics_only || envelope.reason == "build-finished" {
            writeln!(out, "{line}").map_err(Error::Io)?;
        }
    }

    for line in stderr_rx {
        let line = line.map_err(Error::Io)?;
        let attributions =
            collect_text_attributions(&line, db.as_ref(), project_root, &resolver, &eval_config);
        if !attributions.is_empty() {
            emit_text_attribution_message("stderr", &line, attributions, &mut out)
                .map_err(Error::Io)?;
        } else if !diagnostics_only {
            writeln!(out, "{line}").map_err(Error::Io)?;
        }
    }

    emit_cargo_summary_message(compiler_message_count, &all_span_records, &mut out)
        .map_err(Error::Io)?;

    let status = child.wait().map_err(Error::Io)?;
    if status.success() {
        Ok(())
    } else {
        Err(Error::Io(std::io::Error::other(format!(
            "cargo exited with status {status}"
        ))))
    }
}

/// Escape a chunk name for use as a DOT node identifier.
fn dot_id(name: &str) -> String {
    format!("\"{}\"", name.replace('\\', "\\\\").replace('"', "\\\""))
}

fn run_impact(chunk: String, db_path: PathBuf) -> Result<(), Error> {
    let db = open_db(&db_path)?;

    // BFS forward through chunk_deps to collect all transitively reachable chunks.
    let mut reachable: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    seen.insert(chunk.clone());
    let mut queue: std::collections::VecDeque<String> = std::collections::VecDeque::new();
    queue.push_back(chunk.clone());
    while let Some(current) = queue.pop_front() {
        for (child, _src_file) in db.query_chunk_deps(&current)? {
            if seen.insert(child.clone()) {
                reachable.push(child.clone());
                queue.push_back(child);
            }
        }
    }

    // Find affected output files across the root chunk and all reachable chunks.
    let mut affected_files: HashSet<String> = HashSet::new();
    for c in std::iter::once(&chunk).chain(reachable.iter()) {
        for f in db.query_chunk_output_files(c)? {
            affected_files.insert(f);
        }
    }
    let mut affected_files: Vec<String> = affected_files.into_iter().collect();
    affected_files.sort();

    let json = serde_json::json!({
        "chunk": chunk,
        "reachable_chunks": reachable,
        "affected_files": affected_files,
    });
    println!("{}", serde_json::to_string_pretty(&json).unwrap());
    Ok(())
}

fn run_graph(chunk: Option<String>, db_path: PathBuf) -> Result<(), Error> {
    let db = open_db(&db_path)?;

    let edges: Vec<(String, String)> = if let Some(ref root) = chunk {
        // BFS to collect only the edges in the subgraph reachable from root.
        let mut visited: HashSet<String> = HashSet::new();
        let mut queue: std::collections::VecDeque<String> = std::collections::VecDeque::new();
        visited.insert(root.clone());
        queue.push_back(root.clone());
        let mut sub: Vec<(String, String)> = Vec::new();
        while let Some(current) = queue.pop_front() {
            for (child, _) in db.query_chunk_deps(&current)? {
                sub.push((current.clone(), child.clone()));
                if visited.insert(child.clone()) {
                    queue.push_back(child);
                }
            }
        }
        sub
    } else {
        db.query_all_chunk_deps()?.into_iter().map(|(f, t, _)| (f, t)).collect()
    };

    println!("digraph chunk_deps {{");
    for (from, to) in &edges {
        println!("  {} -> {};", dot_id(from), dot_id(to));
    }
    println!("}}");
    Ok(())
}

fn run_search(query: String, limit: usize, db_path: PathBuf) -> Result<(), Error> {
    if !db_path.exists() {
        return Err(Error::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("Database not found at {}. Run weaveback on your source files first.", db_path.display()),
        )));
    }
    let workspace = AgentWorkspace::open(AgentWorkspaceConfig {
        project_root: std::env::current_dir()?,
        db_path,
        gen_dir: PathBuf::from("gen"),
    });
    let results = workspace.session().search(&query, limit)
        .map_err(|e| Error::Io(std::io::Error::other(e)))?;
    if results.is_empty() {
        println!("No results for {:?}", query);
        return Ok(());
    }
    for r in &results {
        let channels = if r.channels.is_empty() {
            String::new()
        } else {
            format!(" via {}", r.channels.join("+"))
        };
        if r.tags.is_empty() {
            println!(
                "{}:{}-{} [{}]{channels}",
                r.src_file,
                r.line_start,
                r.line_end,
                r.block_type,
            );
        } else {
            println!(
                "{}:{}-{} [{}]{channels}  #{}",
                r.src_file,
                r.line_start,
                r.line_end,
                r.block_type,
                r.tags.join(","),
            );
        }
        println!("  {}", r.snippet);
        println!();
    }
    Ok(())
}

fn run_tags(file: Option<String>, db_path: PathBuf) -> Result<(), Error> {
    if !db_path.exists() {
        return Err(Error::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("Database not found at {}. Run weaveback on your source files first.", db_path.display()),
        )));
    }
    let db = weaveback_tangle::db::WeavebackDb::open_read_only(&db_path)
        .map_err(|e| Error::Io(std::io::Error::other(e.to_string())))?;
    let blocks = db.list_block_tags(file.as_deref())
        .map_err(|e| Error::Io(std::io::Error::other(e.to_string())))?;
    if blocks.is_empty() {
        println!("No tagged blocks found. Add a [tags] section to weaveback.toml and run weaveback tangle.");
        return Ok(());
    }
    let mut current_file = String::new();
    for b in &blocks {
        if b.src_file != current_file {
            println!("\n{}", b.src_file);
            current_file = b.src_file.clone();
        }
        println!("  :{} [{}]  #{}", b.line_start, b.block_type, b.tags);
    }
    println!();
    Ok(())
}

fn run_trace(
    out_file: String,
    line: u32,
    col: u32,
    db_path: PathBuf,
    gen_dir: PathBuf,
    eval_config: weaveback_macro::evaluator::EvalConfig
) -> Result<(), Error> {
    let db = open_db(&db_path)?;
    let project_root = std::env::current_dir().unwrap_or_default();
    let resolver = PathResolver::new(project_root, gen_dir);

    match lookup::perform_trace(&out_file, line, col, &db, &resolver, eval_config) {
        Ok(Some(json)) => {
            println!("{}", serde_json::to_string_pretty(&json).unwrap());
            Ok(())
        }
        Ok(None) => {
            eprintln!("No mapping found for {}:{}", out_file, line);
            Ok(())
        }
        Err(lookup::LookupError::InvalidInput(msg)) => {
            Err(Error::Io(std::io::Error::new(std::io::ErrorKind::InvalidInput, msg)))
        }
        Err(lookup::LookupError::Db(e)) => Err(Error::Noweb(WeavebackError::Db(e))),
        Err(lookup::LookupError::Io(e)) => Err(Error::Io(e)),
    }
}

#[derive(serde::Deserialize)]
struct TanglePassCfg {
    dir:              String,
    #[serde(rename = "gen")]
    output_dir:       Option<String>,
    ext:              Option<String>,
    #[serde(default)]
    no_macros:        bool,
    open_delim:       Option<String>,
    close_delim:      Option<String>,
    chunk_end:        Option<String>,
    comment_markers:  Option<String>,
    #[serde(default)]
    sigil:           Vec<String>,
}

#[derive(serde::Deserialize)]
struct TagsCfg {
    /// "anthropic" | "gemini" | "openai" | "ollama"
    #[serde(default = "default_tags_backend")]
    backend:    String,
    /// Model name, e.g. "claude-haiku-4-5-20251001"
    #[serde(default = "default_tags_model")]
    model:      String,
    /// Base URL for openai-compatible / ollama
    endpoint:   Option<String>,
    /// Blocks per LLM request (default: 15)
    #[serde(default = "default_batch_size")]
    batch_size: usize,
}

fn default_tags_backend() -> String { "anthropic".to_string() }
fn default_tags_model()   -> String { "claude-haiku-4-5-20251001".to_string() }
fn default_batch_size()   -> usize  { 15 }

#[derive(serde::Deserialize)]
struct EmbeddingsCfg {
    #[serde(default = "semantic::default_embeddings_backend")]
    backend: String,
    #[serde(default = "semantic::default_embeddings_model")]
    model: String,
    endpoint: Option<String>,
    #[serde(default = "semantic::default_embeddings_batch_size")]
    batch_size: usize,
}

#[derive(serde::Deserialize)]
struct TangleCfg {
    #[serde(rename = "gen")]
    default_gen: Option<String>,
    #[serde(rename = "pass")]
    passes:      Vec<TanglePassCfg>,
        tags:        Option<TagsCfg>,
        embeddings:  Option<EmbeddingsCfg>,
}

fn build_pass_cmd(
    exe: &std::path::Path,
    pass: &TanglePassCfg,
    default_gen: &str,
    force_generated: bool,
) -> std::process::Command {
    let mut cmd = std::process::Command::new(exe);
    cmd.arg("--dir").arg(&pass.dir);
    cmd.arg("--gen").arg(pass.output_dir.as_deref().unwrap_or(default_gen));
    if force_generated {
        cmd.arg("--force-generated");
    }
    if let Some(ext) = &pass.ext {
        cmd.arg("--ext").arg(ext);
    }
    if pass.no_macros {
        cmd.arg("--no-macros");
    }
    if let Some(od) = &pass.open_delim {
        cmd.arg("--open-delim").arg(od);
    }
    if let Some(cd) = &pass.close_delim {
        cmd.arg("--close-delim").arg(cd);
    }
    if let Some(ce) = &pass.chunk_end {
        cmd.arg("--chunk-end").arg(ce);
    }
    if let Some(cm) = &pass.comment_markers {
        cmd.arg("--comment-markers").arg(cm);
    }
    for s in &pass.sigil {
        cmd.arg("--sigil").arg(s);
    }
    cmd.arg("--no-fts");
    cmd
}

fn run_tangle_all(config_path: &std::path::Path, force_generated: bool) -> Result<(), Error> {
    let src = std::fs::read_to_string(config_path)
        .map_err(|e| Error::Io(std::io::Error::new(e.kind(),
            format!("{}: {e}", config_path.display()))))?;
    let cfg: TangleCfg = toml::from_str(&src)
        .map_err(|e| Error::Io(std::io::Error::other(
            format!("{}: {e}", config_path.display()))))?;

    let exe = std::env::current_exe()
        .map_err(Error::Io)?;
    let default_gen = cfg.default_gen.as_deref().unwrap_or(".");

    let errors: Vec<String> = cfg.passes
        .par_iter()
        .filter_map(|pass| {
            let mut cmd = build_pass_cmd(&exe, pass, default_gen, force_generated);
            match cmd.status() {
                Err(e)                => Some(format!("{}: {e}", pass.dir)),
                Ok(s) if !s.success() => Some(format!("tangle pass failed for: {}", pass.dir)),
                _                     => None,
            }
        })
        .collect();

    if let Some(msg) = errors.into_iter().next() {
        return Err(Error::Io(std::io::Error::other(msg)));
    }

    // Rebuild the prose FTS index once after all passes have written their
    // snapshots, so the index reflects the complete current state.
    let db_path = std::path::Path::new("weaveback.db");
    if db_path.exists() {
        match weaveback_tangle::db::WeavebackDb::open(db_path) {
            Ok(mut db) => {
                                if let Some(tags_cfg) = &cfg.tags {
                    tag::run_auto_tag(&mut db, &tag::TagConfig {
                        backend:    tags_cfg.backend.clone(),
                        model:      tags_cfg.model.clone(),
                        endpoint:   tags_cfg.endpoint.clone(),
                        batch_size: tags_cfg.batch_size,
                    });
                }
                                if let Some(embed_cfg) = &cfg.embeddings {
                    semantic::run_auto_embed(&mut db, &semantic::EmbeddingConfig {
                        backend: embed_cfg.backend.clone(),
                        model: embed_cfg.model.clone(),
                        endpoint: embed_cfg.endpoint.clone(),
                        batch_size: embed_cfg.batch_size,
                    });
                }
                if let Err(e) = db.rebuild_prose_fts() {
                    eprintln!("warning: FTS index rebuild failed: {e}");
                }
            }
            Err(e) => eprintln!("warning: could not open db for FTS rebuild: {e}"),
        }
    }
    Ok(())
}

fn run_tag_only(
    config_path: &std::path::Path,
    backend_override:    Option<String>,
    model_override:      Option<String>,
    endpoint_override:   Option<String>,
    batch_size_override: Option<usize>,
    db_path: PathBuf,
) -> Result<(), Error> {
    // Build TagConfig: CLI flags override weaveback.toml [tags] section.
    let tag_cfg: tag::TagConfig = {
        // Try to read toml; if [tags] absent, CLI flags must supply backend+model.
        let toml_tags: Option<TagsCfg> = std::fs::read_to_string(config_path).ok()
            .and_then(|s| toml::from_str::<TangleCfg>(&s).ok())
            .and_then(|c| c.tags);

        let backend = backend_override
            .or_else(|| toml_tags.as_ref().map(|t| t.backend.clone()))
            .unwrap_or_else(default_tags_backend);
        let model = model_override
            .or_else(|| toml_tags.as_ref().map(|t| t.model.clone()))
            .unwrap_or_else(default_tags_model);
        let endpoint = endpoint_override
            .or_else(|| toml_tags.as_ref().and_then(|t| t.endpoint.clone()));
        let batch_size = batch_size_override
            .or_else(|| toml_tags.as_ref().map(|t| t.batch_size))
            .unwrap_or_else(default_batch_size);

        tag::TagConfig { backend, model, endpoint, batch_size }
    };

    if !db_path.exists() {
        return Err(Error::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("Database not found at {}. Run weaveback on your source files first.", db_path.display()),
        )));
    }

    match weaveback_tangle::db::WeavebackDb::open(&db_path) {
        Ok(mut db) => {
            tag::run_auto_tag(&mut db, &tag_cfg);
            if let Err(e) = db.rebuild_prose_fts() {
                eprintln!("warning: FTS index rebuild failed: {e}");
            }
        }
        Err(e) => return Err(Error::Io(std::io::Error::other(e.to_string()))),
    }
    Ok(())
}

use weaveback_lsp::LspClient;

fn run_lsp(
    cmd: LspCommands,
    db_path: PathBuf,
    gen_dir: PathBuf,
    eval_config: EvalConfig,
    override_cmd: Option<String>,
    override_lang: Option<String>,
) -> Result<(), Error> {
    let project_root = std::env::current_dir()?;
    let db = open_db(&db_path)?;
    let resolver = PathResolver::new(project_root.clone(), gen_dir);

    // Determine LSP config based on input file or overrides.
    let sample_file = match &cmd {
        LspCommands::Definition { out_file, .. } => out_file,
        LspCommands::References { out_file, .. } => out_file,
    };
    let ext = Path::new(sample_file).extension().and_then(|e| e.to_str()).unwrap_or("");

    let (lsp_cmd, lsp_lang) = match (override_cmd, override_lang) {
        (Some(c), Some(l)) => (c, l),
        (c, l) => {
            let (def_cmd, def_lang) = weaveback_lsp::get_lsp_config(ext)
                .ok_or_else(|| Error::Io(std::io::Error::other(format!("unsupported file extension: .{}", ext))))?;
            (c.unwrap_or(def_cmd), l.unwrap_or(def_lang))
        }
    };

    let mut client = LspClient::spawn(&lsp_cmd, &[], &project_root, lsp_lang)
        .map_err(|e| Error::Io(std::io::Error::other(format!("failed to start LSP '{}': {e}", lsp_cmd))))?;

    client.initialize(&project_root)
        .map_err(|e| Error::Io(std::io::Error::other(format!("LSP initialization failed: {e}"))))?;

    match cmd {
        LspCommands::Definition { out_file, line, col } => {
            let path = Path::new(&out_file).canonicalize()
                .map_err(|e| Error::Io(std::io::Error::other(format!("invalid file path '{}': {e}", out_file))))?;

            client.did_open(&path)
                .map_err(|e| Error::Io(std::io::Error::other(format!("LSP didOpen failed: {e}"))))?;

            let loc = client.goto_definition(&path, line - 1, col - 1)
                .map_err(|e| Error::Io(std::io::Error::other(format!("LSP definition call failed: {e}"))))?;

            if let Some(loc) = loc {
                let target_path = loc.uri.to_file_path()
                    .map_err(|_| Error::Io(std::io::Error::other("LSP returned non-file URI")))?;
                let target_line = loc.range.start.line + 1;
                let target_col = loc.range.start.character + 1;

                // Map back to source
                let trace = lookup::perform_trace(
                    &target_path.to_string_lossy(),
                    target_line,
                    target_col,
                    &db,
                    &resolver,
                    eval_config,
                ).map_err(|e| Error::Io(std::io::Error::other(format!("Mapping failed: {e:?}"))))?;

                if let Some(res) = trace {
                    println!("{}", serde_json::to_string_pretty(&res).unwrap());
                } else {
                    println!("{}", json!({
                        "out_file": target_path.to_string_lossy(),
                        "out_line": target_line,
                        "out_col":  target_col,
                        "note": "LSP result could not be mapped to source"
                    }));
                }
            } else {
                println!("No definition found.");
            }
        }
        LspCommands::References { out_file, line, col } => {
            let path = Path::new(&out_file).canonicalize()
                .map_err(|e| Error::Io(std::io::Error::other(format!("invalid file path '{}': {e}", out_file))))?;

            client.did_open(&path)
                .map_err(|e| Error::Io(std::io::Error::other(format!("LSP didOpen failed: {e}"))))?;

            let locs = client.find_references(&path, line - 1, col - 1)
                .map_err(|e| Error::Io(std::io::Error::other(format!("LSP references call failed: {e}"))))?;

            let mut results = Vec::new();
            for loc in locs {
                let target_path = loc.uri.to_file_path()
                    .map_err(|_| Error::Io(std::io::Error::other("LSP returned non-file URI")))?;
                let target_line = loc.range.start.line + 1;
                let target_col = loc.range.start.character + 1;

                let trace = lookup::perform_trace(
                    &target_path.to_string_lossy(),
                    target_line,
                    target_col,
                    &db,
                    &resolver,
                    eval_config.clone(),
                ).map_err(|e| Error::Io(std::io::Error::other(format!("Mapping failed: {e:?}"))))?;

                if let Some(res) = trace {
                    results.push(res);
                } else {
                    results.push(json!({
                        "out_file": target_path.to_string_lossy(),
                        "out_line": target_line,
                        "out_col":  target_col,
                        "note": "LSP result could not be mapped to source"
                    }));
                }
            }
            println!("{}", serde_json::to_string_pretty(&results).unwrap());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use weaveback_tangle::db::{Confidence, NowebMapEntry, WeavebackDb};

    #[test]
    fn parse_generated_location_accepts_line_and_optional_col() {
        assert_eq!(
            parse_generated_location("gen/out.rs:17").unwrap(),
            ("gen/out.rs".to_string(), 17, 1)
        );
        assert_eq!(
            parse_generated_location("gen/out.rs:17:9").unwrap(),
            ("gen/out.rs".to_string(), 17, 9)
        );
    }

    #[test]
    fn scan_generated_locations_extracts_unique_specs() {
        let text = "panic at src/generated.rs:1:27\nsee also gen/out.rs:17 and src/generated.rs:1:27";
        assert_eq!(
            scan_generated_locations(text),
            vec!["src/generated.rs:1:27", "gen/out.rs:17"]
        );
    }

    #[test]
    fn scan_generated_locations_trims_punctuation_and_supports_windows_paths() {
        let text = r#"note: (src/generated.rs:1:27), "C:\tmp\gen\out.rs:17:9"."#;
        assert_eq!(
            scan_generated_locations(text),
            vec!["src/generated.rs:1:27", r#"C:\tmp\gen\out.rs:17:9"#]
        );
    }

    #[test]
    fn attribute_command_options_support_multiple_locations() {
        let cli = Cli::try_parse_from([
            "weaveback",
            "attribute",
            "gen/out.rs:17",
            "gen/out.rs:18:3",
        ])
        .unwrap();
        match cli.command.unwrap() {
            Commands::Attribute {
                scan_stdin,
                summary,
                locations,
            } => {
                assert!(!scan_stdin);
                assert!(!summary);
                assert_eq!(locations, vec!["gen/out.rs:17", "gen/out.rs:18:3"]);
            }
            _ => panic!("expected attribute command"),
        }
    }

    #[test]
    fn attribute_command_options_support_scan_stdin() {
        let cli = Cli::try_parse_from([
            "weaveback",
            "attribute",
            "--scan-stdin",
        ])
        .unwrap();
        match cli.command.unwrap() {
            Commands::Attribute {
                scan_stdin,
                summary,
                locations,
            } => {
                assert!(scan_stdin);
                assert!(!summary);
                assert!(locations.is_empty());
            }
            _ => panic!("expected attribute command"),
        }
    }

    #[test]
    fn attribute_command_options_support_summary() {
        let cli = Cli::try_parse_from([
            "weaveback",
            "attribute",
            "--summary",
            "--scan-stdin",
        ])
        .unwrap();
        match cli.command.unwrap() {
            Commands::Attribute {
                scan_stdin,
                summary,
                locations,
            } => {
                assert!(scan_stdin);
                assert!(summary);
                assert!(locations.is_empty());
            }
            _ => panic!("expected attribute command"),
        }
    }

    #[test]
    fn coverage_command_options_support_summary() {
        let cli = Cli::try_parse_from([
            "weaveback",
            "coverage",
            "--summary",
            "lcov.info",
        ])
        .unwrap();
        match cli.command.unwrap() {
            Commands::Coverage {
                summary,
                top_sources,
                top_sections,
                lcov_file,
            } => {
                assert!(summary);
                assert_eq!(top_sources, 10);
                assert_eq!(top_sections, 3);
                assert_eq!(lcov_file, PathBuf::from("lcov.info"));
            }
            _ => panic!("expected coverage command"),
        }
    }

    #[test]
    fn scan_stdin_mode_keeps_bulk_json_shape_for_single_location() {
        let mut db = WeavebackDb::open_temp().expect("db");
        db.set_noweb_entries(
            "out.rs",
            &[(
                0,
                NowebMapEntry {
                    src_file: "src/doc.adoc".to_string(),
                    chunk_name: "main".to_string(),
                    src_line: 3,
                    indent: String::new(),
                    confidence: Confidence::Exact,
                },
            )],
        )
        .expect("noweb");
        db.set_src_snapshot("src/doc.adoc", b"= Root\n\n== Topic\nalpha\n")
            .expect("snapshot");
        let project_root = PathBuf::from(".");
        let resolver = PathResolver::new(project_root, PathBuf::from("gen"));
        let locations = vec!["out.rs:1".to_string()];
        let mut results = Vec::new();

        for location in locations {
            let (out_file, line, col) = parse_generated_location(&location).unwrap();
            match lookup::perform_trace(&out_file, line, col, &db, &resolver, EvalConfig::default()) {
                Ok(Some(json)) => results.push(json!({
                    "location": location,
                    "ok": true,
                    "trace": json,
                })),
                _ => panic!("expected attributed trace"),
            }
        }
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["location"], "out.rs:1");
        assert_eq!(results[0]["ok"], true);
        assert_eq!(results[0]["trace"]["chunk"], "main");
    }

    #[test]
    fn cargo_command_options_support_diagnostics_only() {
        let cli = Cli::try_parse_from([
            "weaveback",
            "cargo",
            "--diagnostics-only",
            "check",
            "-p",
            "weaveback",
        ])
        .unwrap();
        match cli.command.unwrap() {
            Commands::Cargo {
                diagnostics_only,
                args,
            } => {
                assert!(diagnostics_only);
                assert_eq!(args, vec!["check", "-p", "weaveback"]);
            }
            _ => panic!("expected cargo command"),
        }
    }

    #[test]
    fn emit_augmented_cargo_message_attaches_full_trace_json() {
        let line = r#"{"reason":"compiler-message","message":{"spans":[]}}"#;
        let records = vec![json!({
            "out_file": "gen/out.rs",
            "out_line": 17,
            "out_col": 9,
            "src_file": "src/doc.adoc",
            "src_line": 42,
            "src_col": 3,
            "chunk": "main",
            "kind": "Literal",
            "source_section_breadcrumb": ["Root", "Topic"],
            "source_section_prose": "Explain."
        })];
        let span_records = vec![json!({
            "generated_file": "gen/out.rs",
            "generated_line": 17,
            "generated_col": 9,
            "is_primary": true,
            "trace": records[0].clone(),
        })];
        let mut out = Vec::new();
        emit_augmented_cargo_message(line, records, span_records, &mut out).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&out).unwrap();
        let attrs = value["weaveback_attributions"].as_array().unwrap();
        assert_eq!(attrs.len(), 1);
        assert_eq!(attrs[0]["chunk"], "main");
        assert_eq!(attrs[0]["source_section_breadcrumb"], json!(["Root", "Topic"]));
        assert_eq!(attrs[0]["source_section_prose"], "Explain.");
        let span_attrs = value["weaveback_span_attributions"].as_array().unwrap();
        assert_eq!(span_attrs[0]["generated_file"], "gen/out.rs");
        assert_eq!(span_attrs[0]["trace"]["chunk"], "main");
        assert_eq!(value["weaveback_source_summary"]["sources"][0]["src_file"], "src/doc.adoc");
        assert_eq!(
            value["weaveback_source_summary"]["sources"][0]["sections"][0]["source_section_breadcrumb"],
            json!(["Root", "Topic"])
        );
        assert_eq!(
            value["weaveback_source_summary"]["sources"][0]["sections"][0]["generated_spans"][0]["generated_file"],
            "gen/out.rs"
        );
    }

    #[test]
    fn emit_text_attribution_message_wraps_plain_text_line() {
        let mut out = Vec::new();
        emit_text_attribution_message(
            "stderr",
            "panic at src/generated.rs:1:27",
            vec![json!({
                "location": "src/generated.rs:1:27",
                "ok": true,
                "trace": {"expanded_file": "src/doc.adoc", "chunk": "generated"},
            })],
            &mut out,
        )
        .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(value["reason"], "weaveback-text-attribution");
        assert_eq!(value["stream"], "stderr");
        assert_eq!(value["text"], "panic at src/generated.rs:1:27");
        assert_eq!(value["weaveback_attributions"][0]["location"], "src/generated.rs:1:27");
        assert_eq!(
            value["weaveback_source_summary"]["sources"][0]["src_file"],
            "src/doc.adoc"
        );
    }

    #[test]
    fn collect_text_attributions_scans_and_traces_locations() {
        let mut db = WeavebackDb::open_temp().expect("db");
        db.set_noweb_entries(
            "out.rs",
            &[(
                0,
                NowebMapEntry {
                    src_file: "src/doc.adoc".to_string(),
                    chunk_name: "main".to_string(),
                    src_line: 3,
                    indent: String::new(),
                    confidence: Confidence::Exact,
                },
            )],
        )
        .expect("noweb");
        db.set_src_snapshot("src/doc.adoc", b"= Root\n\n== Topic\nalpha\n")
            .expect("snapshot");
        let resolver = PathResolver::new(PathBuf::from("."), PathBuf::from("gen"));
        let records = collect_text_attributions(
            "panic at out.rs:1 and out.rs:1",
            Some(&db),
            Path::new("."),
            &resolver,
            &EvalConfig::default(),
        );
        assert_eq!(records.len(), 1);
        assert_eq!(records[0]["location"], "out.rs:1");
        assert_eq!(records[0]["ok"], true);
        assert_eq!(records[0]["trace"]["chunk"], "main");
    }

    #[test]
    fn build_location_attribution_summary_groups_successful_records() {
        let summary = build_location_attribution_summary(&[
            json!({
                "location": "out.rs:1",
                "ok": true,
                "trace": {
                    "src_file": "src/doc.adoc",
                    "chunk": "main",
                    "source_section_breadcrumb": ["Root", "Topic"],
                    "source_section_prose": "Explain."
                },
            }),
            json!({
                "location": "out.rs:2",
                "ok": false,
                "trace": serde_json::Value::Null,
            }),
        ]);
        assert_eq!(summary["count"], 1);
        assert_eq!(summary["sources"][0]["src_file"], "src/doc.adoc");
        assert_eq!(
            summary["sources"][0]["sections"][0]["locations"],
            json!(["out.rs:1"])
        );
    }

    #[test]
    fn emit_cargo_summary_message_emits_final_grouped_json() {
        let span_records = vec![
            json!({
                "generated_file": "gen/out.rs",
                "generated_line": 17,
                "generated_col": 9,
                "is_primary": true,
                "trace": {
                    "src_file": "src/doc.adoc",
                    "chunk": "main",
                    "source_section_breadcrumb": ["Root", "Topic"],
                    "source_section_prose": "Explain."
                },
            }),
            json!({
                "generated_file": "gen/out.rs",
                "generated_line": 20,
                "generated_col": 1,
                "is_primary": false,
                "trace": {
                    "src_file": "src/doc.adoc",
                    "chunk": "helper",
                    "source_section_breadcrumb": ["Root", "Topic"],
                    "source_section_prose": "Explain."
                },
            }),
        ];
        let mut out = Vec::new();
        emit_cargo_summary_message(3, &span_records, &mut out).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(value["reason"], "weaveback-summary");
        assert_eq!(value["compiler_message_count"], 3);
        assert_eq!(value["generated_span_count"], 2);
        assert_eq!(value["weaveback_source_summary"]["sources"][0]["src_file"], "src/doc.adoc");
        assert_eq!(
            value["weaveback_source_summary"]["sources"][0]["sections"][0]["chunks"],
            json!(["helper", "main"])
        );
    }

    #[test]
    fn run_cargo_annotated_to_writer_traces_real_generated_compile_error() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path();
        std::fs::create_dir_all(root.join("src")).expect("src dir");
        std::fs::write(
            root.join("Cargo.toml"),
            r#"[package]
name = "wb-fixture"
version = "0.1.0"
edition = "2024"
"#,
        )
        .expect("Cargo.toml");
        std::fs::write(
            root.join("src/main.rs"),
            "mod generated;\nfn main() { generated::broken(); }\n",
        )
        .expect("main");
        std::fs::write(
            root.join("src/generated.rs"),
            "pub fn broken() { let x = ; }\n",
        )
        .expect("generated");

        let db_path = root.join("weaveback.db");
        let mut db = WeavebackDb::open(&db_path).expect("db");
        db.set_noweb_entries(
            "src/generated.rs",
            &[(
                0,
                NowebMapEntry {
                    src_file: "src/doc.adoc".to_string(),
                    chunk_name: "generated".to_string(),
                    src_line: 3,
                    indent: String::new(),
                    confidence: Confidence::Exact,
                },
            )],
        )
        .expect("noweb");
        db.set_src_snapshot("src/doc.adoc", b"= Root\n\n== Generated\nThe generated body.\n")
            .expect("snapshot");

        let mut out = Vec::new();
        let err = run_cargo_annotated_to_writer(
            vec!["check".to_string(), "--quiet".to_string()],
            true,
            db_path,
            root.join("gen"),
            EvalConfig::default(),
            root,
            &mut out,
        )
        .expect_err("cargo should fail on generated syntax error");
        let rendered = String::from_utf8(out).expect("utf8");
        let lines = rendered
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("json line"))
            .collect::<Vec<_>>();

        assert!(matches!(err, Error::Io(_)));
        let compiler = lines
            .iter()
            .find(|value| value["reason"] == "compiler-message")
            .expect("compiler message");
        let span_attrs = compiler["weaveback_span_attributions"]
            .as_array()
            .expect("span attributions");
        assert!(!span_attrs.is_empty());
        assert!(span_attrs.iter().any(|record| {
            record["trace"]["src_file"]
                .as_str()
                .or_else(|| record["trace"]["expanded_file"].as_str())
                .is_some_and(|path| path.ends_with("src/doc.adoc"))
                && record["trace"]["source_section_breadcrumb"] == json!(["Root", "Generated"])
        }));

        let summary = lines
            .iter()
            .find(|value| value["reason"] == "weaveback-summary")
            .expect("summary");
        let sections = summary["weaveback_source_summary"]["sources"][0]["sections"]
            .as_array()
            .expect("sections");
        assert!(sections.iter().any(|section| {
            section["source_section_breadcrumb"] == json!(["Root", "Generated"])
                && section["generated_spans"]
                    .as_array()
                    .is_some_and(|spans| spans.iter().any(|span| {
                        span["generated_file"]
                            .as_str()
                            .is_some_and(|file| file.ends_with("src/generated.rs"))
                    }))
        }));
    }

    #[test]
    fn run_cargo_annotated_to_writer_emits_text_attribution_for_text_warning() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path();
        std::fs::create_dir_all(root.join("src")).expect("src dir");
        std::fs::write(
            root.join("Cargo.toml"),
            r#"[package]
name = "wb-fixture-warning"
version = "0.1.0"
edition = "2024"
build = "build.rs"
"#,
        )
        .expect("Cargo.toml");
        std::fs::write(
            root.join("build.rs"),
            "fn main() { println!(\"cargo:warning=src/generated.rs:1:27\"); }\n",
        )
        .expect("build");
        std::fs::write(
            root.join("src/main.rs"),
            "fn main() {}\n",
        )
        .expect("main");
        std::fs::write(
            root.join("src/generated.rs"),
            "pub fn generated() {}\n",
        )
        .expect("generated");

        let db_path = root.join("weaveback.db");
        let mut db = WeavebackDb::open(&db_path).expect("db");
        db.set_noweb_entries(
            "src/generated.rs",
            &[(
                0,
                NowebMapEntry {
                    src_file: "src/doc.adoc".to_string(),
                    chunk_name: "generated".to_string(),
                    src_line: 3,
                    indent: String::new(),
                    confidence: Confidence::Exact,
                },
            )],
        )
        .expect("noweb");
        db.set_src_snapshot("src/doc.adoc", b"= Root\n\n== Generated\nThe generated body.\n")
            .expect("snapshot");

        let mut out = Vec::new();
        run_cargo_annotated_to_writer(
            vec![
                "check".to_string(),
            ],
            true,
            db_path,
            root.join("gen"),
            EvalConfig::default(),
            root,
            &mut out,
        )
        .expect("cargo check should succeed");
        let rendered = String::from_utf8(out).expect("utf8");
        let lines = rendered
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("json line"))
            .collect::<Vec<_>>();

        let text_attr = lines
            .iter()
            .find(|value| value["reason"] == "weaveback-text-attribution")
            .expect("text attribution");
        assert_eq!(text_attr["stream"], "stderr");
        assert!(
            text_attr["weaveback_attributions"]
                .as_array()
                .is_some_and(|items| items.iter().any(|item| {
                    item["trace"]["source_section_breadcrumb"] == json!(["Root", "Generated"])
                }))
        );
    }

    #[test]
    fn parse_lcov_records_extracts_file_line_hits() {
        let text = "TN:\nSF:src/generated.rs\nDA:1,3\nDA:2,0\nend_of_record\nSF:other.rs\nDA:4,1\nend_of_record\n";
        assert_eq!(
            parse_lcov_records(text),
            vec![
                ("src/generated.rs".to_string(), 1, 3),
                ("src/generated.rs".to_string(), 2, 0),
                ("other.rs".to_string(), 4, 1),
            ]
        );
    }

    #[test]
    fn build_coverage_summary_groups_lines_by_source_section() {
        let mut db = WeavebackDb::open_temp().expect("db");
        db.set_noweb_entries(
            "src/generated.rs",
            &[
                (
                    0,
                    NowebMapEntry {
                        src_file: "src/doc.adoc".to_string(),
                        chunk_name: "generated".to_string(),
                        src_line: 3,
                        indent: String::new(),
                        confidence: Confidence::Exact,
                    },
                ),
                (
                    1,
                    NowebMapEntry {
                        src_file: "src/doc.adoc".to_string(),
                        chunk_name: "generated".to_string(),
                        src_line: 3,
                        indent: String::new(),
                        confidence: Confidence::Exact,
                    },
                ),
            ],
        )
        .expect("noweb");
        db.set_src_snapshot(
            "src/doc.adoc",
            b"= Root\n\n== Generated\nThe generated body.\n",
        )
        .expect("snapshot");
        let records = vec![
            ("src/generated.rs".to_string(), 1, 1),
            ("src/generated.rs".to_string(), 2, 0),
            ("unmapped.rs".to_string(), 9, 0),
        ];
        let project_root = PathBuf::from(".");
        let resolver = PathResolver::new(project_root.clone(), PathBuf::from("gen"));
        let summary = build_coverage_summary(
            &records,
            &db,
            &project_root,
            &resolver,
        );
        assert_eq!(summary["line_records"], 3);
        assert_eq!(summary["attributed_records"], 2);
        assert_eq!(summary["unattributed_records"], 1);
        assert!(
            summary["sources"][0]["src_file"]
                .as_str()
                .is_some_and(|path| path.ends_with("src/doc.adoc"))
        );
        assert_eq!(summary["sources"][0]["covered_lines"], 1);
        assert_eq!(summary["sources"][0]["missed_lines"], 1);
        assert_eq!(
            summary["sources"][0]["sections"][0]["source_section_breadcrumb"],
            json!(["Root", "Generated"])
        );
        assert_eq!(
            summary["sources"][0]["sections"][0]["generated_lines"][0]["generated_file"],
            "src/generated.rs"
        );
        assert_eq!(summary["unattributed"][0]["generated_file"], "unmapped.rs");
        assert_eq!(summary["unattributed_files"][0]["generated_file"], "unmapped.rs");
        assert_eq!(summary["unattributed_files"][0]["missed_lines"], 1);
        assert_eq!(summary["unattributed_files"][0]["has_noweb_entries"], false);
    }

    #[test]
    fn build_coverage_summary_marks_partial_unattributed_files() {
        let mut db = WeavebackDb::open_temp().expect("db");
        db.set_noweb_entries(
            "src/generated.rs",
            &[(
                0,
                NowebMapEntry {
                    src_file: "src/doc.adoc".to_string(),
                    chunk_name: "generated".to_string(),
                    src_line: 3,
                    indent: String::new(),
                    confidence: Confidence::Exact,
                },
            )],
        )
        .expect("noweb");
        db.set_src_snapshot(
            "src/doc.adoc",
            b"= Root\n\n== Generated\nThe generated body.\n",
        )
        .expect("snapshot");
        let records = vec![
            ("src/generated.rs".to_string(), 2, 0),
        ];
        let project_root = PathBuf::from(".");
        let resolver = PathResolver::new(project_root.clone(), PathBuf::from("gen"));
        let summary = build_coverage_summary(&records, &db, &project_root, &resolver);
        assert_eq!(summary["unattributed_records"], 1);
        assert_eq!(summary["unattributed_files"][0]["generated_file"], "src/generated.rs");
        assert_eq!(summary["unattributed_files"][0]["has_noweb_entries"], true);
        assert_eq!(summary["unattributed_files"][0]["mapped_line_start"], 1);
        assert_eq!(summary["unattributed_files"][0]["mapped_line_end"], 1);
    }

    #[test]
    fn build_coverage_summary_sorts_sources_and_sections_by_missed_lines() {
        let mut db = WeavebackDb::open_temp().expect("db");
        db.set_noweb_entries(
            "src/a_generated.rs",
            &[
                (
                    0,
                    NowebMapEntry {
                        src_file: "src/a.adoc".to_string(),
                        chunk_name: "alpha".to_string(),
                        src_line: 3,
                        indent: String::new(),
                        confidence: Confidence::Exact,
                    },
                ),
                (
                    1,
                    NowebMapEntry {
                        src_file: "src/a.adoc".to_string(),
                        chunk_name: "alpha".to_string(),
                        src_line: 6,
                        indent: String::new(),
                        confidence: Confidence::Exact,
                    },
                ),
            ],
        )
        .expect("a noweb");
        db.set_noweb_entries(
            "src/b_generated.rs",
            &[
                (
                    0,
                    NowebMapEntry {
                        src_file: "src/b.adoc".to_string(),
                        chunk_name: "beta".to_string(),
                        src_line: 3,
                        indent: String::new(),
                        confidence: Confidence::Exact,
                    },
                ),
                (
                    1,
                    NowebMapEntry {
                        src_file: "src/b.adoc".to_string(),
                        chunk_name: "beta".to_string(),
                        src_line: 3,
                        indent: String::new(),
                        confidence: Confidence::Exact,
                    },
                ),
            ],
        )
        .expect("b noweb");
        db.set_src_snapshot("src/a.adoc", b"= Root\n\n== A1\none\n\n== A2\ntwo\n")
            .expect("a snapshot");
        db.set_src_snapshot("src/b.adoc", b"= Root\n\n== B\nbody\n")
            .expect("b snapshot");
        let records = vec![
            ("src/a_generated.rs".to_string(), 1, 1),
            ("src/a_generated.rs".to_string(), 2, 0),
            ("src/b_generated.rs".to_string(), 1, 0),
            ("src/b_generated.rs".to_string(), 2, 0),
        ];
        let project_root = PathBuf::from(".");
        let resolver = PathResolver::new(project_root.clone(), PathBuf::from("gen"));
        let summary = build_coverage_summary(
            &records,
            &db,
            &project_root,
            &resolver,
        );
        let sources = summary["sources"].as_array().expect("sources");
        assert!(
            sources[0]["src_file"]
                .as_str()
                .is_some_and(|path| path.ends_with("src/b.adoc"))
        );
        let a_sections = sources[1]["sections"].as_array().expect("sections");
        assert_eq!(a_sections[0]["source_section_breadcrumb"], json!(["Root", "A2"]));
        assert_eq!(a_sections[0]["missed_lines"], 1);
        assert_eq!(a_sections[1]["source_section_breadcrumb"], json!(["Root", "A1"]));
    }

    #[test]
    fn build_coverage_summary_view_keeps_ranked_top_slices() {
        let summary = json!({
            "line_records": 3,
            "attributed_records": 3,
            "unattributed_records": 0,
            "unattributed_files": [
                {
                    "generated_file": "gen/a.rs",
                    "missed_lines": 3
                },
                {
                    "generated_file": "gen/b.rs",
                    "missed_lines": 1
                }
            ],
            "sources": [
                {
                    "src_file": "src/a.adoc",
                    "sections": [
                        {"source_section_breadcrumb": ["Root", "A1"]},
                        {"source_section_breadcrumb": ["Root", "A2"]}
                    ]
                },
                {
                    "src_file": "src/b.adoc",
                    "sections": [
                        {"source_section_breadcrumb": ["Root", "B1"]}
                    ]
                }
            ]
        });
        let view = build_coverage_summary_view(&summary, 1, 1);
        assert_eq!(view["summary_view"]["top_sources"], 1);
        assert_eq!(view["summary_view"]["top_sections"], 1);
        assert_eq!(view["summary_view"]["sources"].as_array().unwrap().len(), 1);
        assert_eq!(view["summary_view"]["unattributed_files"].as_array().unwrap().len(), 1);
        assert_eq!(view["summary_view"]["unattributed_files"][0]["generated_file"], "gen/a.rs");
        assert_eq!(
            view["summary_view"]["sources"][0]["sections"].as_array().unwrap().len(),
            1
        );
    }

    #[test]
    fn collect_cargo_attributions_maps_generated_span_back_to_source() {
        let mut db = WeavebackDb::open_temp().expect("db");
        db.set_noweb_entries(
            "out.rs",
            &[(
                0,
                NowebMapEntry {
                    src_file: "src/doc.adoc".to_string(),
                    chunk_name: "main".to_string(),
                    src_line: 3,
                    indent: String::new(),
                    confidence: Confidence::Exact,
                },
            )],
        )
        .expect("noweb");
        db.set_src_snapshot("src/doc.adoc", b"= Root\n\n== Topic\nalpha\n")
            .expect("snapshot");
        let resolver = PathResolver::new(PathBuf::from("."), PathBuf::from("gen"));
        let diagnostic = CargoDiagnostic {
            spans: vec![CargoDiagnosticSpan {
                file_name: "out.rs".to_string(),
                line_start: 1,
                column_start: 1,
                is_primary: true,
            }],
        };

        let records = collect_cargo_attributions(
            &diagnostic,
            Some(&db),
            Path::new("."),
            &resolver,
            &EvalConfig::default(),
        );
        assert_eq!(records.len(), 1);
        assert!(
            records[0]["src_file"]
                .as_str()
                .is_some_and(|path| path.ends_with("src/doc.adoc"))
        );
        assert_eq!(records[0]["src_line"], 4);
        assert_eq!(records[0]["chunk"], "main");
        assert_eq!(records[0]["source_section_breadcrumb"], json!(["Root", "Topic"]));
    }

    #[test]
    fn collect_cargo_span_attributions_keeps_generated_span_context() {
        let mut db = WeavebackDb::open_temp().expect("db");
        db.set_noweb_entries(
            "out.rs",
            &[(
                0,
                NowebMapEntry {
                    src_file: "src/doc.adoc".to_string(),
                    chunk_name: "main".to_string(),
                    src_line: 3,
                    indent: String::new(),
                    confidence: Confidence::Exact,
                },
            )],
        )
        .expect("noweb");
        db.set_src_snapshot("src/doc.adoc", b"= Root\n\n== Topic\nalpha\n")
            .expect("snapshot");
        let resolver = PathResolver::new(PathBuf::from("."), PathBuf::from("gen"));
        let diagnostic = CargoDiagnostic {
            spans: vec![
                CargoDiagnosticSpan {
                    file_name: "out.rs".to_string(),
                    line_start: 1,
                    column_start: 1,
                    is_primary: true,
                },
                CargoDiagnosticSpan {
                    file_name: "out.rs".to_string(),
                    line_start: 1,
                    column_start: 5,
                    is_primary: false,
                },
            ],
        };

        let records = collect_cargo_span_attributions(
            &diagnostic,
            Some(&db),
            Path::new("."),
            &resolver,
            &EvalConfig::default(),
        );
        assert_eq!(records.len(), 2);
        assert_eq!(records[0]["generated_file"], "out.rs");
        assert_eq!(records[0]["trace"]["chunk"], "main");
        assert_eq!(records[1]["is_primary"], false);
    }

    #[test]
    fn build_cargo_attribution_summary_groups_by_source_file() {
        let summary = build_cargo_attribution_summary(&[
            json!({
                "generated_file": "out.rs",
                "generated_line": 1,
                "generated_col": 1,
                "is_primary": true,
                "trace": {
                    "src_file": "src/a.adoc",
                    "chunk": "alpha",
                    "source_section_breadcrumb": ["Root", "Alpha"],
                    "source_section_prose": "Alpha prose."
                }
            }),
            json!({
                "generated_file": "out.rs",
                "generated_line": 2,
                "generated_col": 1,
                "is_primary": false,
                "trace": {
                    "src_file": "src/a.adoc",
                    "chunk": "beta",
                    "source_section_breadcrumb": ["Root", "Alpha"],
                    "source_section_prose": "Alpha prose."
                }
            }),
            json!({
                "generated_file": "out2.rs",
                "generated_line": 1,
                "generated_col": 1,
                "is_primary": true,
                "trace": {
                    "src_file": "src/b.adoc",
                    "chunk": "gamma",
                    "source_section_breadcrumb": ["Root", "Beta"],
                    "source_section_prose": "Beta prose."
                }
            }),
        ]);
        assert_eq!(summary["count"], 3);
        assert_eq!(summary["sources"][0]["src_file"], "src/a.adoc");
        assert_eq!(summary["sources"][0]["count"], 2);
        assert_eq!(
            summary["sources"][0]["sections"][0]["source_section_breadcrumb"],
            json!(["Root", "Alpha"])
        );
        assert_eq!(
            summary["sources"][0]["sections"][0]["generated_spans"][0]["generated_file"],
            "out.rs"
        );
        assert_eq!(summary["sources"][1]["src_file"], "src/b.adoc");
    }
}
