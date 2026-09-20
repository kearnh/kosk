use anyhow::{Context, Result};
use clap::Parser;
use kosk::completion::settings::CompletionConfig;
use kosk::completion::{
    load_type_wordlists, AppTypeMap, CompletionBackend, CompletionContext, DictionaryEngine,
    NgramEngine,
};
use std::sync::atomic::AtomicU64;
use std::sync::Arc;

#[derive(Parser, Debug)]
#[command(name = "completion_dev")]
struct Args {
    #[arg(long)]
    text: Option<String>,
    #[arg(long)]
    cursor: Option<usize>,
    #[arg(long, default_value_t = 8)]
    max: usize,
    #[arg(long)]
    wordlist: Option<std::path::PathBuf>,
    #[arg(long, default_value = "dictionary")]
    backend: String,
    #[arg(long)]
    model_dir: Option<std::path::PathBuf>,
    #[arg(long)]
    eval: Option<std::path::PathBuf>,
    /// Foreground exe basename; mapped through `[completion.app_types]` (default catch-all).
    #[arg(long)]
    exe: Option<String>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let mut cfg = load_completion_cfg();
    cfg.max_suggestions = args.max;
    cfg.suggest_next_word = true;

    let dummy = Arc::new(std::sync::Mutex::new(kosk::completion::UserCache::load(
        &cfg.user_cache,
        std::path::PathBuf::from("target/completion-dev-cache.bin"),
    )));
    let extras = load_type_wordlists(&cfg, Some(std::path::Path::new(".")));

    let engine: Box<dyn CompletionBackend> = match args.backend.as_str() {
        "ngram" => {
            let dir = args
                .model_dir
                .unwrap_or_else(|| std::path::PathBuf::from("data/completion/en"));
            match NgramEngine::load(&dir, &cfg, Arc::clone(&dummy)) {
                Ok(mut e) => {
                    e.set_overlay(extras);
                    Box::new(e)
                }
                Err(_) => {
                    let dict = load_dict(&args.wordlist, &cfg)?;
                    let mut wrapped = NgramEngine::from_dictionary(dict, &cfg, Some(dummy));
                    wrapped.set_overlay(extras);
                    Box::new(wrapped)
                }
            }
        }
        _ => {
            let mut dict = load_dict(&args.wordlist, &cfg)?;
            dict.set_overlay(extras, Some(dummy));
            Box::new(dict)
        }
    };

    if let Some(eval_path) = args.eval {
        return run_eval(engine.as_ref(), &eval_path, &cfg, args.exe.as_deref());
    }

    let text = args
        .text
        .ok_or_else(|| anyhow::anyhow!("--text is required unless --eval is set"))?;
    let cursor = args
        .cursor
        .ok_or_else(|| anyhow::anyhow!("--cursor is required unless --eval is set"))?;
    let mut ctx = CompletionContext::from_buffer(&text, cursor, &cfg)
        .context("cursor must be on a UTF-8 character boundary and <= text length")?;
    stamp_app_type(&mut ctx, &cfg, args.exe.as_deref());
    let gen = AtomicU64::new(1);
    let abort = kosk::completion::Abort {
        mine: 1,
        current: &gen,
    };
    let sugs = engine.suggest(&ctx, &abort).unwrap_or_default();
    if sugs.is_empty() {
        println!("(no suggestions)");
    } else {
        for (i, s) in sugs.iter().enumerate() {
            println!("{}. {}  ({:?} {:.3})", i + 1, s.text, s.source, s.score);
        }
    }
    Ok(())
}

fn load_completion_cfg() -> CompletionConfig {
    let Ok(raw) = std::fs::read_to_string("config.toml") else {
        return CompletionConfig::default();
    };
    let Ok(val) = raw.parse::<toml::Value>() else {
        return CompletionConfig::default();
    };
    let Some(comp) = val.get("completion") else {
        return CompletionConfig::default();
    };
    comp.clone()
        .try_into::<CompletionConfig>()
        .unwrap_or_default()
}

fn stamp_app_type(ctx: &mut CompletionContext, cfg: &CompletionConfig, exe: Option<&str>) {
    let types = AppTypeMap::from_config(&cfg.app_types).unwrap_or_default();
    ctx.app_type = types.resolve(exe);
}

fn load_dict(
    wordlist: &Option<std::path::PathBuf>,
    cfg: &CompletionConfig,
) -> Result<DictionaryEngine> {
    if let Some(path) = wordlist {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("read wordlist {}", path.display()))?;
        Ok(DictionaryEngine::from_wordlist_text(&raw, cfg))
    } else {
        Ok(DictionaryEngine::embedded_demo(cfg))
    }
}

fn run_eval(
    engine: &dyn CompletionBackend,
    path: &std::path::Path,
    cfg: &CompletionConfig,
    exe: Option<&str>,
) -> Result<()> {
    let text = std::fs::read_to_string(path)?;
    let chars: Vec<char> = text.chars().filter(|c| *c != '\r').collect();
    let produced = chars.iter().filter(|c| **c != '\n').count() as u64;
    let mut typed = String::new();
    let mut keystrokes = 0u64;
    let mut top1 = 0u64;
    let mut top3 = 0u64;
    let mut next_n = 0u64;
    let gen = AtomicU64::new(1);
    let abort = kosk::completion::Abort {
        mine: 1,
        current: &gen,
    };

    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\n' {
            typed.clear();
            i += 1;
            continue;
        }
        let ctx = CompletionContext::from_buffer(&typed, typed.len(), cfg);
        if let Some(mut ctx) = ctx {
            stamp_app_type(&mut ctx, cfg, exe);
            if let Some(sugs) = engine.suggest(&ctx, &abort) {
                let rest: String = chars[i..].iter().collect();
                let rest_word: String = rest
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '\'' || *c == '_')
                    .collect();
                let upcoming = format!("{}{rest_word}", ctx.token);
                if ctx.token.is_empty() && !upcoming.is_empty() {
                    next_n += 1;
                    if sugs
                        .first()
                        .is_some_and(|s| s.text.eq_ignore_ascii_case(&upcoming))
                    {
                        top1 += 1;
                    }
                    if sugs.iter().any(|s| s.text.eq_ignore_ascii_case(&upcoming)) {
                        top3 += 1;
                    }
                }
                if !rest_word.is_empty() {
                    if let Some(hit) = sugs.iter().find(|s| s.text.eq_ignore_ascii_case(&upcoming))
                    {
                        let rem = kosk::completion::remainder(&ctx.token, &hit.text);
                        if !rem.is_empty() {
                            typed.push_str(&rem);
                            i += rem.chars().count();
                            keystrokes += 1;
                            continue;
                        }
                    }
                }
            }
        }
        typed.push(chars[i]);
        keystrokes += 1;
        i += 1;
    }

    let ksr = if produced == 0 {
        0.0
    } else {
        1.0 - (keystrokes as f64 / produced as f64)
    };
    println!("chars={produced} keystrokes={keystrokes} ksr={ksr:.4}");
    if next_n > 0 {
        println!(
            "next-word top1={:.3} top3={:.3} n={next_n}",
            top1 as f64 / next_n as f64,
            top3 as f64 / next_n as f64
        );
    }
    Ok(())
}
