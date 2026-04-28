// weaveback-serve/src/server/run.rs
// I'd Really Rather You Didn't edit this generated file.

fn find_project_root() -> PathBuf {
    let mut dir = std::env::current_dir().expect("cannot determine cwd");
    loop {
        let cargo_toml = dir.join("Cargo.toml");
        if cargo_toml.exists()
            && let Ok(content) = std::fs::read_to_string(&cargo_toml)
            && content.contains("[workspace]") {
            return dir;
        }
        if !dir.pop() {
            break;
        }
    }
    std::env::current_dir().unwrap()
}

pub fn run_serve(
    port: u16,
    html_override: Option<PathBuf>,
    tangle_cfg: TangleConfig,
    watch: bool,
) -> Result<(), String> {
    let project_root = find_project_root();
    let html_dir = html_override.unwrap_or_else(|| project_root.join("docs").join("html"));
    let html_dir = if html_dir.exists() {
        html_dir.canonicalize().map_err(|e| e.to_string())?
    } else {
        return Err(format!(
            "docs directory not found: {}\nRun `just docs` first to generate the HTML documentation.",
            html_dir.display()
        ));
    };

    let addr = format!("127.0.0.1:{port}");
    let server = Server::http(&addr).map_err(|e| e.to_string())?;

    run_server_loop(
        server,
        project_root,
        html_dir,
        watch,
        tangle_cfg,
    )
}

pub fn run_server_loop(
    server: Server,
    project_root: PathBuf,
    html_dir: PathBuf,
    watch: bool,
    tangle_cfg: TangleConfig,
) -> Result<(), String> {
    let senders: SseSenders = Arc::new(Mutex::new(Vec::new()));
    let reload_version: ReloadVersion = Arc::new(AtomicU64::new(0));
    spawn_watcher(html_dir.clone(), senders.clone(), reload_version.clone());
    if watch {
        spawn_source_watcher(project_root.clone());
    }

    let tangle_cfg = Arc::new(tangle_cfg);

    println!("wb-serve: http://{}", server.server_addr());
    println!("  Serving: {}", html_dir.display());
    println!("  Editor:  $VISUAL / $EDITOR ({})",
        std::env::var("VISUAL")
            .or_else(|_| std::env::var("EDITOR"))
            .unwrap_or_else(|_| "vi (fallback)".into()));
    if watch {
        println!("  Watch:   .adoc + theme sources (tangle + docs on change)");
    }
    println!("  Press Ctrl-C to stop.");

    for request in server.incoming_requests() {
        let html_dir2     = html_dir.clone();
        let senders2      = senders.clone();
        let reload_version2 = reload_version.clone();
        let root2         = project_root.clone();
        let cfg2          = tangle_cfg.clone();
        thread::spawn(move || {
            handle_request(request, &html_dir2, &senders2, &reload_version2, &root2, &cfg2);
        });
    }

    Ok(())
}

