# Single-Pass Runner

## run_single_pass

```rust
// <[process-run]>=
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
            let expanded_str = normalize_expanded_document(expanded_ext, &expanded);
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
                    &expanded_str,
                )?;
            }
            if args.macro_only {
                continue;
            }
            source_contents.insert(src_key.clone(), expanded_str.clone());
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
// @
```

