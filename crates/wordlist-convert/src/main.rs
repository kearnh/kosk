use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use wordlist_convert::{convert, write_tsv, ConvertOpts, Format};

#[derive(Parser, Debug)]
#[command(name = "wordlist-convert")]
struct Args {
    #[arg(long)]
    format: String,
    #[arg(long = "in")]
    inputs: Vec<PathBuf>,
    #[arg(long)]
    exclude: Option<PathBuf>,
    #[arg(long, default_value_t = 0)]
    max_words: usize,
    #[arg(long, default_value_t = 3)]
    min_len: usize,
    #[arg(long)]
    out: PathBuf,
}

fn main() -> Result<()> {
    let args = Args::parse();
    if args.inputs.is_empty() {
        anyhow::bail!("at least one --in FILE is required");
    }
    let rows = convert(ConvertOpts {
        format: Format::parse(&args.format)?,
        inputs: &args.inputs,
        exclude: args.exclude.as_deref(),
        max_words: args.max_words,
        min_len: args.min_len,
    })?;
    write_tsv(&args.out, &rows)?;
    Ok(())
}
