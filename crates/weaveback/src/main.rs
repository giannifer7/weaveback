use weaveback_macro::{
    evaluator::{EvalConfig, EvalError, Evaluator},
    macro_api::process_string,
};
use weaveback_tangle::{WeavebackError, Clip, SafeFileWriter, SafeWriterConfig};
use weaveback_core::PathResolver;
use clap::Parser;
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

mod apply_back;
mod lookup;
mod mcp;
mod serve;

fn default_pathsep() -> String {
    if cfg!(windows) {
        ";".to_string()
    } else {
        ":".to_string()
    }
}

#[derive(Parser, Debug)]
#[command(
    name = "weaveback",
    version,
    about = "Macro expander + literate-programming chunk extractor in one pass"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    #[command(flatten)]
    args: Args,
}

#[derive(clap::Subcommand, Debug)]
enum Commands {
    /// Trace back output line to its noweb and macro sources
    Trace {
        out_file: String,
        line: u32,
        /// 1-indexed character position within the output line.
        /// Defaults to 1 (first character).  Use this to look past a structural
        /// wrapper and find the token that produced a specific sub-expression.
        #[arg(long, default_value = "1")]
        col: u32,
    },
    /// Find the noweb chunk that produced output line
    Where {
        out_file: String,
        line: u32,
    },
    /// Run as an MCP server for IDE/agent integration
    Mcp,
    /// Propagate edits in gen/ back to the literate source
    ApplyBack {
        /// Relative paths within gen/ to process (default: all modified files)
        files: Vec<String>,
        /// Show what would change without writing anything
        #[arg(long)]
        dry_run: bool,
    },
    /// Show every chunk and output file transitively affected if CHUNK changes
    Impact {
        chunk: String,
    },
    /// Export the chunk-dependency graph in DOT (Graphviz) format
    Graph {
        /// Restrict to the subgraph reachable from this chunk
        #[arg(long)]
        chunk: Option<String>,
    },
    /// Semantic language server operations (requires rust-analyzer)
    Lsp {
        /// Manual override for the LSP command (e.g. "nimlsp")
        #[arg(long)]
        lsp_cmd: Option<String>,
        /// Manual override for the language ID (e.g. "nim")
        #[arg(long)]
        lsp_lang: Option<String>,
        #[command(subcommand)]
        cmd: LspCommands,
    },
    /// Serve docs/html/ locally with live reload and "Edit source" navigation
    Serve {
        /// TCP port to listen on
        #[arg(long, default_value = "7779")]
        port: u16,
        /// Directory to serve (default: <project-root>/docs/html)
        #[arg(long)]
        html: Option<PathBuf>,
        /// Chunk open delimiter for the tangle oracle (default: <[)
        #[arg(long, default_value = "<[")]
        open_delim: String,
        /// Chunk close delimiter for the tangle oracle (default: ]>)
        #[arg(long, default_value = "]>")]
        close_delim: String,
        /// Chunk-end marker for the tangle oracle (default: @@)
        #[arg(long, default_value = "@@")]
        chunk_end: String,
        /// Comment markers for the tangle oracle (comma-separated, default: //)
        #[arg(long, default_value = "//")]
        comment_markers: String,
        /// AI backend for /__ai: "claude-cli" (default), "anthropic", "gemini", "ollama", "openai"
        #[arg(long, default_value = "claude-cli")]
        ai_backend: String,
        /// AI model name (e.g. "claude-3-5-sonnet-20240620", "gemini-1.5-pro", "llama3")
        #[arg(long)]
        ai_model: Option<String>,
        /// AI API endpoint / base URL (for ollama or openai-compatible backends)
        #[arg(long)]
        ai_endpoint: Option<String>,
    },
}

#[derive(clap::Subcommand, Debug)]
enum LspCommands {
    /// Go to definition of a symbol and map it to literate source
    Definition {
        out_file: String,
        line: u32,
        col: u32,
    },
    /// Find all references to a symbol and map them to literate sources
    References {
        out_file: String,
        line: u32,
        col: u32,
    },
}

#[derive(clap::Args, Debug)]
struct Args {
    /// Input files (mutually exclusive with --dir)
    #[arg(required = false)]
    inputs: Vec<PathBuf>,

    // ── weaveback-macro options ──────────────────────────────────────────────────
    /// Base directory prepended to every input path
    #[arg(long, default_value = ".")]
    input_dir: PathBuf,

    /// Special character for macros
    #[arg(long, default_value = "%")]
    special: char,

    /// Skip macro expansion and feed source files directly to the tangle pass.
    /// Use this when the source files contain no macros and the special
    /// character would collide with literal text (e.g. %, ^ in Rust or shell).
    #[arg(long)]
    no_macros: bool,

    /// Include paths for %include/%import (colon-separated on Unix)
    #[arg(long, default_value = ".")]
    include: String,

    /// Path to the weaveback database [default: weaveback.db in current directory]
    #[arg(long, default_value = "weaveback.db")]
    db: PathBuf,

    // ── debugging ─────────────────────────────────────────────────────────────
    /// Print macro-expanded text to stderr before noweb processing
    #[arg(long)]
    dump_expanded: bool,

    // ── weaveback-tangle options ───────────────────────────────────────────────────
    /// Base directory for generated output files
    #[arg(long = "gen", default_value = "gen")]
    gen_dir: PathBuf,

    /// Chunk open delimiter
    #[arg(long, default_value = "<[")]
    open_delim: String,

    /// Chunk close delimiter
    #[arg(long, default_value = "]>")]
    close_delim: String,

    /// Chunk end marker
    #[arg(long, default_value = "@")]
    chunk_end: String,

    /// Comment markers recognised before chunk delimiters (comma-separated)
    #[arg(long, default_value = "#,//")]
    comment_markers: String,

    /// Formatter command per output file extension, e.g. --formatter rs=rustfmt
    #[arg(long, value_name = "EXT=CMD")]
    formatter: Vec<String>,

    // ── batch/directory mode ──────────────────────────────────────────────────
    /// Discover and process driver files under this directory.
    /// A driver is any file (matching --ext) not referenced by a %include() in another such file.
    /// Mutually exclusive with positional input files.
    #[arg(long = "dir", conflicts_with = "inputs")]
    directory: Option<PathBuf>,

    /// File extension(s) to scan in --dir mode (can be repeated).
    /// Default: md. Example: --ext adoc --ext md to scan both.
    #[arg(long, default_value = "md")]
    ext: Vec<String>,

    // ── build-system integration ──────────────────────────────────────────────
    /// Write a Makefile depfile listing every source file read.
    /// In --dir mode the depfile lists ALL matching files found so that
    /// adding a new file triggers a rebuild.
    #[arg(long)]
    depfile: Option<PathBuf>,

    /// Touch this file on success (build-system stamp).
    #[arg(long)]
    stamp: Option<PathBuf>,

    // ── security ──────────────────────────────────────────────────────────────
    /// Allow %env(NAME) to read environment variables.
    /// Disabled by default to prevent templates from silently reading secrets.
    #[arg(long)]
    allow_env: bool,

    /// Allow @file ~/… chunks to write outside the gen/ directory.
    #[arg(long)]
    allow_home: bool,

    /// Treat references to undefined chunks as fatal errors (default: expand to nothing).
    #[arg(long)]
    strict: bool,

    /// Print output paths without writing anything.
    #[arg(long)]
    dry_run: bool,
}

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
        special_char: args.special,
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

    // Phase 1: process each driver and feed result to noweb.
    for full_path in &drivers {
        let content = std::fs::read_to_string(full_path)?;
        
        // Record the configuration used for this source file.
        let tangle_cfg = weaveback_tangle::db::TangleConfig {
            special_char: args.special,
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
    clip.write_files()?;

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
        special_char: args.special,
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
        Some(Commands::Where { out_file, line }) => {
            run_where(out_file, line, cli.args.db, cli.args.gen_dir)
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
        Some(Commands::Lsp { lsp_cmd, lsp_lang, cmd }) => {
            let eval_config = build_eval_config(&cli.args);
            run_lsp(cmd, cli.args.db, cli.args.gen_dir, eval_config, lsp_cmd, lsp_lang)
        }
        Some(Commands::Serve { port, html, open_delim, close_delim, chunk_end, comment_markers, ai_backend, ai_model, ai_endpoint }) => {
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
            serve::run_serve(port, html, tangle_cfg)
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
