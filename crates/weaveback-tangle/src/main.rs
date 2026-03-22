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

    clipper.read_files(&args.files)?;
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

    clipper.finish(std::path::Path::new("weaveback.db"))?;

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
