// crates/weaveback-macro/src/bin/macro_cli.rs

use weaveback_macro::evaluator::{EvalConfig, EvalError, Evaluator};
use weaveback_macro::macro_api::process_string;
use clap::{ArgGroup, Parser};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
/// Returns the default path separator based on the platform
fn default_pathsep() -> String {
    if cfg!(windows) {
        ";".to_string()
    } else {
        ":".to_string()
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
#[derive(Parser, Debug)]
#[command(
    name = "weaveback-macro",
    version,
    about = "Weaveback macros translator (Rust)",
    group(ArgGroup::new("source").required(true).args(["inputs", "directory"]))
)]
struct Args {
    /// Output path (file or '-' for stdout)
    #[arg(long = "output", default_value = "-")]
    output: PathBuf,

    /// Macro sigil
    #[arg(long = "sigil", default_value = "%")]
    sigil: char,

    /// List of include paths separated by the path separator
    #[arg(long = "include", default_value = ".")]
    include: String,

    /// Path separator (usually ':' on Unix, ';' on Windows)
    #[arg(long = "pathsep", default_value_t = default_pathsep())]
    pathsep: String,

    /// Base directory for input files
    #[arg(long = "input-dir", default_value = ".")]
    input_dir: PathBuf,

    /// Allow %env(NAME) to read environment variables.
    #[arg(long)]
    allow_env: bool,

    /// The input files (mutually exclusive with --dir)
    #[arg(required = false)]
    inputs: Vec<PathBuf>,

    /// Discover and process driver files under this directory.
    /// A driver is any file (matching --ext) not referenced by a %include() in another such file.
    /// Mutually exclusive with positional input files.
    #[arg(long = "dir", conflicts_with = "inputs")]
    directory: Option<PathBuf>,

    /// File extension(s) to scan in --dir mode (can be repeated).
    /// Default: md. Example: --ext adoc --ext md to scan both.
    #[arg(long, default_value = "md")]
    ext: Vec<String>,

    /// Dump the parsed AST for each input file to <file>.ast (or stdout for stdin).
    /// Skips macro evaluation entirely.
    #[arg(long = "dump-ast")]
    dump_ast: bool,
}
fn run(args: Args) -> Result<(), EvalError> {
    let include_paths: Vec<PathBuf> = args
        .include
        .split(&args.pathsep)
        .map(PathBuf::from)
        .collect();

    let config = EvalConfig {
        sigil: args.sigil,
        include_paths,
        discovery_mode: false,
        allow_env: args.allow_env,
    };

    let final_inputs: Vec<PathBuf> = if let Some(ref dir) = args.directory {
        let mut all = Vec::new();
        find_files(dir, &args.ext, &mut all)
            .map_err(|e| EvalError::Runtime(format!("Directory scan failed: {e}")))?;
        all.sort();

        // Discovery pass: identify which files are %include'd by others (fragments).
        let discovery_config = EvalConfig {
            discovery_mode: true,
            ..config.clone()
        };
        let mut included: HashSet<PathBuf> = HashSet::new();
        for f in &all {
            if let Ok(text) = std::fs::read_to_string(f) {
                let mut disc = Evaluator::new(discovery_config.clone());
                if process_string(&text, Some(f), &mut disc).is_ok() {
                    for p in disc.take_discovered_includes() {
                        included.insert(p.canonicalize().unwrap_or(p));
                    }
                }
            }
        }

        all.into_iter()
            .filter(|f| {
                let canon = f.canonicalize().unwrap_or_else(|_| f.to_path_buf());
                !included.contains(&canon)
            })
            .collect()
    } else {
        let mut inputs = Vec::new();
        for inp in &args.inputs {
            let full = args.input_dir.join(inp);
            let canon = full.canonicalize().unwrap_or_else(|_| full.clone());
            if !full.exists() {
                return Err(EvalError::Runtime(format!(
                    "Input file does not exist: {:?}",
                    canon
                )));
            }
            inputs.push(full);
        }
        inputs
    };

    if args.dump_ast {
        return weaveback_macro::ast::dump_macro_ast(args.sigil, &final_inputs);
    }

    weaveback_macro::macro_api::process_files_from_config(&final_inputs, &args.output, config)
}
fn main() {
    let args = Args::parse();
    match run(args) {
        Ok(()) => std::process::exit(0),
        Err(e) => {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
    }
}
