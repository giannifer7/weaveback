// wb-tangle/src/main.rs
// I'd Really Rather You Didn't edit this generated file.

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
        macro_prelude:   s.macro_prelude,
        expanded_ext:    s.expanded_ext,
        expanded_adoc_dir: s.expanded_adoc_dir,
        expanded_md_dir: s.expanded_md_dir,
        macro_only:      s.macro_only,
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
mod tests;

