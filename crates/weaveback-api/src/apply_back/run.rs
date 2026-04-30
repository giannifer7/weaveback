// weaveback-api/src/apply_back/run.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;

pub fn run_apply_back(opts: ApplyBackOptions, out: &mut dyn Write) -> Result<(), ApplyBackError> {
    if !opts.db_path.exists() {
        let _ = writeln!(out,
            "Database not found at {}. Run weaveback on your source files first.",
            opts.db_path.display()
        );
        return Ok(());
    }

    let db = WeavebackDb::open(&opts.db_path)?;

    // If gen_dir is the default "gen" and that directory doesn't exist, fall back
    // to the gen_dir stored in the database from the last tangle run.
    let gen_dir = {
        let default_gen = std::path::PathBuf::from("gen");
        if opts.gen_dir == default_gen && !default_gen.exists() {
            db.get_run_config("gen_dir")?
                .map(std::path::PathBuf::from)
                .unwrap_or(opts.gen_dir)
        } else {
            opts.gen_dir
        }
    };

    let project_root = opts.db_path
        .canonicalize()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    let resolver = PathResolver::new(project_root.clone(), gen_dir.clone());

    let baselines: Vec<(String, Vec<u8>)> = if opts.files.is_empty() {
        db.list_baselines()?
    } else {
        opts.files
            .iter()
            .filter_map(|f| db.get_baseline(f).ok().flatten().map(|b| (f.clone(), b)))
            .collect()
    };

    let sigil = opts.eval_config.as_ref().map_or('%', |ec| ec.sigil);

    // Snapshot cache: driver path → bytes.  Populated lazily.
    let mut snapshot_cache: HashMap<String, Option<Vec<u8>>> = HashMap::new();
    let mut lsp_clients: HashMap<String, LspClient> = HashMap::new();

    let mut any_changed = false;

    for (rel_path, baseline_bytes) in &baselines {
        let gen_path = gen_dir.join(rel_path);
        let current_bytes = match std::fs::read(&gen_path) {
            Ok(b) => b,
            Err(_) => {
                let _ = writeln!(out, "  skip {}: file not found in gen/", rel_path);
                continue;
            }
        };

        if current_bytes == *baseline_bytes { continue; }

        any_changed = true;
        let _ = writeln!(out, "Processing {}", rel_path);

        let baseline_str = String::from_utf8_lossy(baseline_bytes);
        let current_str  = String::from_utf8_lossy(&current_bytes);
        let baseline_lines: Vec<&str> = baseline_str.lines().collect();
        let current_lines:  Vec<&str> = current_str.lines().collect();

        let mut src_patches: HashMap<String, Vec<Patch>> = HashMap::new();
        let mut skipped = 0usize;

        let diff = TextDiff::from_lines(baseline_str.as_ref(), current_str.as_ref());
        for op in diff.ops() {
            match op {
                similar::DiffOp::Equal { .. } => {}

                similar::DiffOp::Replace { old_index, old_len, new_index, new_len } => {
                    let old_lines = &baseline_lines[*old_index..*old_index + *old_len];
                    let new_lines = &current_lines[*new_index..*new_index + *new_len];

                    // If it's a 1-for-1 replacement, we try to go two levels deep (macros).
                    if old_len == new_len && *old_len == 1 {
                        let out_line_0 = *old_index as u32;
                        let old_line = old_lines[0];
                        let new_line = new_lines[0];

                        match resolve_noweb_entry(&db, rel_path, out_line_0, &resolver)? {
                            None => {
                                let _ = writeln!(out, "  skip line {}: no source map entry", out_line_0 + 1);
                                skipped += 1;
                            }
                            Some(entry) => {
                                let old_text = strip_indent(old_line, &entry.indent).to_string();
                                let new_text = strip_indent(new_line, &entry.indent).to_string();

                                let snap = snapshot_cache
                                    .entry(entry.src_file.clone())
                                    .or_insert_with(|| {
                                        db.get_src_snapshot(&entry.src_file).ok().flatten()
                                    })
                                    .as_deref();

                                // Retrieve the config used for this source file to get the correct sigil.
                                let mut file_eval_config = opts.eval_config.clone();
                                let mut file_special_char = sigil;
                                if let Ok(Some(cfg)) = weaveback_tangle::lookup::find_best_source_config(&db, &entry.src_file) {
                                    if file_eval_config.is_none() {
                                        file_eval_config = Some(EvalConfig::default());
                                    }
                                    if let Some(ec) = &mut file_eval_config {
                                        ec.sigil = cfg.sigil;
                                    }
                                    file_special_char = cfg.sigil;
                                }

                                let source = if let Some(ec) = &file_eval_config {
                                    let lsp_hint = lsp_definition_hint(
                                        rel_path,
                                        out_line_0,
                                        entry.indent.chars().count() as u32 + 1,
                                        &resolver,
                                        &db,
                                        ec,
                                        &mut lsp_clients,
                                    );
                                    resolve_best_patch_source(
                                        rel_path,
                                        out_line_0,
                                        &old_text,
                                        &new_text,
                                        entry.indent.chars().count() as u32,
                                        &db, &resolver, ec,
                                        &entry.src_file, entry.src_line,
                                        snap, file_special_char, 1,
                                        lsp_hint.as_ref(),
                                    )?
                                } else {
                                    PatchSource::Noweb {
                                        src_file: entry.src_file.clone(),
                                        src_line: entry.src_line as usize,
                                        len: 1,
                                    }
                                };

                                let file_key = source.src_file().to_string();
                                src_patches
                                    .entry(file_key)
                                    .or_default()
                                    .push(Patch {
                                        source,
                                        old_text,
                                        new_text,
                                        expanded_line: entry.src_line,
                                    });
                            }
                        }
                        continue;
                    }

                    // For multi-line or size-changing Replace, we only support Noweb-level patching for now.
                    // Check if the entire hunk maps to a continuous region in one source file.
                    let mut hunk_entries = Vec::new();
                    for i in 0..*old_len {
                        hunk_entries.push(resolve_noweb_entry(&db, rel_path, (*old_index + i) as u32, &resolver)?);
                    }

                    if hunk_entries.iter().all(|e| e.is_some()) {
                        let entries: Vec<_> = hunk_entries.into_iter().flatten().collect();
                        let first = &entries[0];
                        if entries.iter().all(|e| e.src_file == first.src_file && e.indent == first.indent)
                            && entries.windows(2).all(|w| w[1].src_line == w[0].src_line + 1)
                        {
                            let old_text = old_lines.iter().map(|l| strip_indent(l, &first.indent)).collect::<Vec<_>>().join("\n");
                            let new_text = new_lines.iter().map(|l| strip_indent(l, &first.indent)).collect::<Vec<_>>().join("\n");

                            src_patches
                                .entry(first.src_file.clone())
                                .or_default()
                                .push(Patch {
                                    source: PatchSource::Noweb {
                                        src_file: first.src_file.clone(),
                                        src_line: first.src_line as usize,
                                        len: *old_len,
                                    },
                                    old_text,
                                    new_text,
                                    expanded_line: first.src_line,
                                });
                            continue;
                        }
                    }

                    let _ = writeln!(out,
                        "  skip lines {}-{}: complex size-changing hunk ({} → {} lines) — edit literate source manually",
                        old_index + 1, old_index + old_len, old_len, new_len,
                    );
                    skipped += old_len;
                }

                similar::DiffOp::Delete { old_index, old_len, .. } => {
                    let mut hunk_entries = Vec::new();
                    for i in 0..*old_len {
                        hunk_entries.push(resolve_noweb_entry(&db, rel_path, (*old_index + i) as u32, &resolver)?);
                    }

                    if hunk_entries.iter().all(|e| e.is_some()) {
                        let entries: Vec<_> = hunk_entries.into_iter().flatten().collect();
                        let first = &entries[0];
                        if entries.iter().all(|e| e.src_file == first.src_file && e.indent == first.indent)
                            && entries.windows(2).all(|w| w[1].src_line == w[0].src_line + 1)
                        {
                            let old_text = baseline_lines[*old_index..*old_index + *old_len]
                                .iter().map(|l| strip_indent(l, &first.indent)).collect::<Vec<_>>().join("\n");

                            src_patches
                                .entry(first.src_file.clone())
                                .or_default()
                                .push(Patch {
                                    source: PatchSource::Noweb {
                                        src_file: first.src_file.clone(),
                                        src_line: first.src_line as usize,
                                        len: *old_len,
                                    },
                                    old_text,
                                    new_text: "".to_string(),
                                    expanded_line: first.src_line,
                                });
                            continue;
                        }
                    }

                    let _ = writeln!(out,
                        "  skip lines {}-{}: {} deleted line(s) — remove from literate source manually",
                        old_index + 1, old_index + old_len, old_len,
                    );
                    skipped += old_len;
                }

                similar::DiffOp::Insert { old_index, new_index, new_len, .. } => {
                    let mut is_after = true;
                    let target_entry = if *old_index > 0 {
                        resolve_noweb_entry(&db, rel_path, (*old_index - 1) as u32, &resolver)?
                    } else {
                        is_after = false;
                        resolve_noweb_entry(&db, rel_path, *old_index as u32, &resolver)?
                    };

                    if let Some(entry) = target_entry {
                        let new_text = current_lines[*new_index..*new_index + *new_len]
                            .iter().map(|l| strip_indent(l, &entry.indent)).collect::<Vec<_>>().join("\n");

                        let src_line = if is_after { entry.src_line as usize + 1 } else { entry.src_line as usize };

                        src_patches
                            .entry(entry.src_file.clone())
                            .or_default()
                            .push(Patch {
                                source: PatchSource::Noweb {
                                    src_file: entry.src_file.clone(),
                                    src_line,
                                    len: 0,
                                },
                                old_text: "".to_string(),
                                new_text: format!("{}\n", new_text),
                                expanded_line: entry.src_line,
                            });
                    } else {
                        let _ = writeln!(out,
                            "  skip {} inserted line(s) at gen/ line {} — add to literate source manually",
                            new_len, old_index + 1,
                        );
                        skipped += new_len;
                    }
                }
            }
        }

        // Apply collected patches to each source file.
        for (src_file, patches) in &src_patches {
            let snap = snapshot_cache
                .entry(src_file.clone())
                .or_insert_with(|| {
                    db.get_src_snapshot(src_file).ok().flatten()
                })
                .as_deref();

            // Retrieve the config used for this source file to get the correct sigil.
            let mut file_eval_config = opts.eval_config.clone();
            let mut file_special_char = sigil;
            if let Ok(Some(cfg)) = weaveback_tangle::lookup::find_best_source_config(&db, src_file) {
                if file_eval_config.is_none() {
                    file_eval_config = Some(EvalConfig::default());
                }
                if let Some(ec) = &mut file_eval_config {
                    ec.sigil = cfg.sigil;
                }
                file_special_char = cfg.sigil;
            }

            apply_patches_to_file(
                FilePatchContext {
                    db: &db,
                    src_file,
                    src_root: &project_root,
                    patches,
                    dry_run: opts.dry_run,
                    eval_config: file_eval_config,
                    snapshot: snap,
                    sigil: file_special_char,
                },
                &mut skipped,
                out,
            )?;
        }

        if opts.dry_run {
            let _ = writeln!(out, "  [dry-run] would update baseline for {}", rel_path);
        } else if skipped == 0 {
            db.set_baseline(rel_path, &current_bytes)?;
            let _ = writeln!(out, "  baseline updated for {}", rel_path);
        } else {
            let _ = writeln!(out,
                "  baseline NOT updated for {} ({} line(s) could not be applied)",
                rel_path, skipped,
            );
        }
    }

    if !any_changed {
        let _ = writeln!(out, "No modified gen/ files found.");
    }

    Ok(())
}

