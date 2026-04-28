// weaveback-api/src/coverage/text.rs
// I'd Really Rather You Didn't edit this generated file.

pub fn collect_text_attributions(
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

pub fn emit_text_attribution_message(
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

pub fn run_cargo_annotated(
    cargo_args: Vec<String>,
    diagnostics_only: bool,
    db_path: PathBuf,
    gen_dir: PathBuf,
    eval_config: EvalConfig,
) -> Result<(), CoverageApiError> {
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

pub fn run_cargo_annotated_to_writer(
    mut cargo_args: Vec<String>,
    diagnostics_only: bool,
    db_path: PathBuf,
    gen_dir: PathBuf,
    eval_config: EvalConfig,
    project_root: &Path,
    mut out: impl Write,
) -> Result<(), CoverageApiError> {
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

    let cargo_bin = std::env::var("WEAVEBACK_CARGO_BIN").unwrap_or_else(|_| "cargo".to_string());
    let mut child = Command::new(cargo_bin)
        .args(&cargo_args)
        .current_dir(project_root)
        .stdin(Stdio::inherit())
        .stderr(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(CoverageApiError::Io)?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| CoverageApiError::Io(std::io::Error::other("failed to capture cargo stdout")))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| CoverageApiError::Io(std::io::Error::other("failed to capture cargo stderr")))?;
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
        let line = line.map_err(CoverageApiError::Io)?;
        let Ok(envelope) = serde_json::from_str::<CargoMessageEnvelope>(&line) else {
            let attributions =
                collect_text_attributions(&line, db.as_ref(), project_root, &resolver, &eval_config);
            if !attributions.is_empty() {
                emit_text_attribution_message("stdout", &line, attributions, &mut out)
                    .map_err(CoverageApiError::Io)?;
            } else if !diagnostics_only {
                writeln!(out, "{line}").map_err(CoverageApiError::Io)?;
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
                .map_err(CoverageApiError::Io)?;
        } else if !diagnostics_only || envelope.reason == "build-finished" {
            writeln!(out, "{line}").map_err(CoverageApiError::Io)?;
        }
    }

    for line in stderr_rx {
        let line = line.map_err(CoverageApiError::Io)?;
        let attributions =
            collect_text_attributions(&line, db.as_ref(), project_root, &resolver, &eval_config);
        if !attributions.is_empty() {
            emit_text_attribution_message("stderr", &line, attributions, &mut out)
                .map_err(CoverageApiError::Io)?;
        } else if !diagnostics_only {
            writeln!(out, "{line}").map_err(CoverageApiError::Io)?;
        }
    }

    emit_cargo_summary_message(compiler_message_count, &all_span_records, &mut out)
        .map_err(CoverageApiError::Io)?;

    let status = child.wait().map_err(CoverageApiError::Io)?;
    if status.success() {
        Ok(())
    } else {
        Err(CoverageApiError::Io(std::io::Error::other(format!(
            "cargo exited with status {status}"
        ))))
    }
}

pub fn run_impact(chunk: String, db_path: PathBuf) -> Result<(), CoverageApiError> {
    let json = crate::query::impact_analysis(&chunk, &db_path)?;
    println!("{}", serde_json::to_string_pretty(&json).unwrap());
    Ok(())
}

pub fn run_graph(chunk: Option<String>, db_path: PathBuf) -> Result<(), CoverageApiError> {
    let dot = crate::query::chunk_graph_dot(chunk.as_deref(), &db_path)?;
    println!("{dot}");
    Ok(())
}

pub fn run_search(query: String, limit: usize, db_path: PathBuf) -> Result<(), CoverageApiError> {
    if !db_path.exists() {
        return Err(CoverageApiError::Io(std::io::Error::new(
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
        .map_err(|e| CoverageApiError::Io(std::io::Error::other(e)))?;
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

pub fn run_tags(file: Option<String>, db_path: PathBuf) -> Result<(), CoverageApiError> {
    let blocks = crate::query::list_block_tags(file.as_deref(), &db_path)?;
    if blocks.is_empty() {
        println!("No tagged blocks found. Add a [tags] section to weaveback.toml and run wb-tangle.");
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

pub fn run_trace(
    out_file: String,
    line: u32,
    col: u32,
    db_path: PathBuf,
    gen_dir: PathBuf,
    eval_config: weaveback_macro::evaluator::EvalConfig
) -> Result<(), CoverageApiError> {
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
            Err(CoverageApiError::Io(std::io::Error::new(std::io::ErrorKind::InvalidInput, msg)))
        }
        Err(lookup::LookupError::Db(e)) => Err(CoverageApiError::Noweb(WeavebackError::Db(e))),
        Err(lookup::LookupError::Io(e)) => Err(CoverageApiError::Io(e)),
    }
}

