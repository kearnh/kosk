use anyhow::{Context, Result};
use clap::Parser;
use kosk::completion::{split_at_cursor, CompletionContext, CompletionEngine, DictionaryEngine};

#[derive(Parser, Debug)]
#[command(name = "completion_dev")]
struct Args {
    #[arg(long)]
    text: String,
    #[arg(long)]
    cursor: usize,
    #[arg(long, default_value_t = 8)]
    max: usize,
    #[arg(long)]
    wordlist: Option<std::path::PathBuf>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let engine = if let Some(path) = &args.wordlist {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("read wordlist {}", path.display()))?;
        DictionaryEngine::from_wordlist_text(&raw)
    } else {
        DictionaryEngine::embedded_demo()
    };

    let (prefix, suffix) = split_at_cursor(&args.text, args.cursor)
        .context("cursor must be on a UTF-8 character boundary and <= text length")?;
    let ctx = CompletionContext::new(prefix, suffix, args.max);
    let sugs = engine.suggest(&ctx);
    if sugs.is_empty() {
        println!("(no suggestions)");
    } else {
        for (i, s) in sugs.iter().enumerate() {
            println!("{}. {}", i + 1, s.text);
        }
    }
    Ok(())
}
