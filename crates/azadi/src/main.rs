// crates/azadi/src/main.rs
//
// Combined macro-expander + literate-programming extractor.
// Runs azadi-macros then azadi-noweb in-process, no subprocess spawning.

use azadi_macros::{
    evaluator::{EvalConfig, EvalError, Evaluator},
    macro_api::process_string,
};
use azadi_noweb::{AzadiError, Clip, SafeFileWriter, SafeWriterConfig};
use clap::Parser;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

mod apply_back;
mod lookup;
mod mcp;

fn default_pathsep() -> String {
    if cfg!(windows) {
        ";".to_string()
    } else {
        ":".to_string()
    }
}

#[derive(Parser, Debug)]
#[command(
    name = "azadi",
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
}

#[derive(clap::Args, Debug)]
struct Args {
    /// Input files (mutually exclusive with --dir)
    #[arg(required = false)]
    inputs: Vec<PathBuf>,

    // ── azadi-macros options ──────────────────────────────────────────────────
    /// Base directory prepended to every input path
    #[arg(long, default_value = ".")]
    input_dir: PathBuf,

    /// Special character for macros
    #[arg(long, default_value = "%")]
    special: char,

    /// Include paths for %include/%import (colon-separated on Unix)
    #[arg(long, default_value = ".")]
    include: String,

    /// Path to the azadi database [default: azadi.db in current directory]
    #[arg(long, default_value = "azadi.db")]
    db: PathBuf,

    // ── debugging ─────────────────────────────────────────────────────────────
    /// Print macro-expanded text to stderr before noweb processing
    #[arg(long)]
    dump_expanded: bool,

    // ── azadi-noweb options ───────────────────────────────────────────────────
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
}

#[derive(Debug)]
enum Error {
    Macro(EvalError),
    Noweb(AzadiError),
    Io(std::io::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Macro(e) => write!(f, "{e}"),
            Error::Noweb(e) => write!(f, "{e}"),
            Error::Io(e) => write!(f, "{e}"),
        }
    }
}

impl From<EvalError> for Error {
    fn from(e: EvalError) -> Self {
        Error::Macro(e)
    }
}
impl From<AzadiError> for Error {
    fn from(e: AzadiError) -> Self {
        Error::Noweb(e)
    }
}
impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}
impl From<azadi_noweb::db::DbError> for Error {
    fn from(e: azadi_noweb::db::DbError) -> Self {
        Error::Noweb(AzadiError::Db(e))
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
            ..SafeWriterConfig::default()
        },
    );
    let mut clip = Clip::new(
        safe_writer,
        &args.open_delim,
        &args.close_delim,
        &args.chunk_end,
        &comment_markers,
    );

    // Determine the set of driver files to process and all .adoc files for the depfile.
    let (drivers, all_adoc): (Vec<PathBuf>, Vec<PathBuf>) = if let Some(ref dir) = args.directory {
        let mut all = Vec::new();
        find_files(dir, &args.ext, &mut all).map_err(Error::Io)?;
        all.sort();

        // Discovery pass: evaluate each file with discovery_mode=true so that
        // %include/%import resolve their path arguments fully (handling %if,
        // computed paths, etc.) but do not recurse into the included file.
        // Each file gets a fresh evaluator so scope does not leak between files.
        let discovery_config = EvalConfig {
            discovery_mode: true,
            ..eval_config.clone()
        };
        let mut included: HashSet<PathBuf> = HashSet::new();
        for adoc in &all {
            if let Ok(text) = std::fs::read_to_string(adoc) {
                let mut disc = Evaluator::new(discovery_config.clone());
                // Ignore evaluation errors — a broken file is not a fragment.
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

    // Phase 1: macro-expand each driver, feed result to noweb.
    for full_path in &drivers {
        let content = std::fs::read_to_string(full_path)?;
        
        let (expanded, map_entries) = azadi_macros::macro_api::process_string_tracing(
            &content,
            Some(full_path),
            &mut evaluator,
        )?;
        let serialized_entries: Vec<(u32, Vec<u8>)> = map_entries.into_iter()
            .map(|(li, entry)| (li, postcard::to_allocvec(&entry).unwrap()))
            .collect();
        clip.db().set_macro_map_entries(&full_path.to_string_lossy(), &serialized_entries)?;
        
        let expanded_str = String::from_utf8_lossy(&expanded);
        if args.dump_expanded {
            eprintln!("=== expanded: {} ===", full_path.display());
            eprintln!("{}", expanded_str);
            eprintln!("=== end: {} ===", full_path.display());
        }
        clip.read(&expanded_str, &full_path.to_string_lossy());
    }

    // Phase 2: write all @file chunks.
    clip.write_files()?;

    // Phase 3: snapshot all source files read this run.
    (|| -> Result<(), azadi_noweb::AzadiError> {
        for path in evaluator.source_files() {
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
        // In directory mode: depend on all .adoc so adding a new file triggers rebuild.
        // In file mode: depend only on files actually read by the evaluator.
        let deps: Vec<PathBuf> = if args.directory.is_some() {
            all_adoc
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

fn main() {
    let cli = Cli::parse();
    
    let result = match cli.command {
        Some(Commands::Trace { out_file, line }) => {
            run_trace(out_file, line, cli.args.db, cli.args.gen_dir)
        }
        Some(Commands::Where { out_file, line }) => {
            run_where(out_file, line, cli.args.db, cli.args.gen_dir)
        }
        Some(Commands::Mcp) => {
            mcp::run_mcp(cli.args.db, cli.args.gen_dir)
        }
        Some(Commands::ApplyBack { files, dry_run }) => {
            let opts = apply_back::ApplyBackOptions {
                db_path: cli.args.db,
                gen_dir: cli.args.gen_dir,
                dry_run,
                files,
            };
            apply_back::run_apply_back(opts).map_err(|e| Error::Io(std::io::Error::other(e.to_string())))
        }
        None => run(cli.args),
    };

    if let Err(e) = result {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}

// ── query tools ─────────────────────────────────────────────────────────────

fn run_where(out_file: String, line: u32, db_path: PathBuf, gen_dir: PathBuf) -> Result<(), Error> {
    if !db_path.exists() {
        return Err(Error::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("Database not found at {}. Run azadi on your source files first.", db_path.display()),
        )));
    }
    let db = azadi_noweb::db::AzadiDb::open(&db_path)?;

    match lookup::perform_where(&out_file, line, &db, &gen_dir) {
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
        Err(lookup::LookupError::Db(e)) => Err(Error::Noweb(AzadiError::Db(e))),
        Err(lookup::LookupError::Io(e)) => Err(Error::Io(e)),
    }
}

fn run_trace(out_file: String, line: u32, db_path: PathBuf, gen_dir: PathBuf) -> Result<(), Error> {
    if !db_path.exists() {
        return Err(Error::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("Database not found at {}. Run azadi on your source files first.", db_path.display()),
        )));
    }
    let db = azadi_noweb::db::AzadiDb::open(&db_path)?;

    match lookup::perform_trace(&out_file, line, &db, &gen_dir) {
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
        Err(lookup::LookupError::Db(e)) => Err(Error::Noweb(AzadiError::Db(e))),
        Err(lookup::LookupError::Io(e)) => Err(Error::Io(e)),
    }
}
