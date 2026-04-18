// crates/weaveback-macro/src/bin/macro_cli.rs

use weaveback_macro::evaluator::{EvalConfig, EvalError, Evaluator};
use weaveback_macro::macro_api::{discover_includes_in_file, process_files};
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
fn is_ascii_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn apply_cli_defines(eval: &mut Evaluator, defines: &[String]) -> Result<(), EvalError> {
    for item in defines {
        let (name, value) = item.split_once('=').ok_or_else(|| {
            EvalError::InvalidUsage(format!("define: expected NAME=VALUE, got '{item}'"))
        })?;
        if !is_ascii_identifier(name) {
            return Err(EvalError::InvalidUsage(format!(
                "define: '{name}' is not a valid identifier"
            )));
        }
        eval.set_variable(name, value);
    }
    Ok(())
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

    /// Optional prefix prepended to environment lookups.
    /// Example: `--env-prefix WB_` makes `%env(PATH)` read `WB_PATH`.
    #[arg(long)]
    env_prefix: Option<String>,

    /// Maximum macro recursion depth for this run.
    #[arg(long = "recursion-limit", default_value_t = weaveback_core::MAX_RECURSION_DEPTH)]
    recursion_limit: usize,

    /// Define a top-level variable before evaluation. Repeatable.
    /// Form: `-D NAME=VALUE`
    #[arg(short = 'D', long = "define")]
    define: Vec<String>,

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
        allow_env: args.allow_env,
        env_prefix: args.env_prefix.clone(),
        recursion_limit: args.recursion_limit,
    };

    let final_inputs: Vec<PathBuf> = if let Some(ref dir) = args.directory {
        let mut all = Vec::new();
        find_files(dir, &args.ext, &mut all)
            .map_err(|e| EvalError::Runtime(format!("Directory scan failed: {e}")))?;
        all.sort();

        // Discovery pass: identify which files are %include'd by others (fragments).
        let mut included: HashSet<PathBuf> = HashSet::new();
        for f in &all {
            let mut disc = Evaluator::new(config.clone());
            apply_cli_defines(&mut disc, &args.define)?;
            if let Ok(paths) = discover_includes_in_file(f, &mut disc) {
                for p in paths {
                    included.insert(p.canonicalize().unwrap_or(p));
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

    let mut evaluator = Evaluator::new(config);
    apply_cli_defines(&mut evaluator, &args.define)?;
    process_files(&final_inputs, &args.output, &mut evaluator)
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
#[cfg(test)]
mod bin_tests {
    use super::*;

    struct TestWorkspace {
        root: PathBuf,
    }

    impl TestWorkspace {
        fn new() -> Self {
            let unique = format!(
                "wb-macro-bin-tests-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            );
            let root = std::env::temp_dir().join(unique);
            std::fs::create_dir_all(&root).unwrap();
            Self { root }
        }

        fn write(&self, name: &str, content: &str) -> PathBuf {
            let p = self.root.join(name);
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(&p, content).unwrap();
            p
        }
    }

    impl Drop for TestWorkspace {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn default_args() -> Args {
        Args {
            output: PathBuf::from("-"),
            sigil: '%',
            include: ".".to_string(),
            pathsep: default_pathsep(),
            input_dir: PathBuf::from("."),
            allow_env: false,
            env_prefix: None,
            recursion_limit: 1000,
            define: vec![],
            inputs: vec![],
            directory: None,
            ext: vec!["md".to_string()],
            dump_ast: false,
        }
    }

    #[test]
    fn test_bin_run_basic() {
        let ws = TestWorkspace::new();
        let input = ws.write("test.md", "hello %def(x,y)%x() world");
        let output = ws.root.join("out.txt");

        let mut args = default_args();
        args.inputs = vec![input];
        args.output = output.clone();

        run(args).unwrap();

        let body = std::fs::read_to_string(output).unwrap();
        assert_eq!(body.trim(), "hello y world");
    }

    #[test]
    fn test_bin_run_dir_scan() {
        let ws = TestWorkspace::new();
        // Create a driver and a fragment
        ws.write("driver.md", "include %include(frag.md)");
        ws.write("frag.md", "fragment content");

        let output = ws.root.join("out.txt");
        let mut args = default_args();
        args.directory = Some(ws.root.clone());
        args.include = ws.root.to_string_lossy().to_string(); // Ensure includes are found
        args.output = output.clone();

        run(args).unwrap();

        let body = std::fs::read_to_string(output).unwrap();
        assert!(body.contains("fragment content"));
    }

    #[test]
    fn test_bin_run_not_found() {
        let mut args = default_args();
        args.inputs = vec![PathBuf::from("nonexistent.md")];
        let res = run(args);
        assert!(res.is_err());
        assert!(res.unwrap_err().to_string().contains("does not exist"));
    }

    #[test]
    fn test_bin_run_dump_ast() {
        let ws = TestWorkspace::new();
        let input = ws.write("test.md", "hello world");

        let mut args = default_args();
        args.inputs = vec![input.clone()];
        args.dump_ast = true;

        run(args).unwrap();

        let ast_file = input.with_extension("ast");
        assert!(ast_file.exists());
        let _ = std::fs::remove_file(ast_file);
    }
}
