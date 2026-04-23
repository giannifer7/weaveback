# Command-Line Interface

`main.rs` is the entry point for the `weaveback-tangle` binary.  It parses
command-line arguments with `clap`, builds a
[`Clip`](noweb.adoc), reads the input files, writes all `@file` chunks to
disk via [`SafeFileWriter`](safe_writer.adoc), and merges the run's database
into the path given by `--db` (default `weaveback.db`).

See [weaveback_tangle.adoc](weaveback_tangle.adoc) for the module map and
[architecture.adoc](../../../docs/architecture.adoc) for the pipeline
overview.

## Arguments

<table>
  <tr><td>Flag</td><td>Default</td><td>Description</td></tr>
  <tr><td>`--gen DIR`</td><td>`gen`</td><td>Base directory for generated output files.</td></tr>
  <tr><td>`--open-delim STR`</td><td>`&lt;[`</td><td>Chunk open delimiter.</td></tr>
  <tr><td>`--close-delim STR`</td><td>`]&gt;`</td><td>Chunk close delimiter.</td></tr>
  <tr><td>`--chunk-end STR`</td><td>`@`</td><td>Marker that closes a chunk definition.</td></tr>
  <tr><td>`--comment-markers LIST`</td><td>`#,//`</td><td>Comma-separated comment prefixes.</td></tr>
  <tr><td>`--formatter EXT=CMD`</td><td>—</td><td>Formatter to run on files with a given extension.</td></tr>
  <tr><td>`--allow-home`</td><td>off</td><td>Allow `@file ~/…` chunks to write outside `gen/`.</td></tr>
  <tr><td>`--strict`</td><td>off</td><td>Treat undefined chunk references as fatal errors (default: expand to nothing).</td></tr>
  <tr><td>`--dry-run`</td><td>off</td><td>Print output paths without writing anything.</td></tr>
  <tr><td>`--db FILE`</td><td>`weaveback.db`</td><td>Path to the persistent source-map database.</td></tr>
  <tr><td>`--chunks LIST`</td><td>—</td><td>Named chunks to extract to stdout (or `--output`).</td></tr>
  <tr><td>`--output FILE`</td><td>stdout</td><td>Destination for `--chunks` output.</td></tr>
  <tr><td>`FILES…`</td><td>—</td><td>Input literate source files (`-` for stdin).</td></tr>
</table>

```rust
// <[@file weaveback-tangle/src/main.rs]>=
// weaveback-tangle/src/main.rs
// I'd Really Rather You Didn't edit this generated file.

use weaveback_tangle::{WeavebackError, Clip, SafeFileWriter, SafeWriterConfig};
use clap::Parser;
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Write};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "weaveback",
    about = "Expand chunks like noweb - A literate programming tool",
    version
)]
struct Args {
    /// Output file for --chunks [default: stdout]
    #[arg(long)]
    output: Option<PathBuf>,

    /// Names of chunks to extract (comma separated)
    #[arg(long)]
    chunks: Option<String>,

    /// Base directory of generated files
    #[arg(long = "gen", default_value = "gen")]
    gen_dir: PathBuf,

    /// Delimiter used to open a chunk
    #[arg(long, default_value = "<[")]
    open_delim: String,

    /// Delimiter used to close a chunk definition
    #[arg(long, default_value = "]>")]
    close_delim: String,

    /// Delimiter for chunk-end lines
    #[arg(long, default_value = "@")]
    chunk_end: String,

    /// Comment markers (comma separated)
    #[arg(long, default_value = "#,//")]
    comment_markers: String,

    /// Formatter command per file extension, e.g. --formatter rs=rustfmt
    /// Can be repeated: --formatter rs=rustfmt --formatter ts="prettier --write"
    #[arg(long, value_name = "EXT=CMD")]
    formatter: Vec<String>,

    /// Allow @file ~/... chunks to write outside the gen/ directory
    #[arg(long)]
    allow_home: bool,

    /// Overwrite generated files even if they differ from the stored baseline.
    /// Use this only when the literate source is the authoritative state.
    #[arg(long)]
    force_generated: bool,

    /// Treat references to undefined chunks as fatal errors
    #[arg(long)]
    strict: bool,

    /// Show what would be written without writing anything
    #[arg(long)]
    dry_run: bool,

    /// Path to the persistent source-map database
    #[arg(long, default_value = "weaveback.db")]
    db: PathBuf,

    /// Input files (use - for stdin)
    #[arg(required = true)]
    files: Vec<PathBuf>,
}

fn write_chunks<W: Write>(
    clipper: &mut Clip,
    chunks: &[&str],
    writer: &mut W,
) -> Result<(), WeavebackError> {
    for chunk in chunks {
        clipper.get_chunk(chunk, writer)?;
        writeln!(writer)?;
    }
    Ok(())
}

fn run(args: Args) -> Result<(), WeavebackError> {
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
            force_generated: args.force_generated,
            ..SafeWriterConfig::default()
        },
    )?;
    let mut clipper = Clip::new(
        safe_writer,
        &args.open_delim,
        &args.close_delim,
        &args.chunk_end,
        &comment_markers,
    );

    clipper.set_strict_undefined(args.strict);
    clipper.read_files(&args.files)?;

    if args.dry_run {
        for path in clipper.list_output_files() {
            println!("{}", path.display());
        }
        return Ok(());
    }

    clipper.write_files()?;

    if let Some(chunks) = args.chunks {
        let chunks: Vec<&str> = chunks.split(',').collect();
        if let Some(output_path) = args.output {
            let mut file = File::create(output_path)?;
            write_chunks(&mut clipper, &chunks, &mut file)?;
        } else {
            let stdout = io::stdout();
            let mut handle = stdout.lock();
            write_chunks(&mut clipper, &chunks, &mut handle)?;
        }
    }

    clipper.finish(&args.db)?;

    Ok(())
}

fn main() {
    env_logger::init();
    let args = Args::parse();

    if let Err(e) = run(args) {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

// @
```


## Tests

Integration tests for the `weaveback-tangle` CLI binary.  They use
`assert_cmd` to invoke the built binary as a subprocess and exercise the
full argument-parsing / chunk-extraction contract:

* `test_no_arguments_fails` — no args → non-zero exit with "required"
* `test_basic_chunk_extraction` — `@file` chunk written to `gen/` directory
* `test_extract_specific_chunk_to_stdout` — `--chunks` prints named chunk
* `test_extract_chunk_to_file` — `--chunks --output` writes chunk to file

```rust
// <[@file weaveback-tangle/tests/main_tests.rs]>=
// weaveback-tangle/tests/main_tests.rs
// I'd Really Rather You Didn't edit this generated file.

use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::fs;
use std::io::Write;
use std::process::Command;
use tempfile::tempdir;

#[test]
fn test_no_arguments_fails() -> Result<(), Box<dyn std::error::Error>> {
    // Running the binary with no arguments should fail and print usage or an error.
    let mut cmd = Command::cargo_bin("weaveback-tangle")?;
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("required"));

    Ok(())
}

#[test]
fn test_basic_chunk_extraction() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let input_file = dir.path().join("input.nw");
    let mut file = fs::File::create(&input_file)?;
    writeln!(file, "<[@file test.txt]>=")?;
    writeln!(file, "Hello, world!")?;
    writeln!(file, "@")?;

    let gen_dir = dir.path().join("gen");
    fs::create_dir_all(&gen_dir)?;

    let mut cmd = Command::cargo_bin("weaveback-tangle")?;
    cmd.arg("--gen")
        .arg(&gen_dir)
        .arg(&input_file)
        .current_dir(dir.path());

    cmd.assert().success();

    let output_path = gen_dir.join("test.txt");
    let output_content = fs::read_to_string(output_path)?;
    assert_eq!(output_content, "Hello, world!\n");

    Ok(())
}

#[test]
fn test_extract_specific_chunk_to_stdout() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let input_file = dir.path().join("input.nw");
    let mut file = fs::File::create(&input_file)?;
    writeln!(file, "<[chunk1]>=")?;
    writeln!(file, "Chunk 1 content")?;
    writeln!(file, "@")?;
    writeln!(file, "<[chunk2]>=")?;
    writeln!(file, "Chunk 2 content")?;
    writeln!(file, "@")?;

    let gen_dir = dir.path().join("gen");
    fs::create_dir_all(&gen_dir)?;

    let mut cmd = Command::cargo_bin("weaveback-tangle")?;
    cmd.arg("--gen")
        .arg(&gen_dir)
        .arg("--chunks")
        .arg("chunk2")
        .arg(&input_file)
        .current_dir(dir.path());

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Chunk 2 content"));

    Ok(())
}

#[test]
fn test_extract_chunk_to_file() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let input_file = dir.path().join("input.nw");
    {
        let mut file = fs::File::create(&input_file)?;
        writeln!(file, "<[chunk3]>=")?;
        writeln!(file, "This is chunk 3.")?;
        writeln!(file, "@")?;
    }

    let output_file = dir.path().join("chunk3_output.txt");
    let gen_dir = dir.path().join("gen");
    fs::create_dir_all(&gen_dir)?;

    let mut cmd = Command::cargo_bin("weaveback-tangle")?;
    cmd.arg("--gen")
        .arg(&gen_dir)
        .arg("--chunks")
        .arg("chunk3")
        .arg("--output")
        .arg(&output_file)
        .arg(&input_file)
        .current_dir(dir.path());

    cmd.assert().success();

    let content = fs::read_to_string(&output_file)?;
    assert!(content.contains("This is chunk 3."));

    Ok(())
}

// @
```

