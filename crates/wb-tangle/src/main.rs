use thiserror::Error;
use weaveback_macro::evaluator::EvalError;
use weaveback_tangle::WeavebackError;

#[derive(Debug, Error)]
enum Error {
    #[error("{0}")]
    Macro(#[from] EvalError),
    #[error("{0}")]
    Noweb(#[from] WeavebackError),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Process(#[from] weaveback_api::process::ProcessError),
    #[error("{0}")]
    Generic(String),
}
mod cli_generated;
use cli_generated::{Cli, Commands, SinglePassCli};
use clap::Parser;
fn run_multi_pass(config: &std::path::Path, force_generated: bool) -> Result<(), Error> {
    weaveback_api::tangle::run_tangle_all(config, force_generated).map_err(Error::Io)
}

fn run_single_pass_from_cli(s: SinglePassCli, force_generated: bool) -> Result<(), Error> {
    use weaveback_api::process::{SinglePassArgs, run_single_pass};
    run_single_pass(SinglePassArgs {
        inputs:          s.inputs,
        directory:       s.directory,
        input_dir:       s.input_dir,
        gen_dir:         s.gen_dir,
        open_delim:      s.open_delim,
        close_delim:     s.close_delim,
        chunk_end:       s.chunk_end,
        comment_markers: s.comment_markers,
        ext:             s.ext,
        no_macros:       s.no_macros,
        dry_run:         s.dry_run,
        db:              s.db,
        depfile:         s.depfile,
        stamp:           s.stamp,
        strict:          s.strict,
        warn_unused:     s.warn_unused,
        allow_env:       s.allow_env,
        allow_home:      s.allow_home,
        force_generated,
        sigil:           s.sigil,
        include:         s.include,
        formatter:       s.formatter,
        no_fts:          s.no_fts,
        dump_expanded:   s.dump_expanded,
        project_root:    None,
    }).map_err(Error::Generic)?;
    Ok(())
}

fn run_apply_back(
    files: Vec<String>,
    dry_run: bool,
    single: &SinglePassCli,
) -> Result<(), Error> {
    use weaveback_api::apply_back::{ApplyBackOptions, run_apply_back};

    let pathsep = if cfg!(windows) { ";" } else { ":" };
    let include_paths: Vec<std::path::PathBuf> = single
        .include
        .split(pathsep)
        .map(std::path::PathBuf::from)
        .collect();
    let eval_config = weaveback_macro::evaluator::EvalConfig {
        sigil: single.sigil,
        include_paths,
        allow_env: single.allow_env,
        ..Default::default()
    };
    let opts = ApplyBackOptions {
        db_path: single.db.clone(),
        gen_dir: single.gen_dir.clone(),
        dry_run,
        files,
        eval_config: Some(eval_config),
    };
    run_apply_back(opts, &mut std::io::stdout())
        .map_err(|e| Error::Io(std::io::Error::other(e.to_string())))
}

fn main() {
    let cli = Cli::parse();

    let result: Result<(), Error> = match cli.command {
        Some(Commands::ApplyBack { files, dry_run }) => {
            run_apply_back(files, dry_run, &cli.single)
        }
        None if cli.single.directory.is_some() || !cli.single.inputs.is_empty() => {
            run_single_pass_from_cli(cli.single, cli.force_generated)
        }
        None => run_multi_pass(&cli.config, cli.force_generated),
    };

    if let Err(e) = result {
        eprintln!("wb-tangle: {e}");
        std::process::exit(1);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    struct TestWorkspace {
        root: PathBuf,
    }

    impl TestWorkspace {
        fn new() -> Self {
            let unique = format!(
                "wb-tangle-tests-{}-{}",
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

        fn db(&self) -> PathBuf {
            self.root.join("weaveback.db")
        }

        fn gen_dir(&self) -> PathBuf {
            self.root.join("gen")
        }

        fn gen_file(&self, path: &str) -> PathBuf {
            self.gen_dir().join(path)
        }

        fn open_db(&self) -> weaveback_tangle::db::WeavebackDb {
            weaveback_tangle::db::WeavebackDb::open(self.db()).unwrap()
        }
    }

    impl Drop for TestWorkspace {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn default_single_pass(root: &TestWorkspace) -> SinglePassCli {
        SinglePassCli {
            inputs: vec![],
            input_dir: PathBuf::from("."),
            sigil: '%',
            no_macros: false,
            include: ".".to_string(),
            db: root.db(),
            dump_expanded: false,
            directory: None,
            ext: vec!["adoc".to_string()],
            gen_dir: root.gen_dir(),
            open_delim: "<[".to_string(),
            close_delim: "]>".to_string(),
            chunk_end: "@".to_string(),
            comment_markers: "#,//".to_string(),
            formatter: vec![],
            depfile: None,
            stamp: None,
            no_fts: false,
            allow_env: false,
            allow_home: false,
            strict: false,
            dry_run: false,
            warn_unused: false,
        }
    }

    #[test]
    fn test_bin_run_single_pass() {
        let mut ws = TestWorkspace::new();
        let adoc = ws.root.join("test.adoc");
        // Ensure no spaces between comment and delimiter to avoid regex ambiguity
        // AND add the missing '=' suffix required for chunk definitions.
        std::fs::write(&adoc, "= Test\n\n[source,rust]\n----\n//<[@file test.rs]>=\nfn main() {}\n// @\n----\n").unwrap();

        let mut single = default_single_pass(&ws);
        single.directory = Some(ws.root.clone());

        println!("Running single pass on {:?}", ws.root);
        run_single_pass_from_cli(single, false).expect("single pass failed");

        let out = ws.gen_file("test.rs");
        println!("Checking output path: {:?}", out);
        if !out.exists() {
            if let Ok(entries) = std::fs::read_dir(&ws.root) {
                for entry in entries {
                    println!("In root: {:?}", entry.unwrap().path());
                }
            }
            if let Ok(entries) = std::fs::read_dir(ws.gen_dir()) {
                for entry in entries {
                    println!("In gen: {:?}", entry.unwrap().path());
                }
            }
        }
        assert!(out.exists(), "Output file test.rs should exist in gen dir");
        assert!(ws.db().exists(), "Database should exist");
    }

    #[test]
    fn test_bin_run_multi_pass_error() {
        let ws = TestWorkspace::new();
        let config = ws.root.join("weaveback.toml");
        // Missing config should error
        let res = run_multi_pass(&config, false);
        assert!(res.is_err());
    }

    #[test]
    fn test_bin_run_apply_back() {
        let mut ws = TestWorkspace::new();
        let mut db = ws.open_db();
        db.set_chunk_defs(&[weaveback_tangle::db::ChunkDefEntry {
            src_file: "test.adoc".to_string(),
            chunk_name: "@file test.rs".to_string(),
            nth: 0,
            def_start: 5,
            def_end: 7,
        }]).unwrap();
        db.set_baseline("test.rs", b"// <[@file test.rs]>\nfn old() {}\n// @\n").unwrap();

        let gen_file = ws.gen_file("test.rs");
        std::fs::create_dir_all(gen_file.parent().unwrap()).unwrap();
        std::fs::write(&gen_file, "// <[@file test.rs]>\nfn new() {}\n// @\n").unwrap();

        let single = default_single_pass(&ws);
        let res = run_apply_back(vec!["test.rs".to_string()], false, &single);
        let _ = res;
    }
}
