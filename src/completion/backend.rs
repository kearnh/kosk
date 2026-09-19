use std::sync::Arc;

use anyhow::Result;
use std::sync::atomic::{AtomicU64, Ordering};

use super::context::CompletionContext;
use super::settings::{CompletionBackendKind, CompletionConfig};
use super::user_cache::UserCache;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Unigram,
    Ngram,
    UserCache,
    Dictionary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MatchKind {
    #[default]
    ExactPrefix,
    Correction,
    Fuzzy,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub text: String,
    pub score: f32,
    pub source: Source,
    pub kind: MatchKind,
}

pub struct Abort<'a> {
    pub mine: u64,
    pub current: &'a AtomicU64,
}

impl Abort<'_> {
    #[inline]
    pub fn stale(&self) -> bool {
        self.current.load(Ordering::Relaxed) != self.mine
    }
}

pub trait CompletionBackend: Send + Sync {
    fn suggest(&self, ctx: &CompletionContext, abort: &Abort<'_>) -> Option<Vec<Candidate>>;
}

pub fn backend_from_config(
    cfg: &CompletionConfig,
    config_dir: Option<&std::path::Path>,
    user: Arc<std::sync::Mutex<UserCache>>,
) -> Result<Arc<dyn CompletionBackend>> {
    match try_backend(cfg.backend, cfg, config_dir, Arc::clone(&user)) {
        Ok(b) => Ok(b),
        Err(e) => {
            if cfg.fallback != cfg.backend {
                eprintln!(
                    "completion: {} backend failed ({e}); falling back to {:?}",
                    backend_name(cfg.backend),
                    cfg.fallback
                );
                try_backend(cfg.fallback, cfg, config_dir, user)
            } else {
                Err(e)
            }
        }
    }
}

fn backend_name(kind: CompletionBackendKind) -> &'static str {
    match kind {
        CompletionBackendKind::Ngram => "ngram",
        CompletionBackendKind::Dictionary => "dictionary",
    }
}

fn try_backend(
    kind: CompletionBackendKind,
    cfg: &CompletionConfig,
    config_dir: Option<&std::path::Path>,
    user: Arc<std::sync::Mutex<UserCache>>,
) -> Result<Arc<dyn CompletionBackend>> {
    match kind {
        CompletionBackendKind::Dictionary => {
            let path = resolve(config_dir, &cfg.dictionary.wordlist);
            let engine = super::dictionary::DictionaryEngine::from_path_or_embedded(&path, cfg)?;
            Ok(Arc::new(engine))
        }
        CompletionBackendKind::Ngram => {
            let dir = resolve(config_dir, &cfg.ngram.model_dir);
            match super::ngram::NgramEngine::load(&dir, cfg, Arc::clone(&user)) {
                Ok(eng) => Ok(Arc::new(eng)),
                Err(e) => {
                    let wordlist = resolve(config_dir, &cfg.dictionary.wordlist);
                    let dict =
                        super::dictionary::DictionaryEngine::from_path_or_embedded(&wordlist, cfg)?;
                    let wrapped = super::ngram::NgramEngine::from_dictionary(dict, cfg, Some(user));
                    eprintln!(
                        "completion: ngram model at {} not loaded ({e}); unigram-only",
                        dir.display()
                    );
                    Ok(Arc::new(wrapped))
                }
            }
        }
    }
}

fn resolve(dir: Option<&std::path::Path>, path: &std::path::Path) -> std::path::PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else if let Some(d) = dir {
        d.join(path)
    } else {
        path.to_path_buf()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_backend_is_compile_time_enum() {
        let cfg = CompletionConfig::default();
        assert_eq!(cfg.backend, CompletionBackendKind::Ngram);
    }
}
