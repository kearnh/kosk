use std::sync::Arc;

use anyhow::Result;
use std::sync::atomic::{AtomicU64, Ordering};

use super::context::CompletionContext;
use super::dictionary::DictionaryEngine;
use super::settings::{CompletionBackendKind, CompletionConfig};
use super::user_cache::UserCache;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Unigram,
    Ngram,
    UserCache,
    Dictionary,
    CurrentWord,
    OriginalText,
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

    fn knows_word(&self, ctx: &CompletionContext) -> bool;

    /// Live engines persist accepted words. Playback does not.
    fn writes_user_cache(&self) -> bool {
        true
    }
}

pub fn backend_from_config(
    cfg: &CompletionConfig,
    config_dir: Option<&std::path::Path>,
    user: Arc<std::sync::Mutex<UserCache>>,
) -> Result<BackendLoad> {
    let wants_next_word =
        cfg.enabled && cfg.suggest_next_word && cfg.backend == CompletionBackendKind::Ngram;
    match try_backend(cfg.backend, cfg, config_dir, Arc::clone(&user)) {
        Ok(loaded) => Ok(BackendLoad {
            next_word: if wants_next_word && !loaded.has_tables {
                NextWordTables::Missing
            } else {
                NextWordTables::Ready
            },
            backend: loaded.backend,
        }),
        Err(e) => {
            if cfg.fallback != cfg.backend {
                eprintln!(
                    "completion: {} backend failed ({e}); falling back to {:?}",
                    backend_name(cfg.backend),
                    cfg.fallback
                );
                Ok(BackendLoad {
                    backend: try_backend(cfg.fallback, cfg, config_dir, user)?.backend,
                    next_word: if wants_next_word {
                        NextWordTables::Missing
                    } else {
                        NextWordTables::Ready
                    },
                })
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

/// Whether the optional next-word pair tables are available.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NextWordTables {
    Ready,
    Missing,
}

pub struct BackendLoad {
    pub backend: Arc<dyn CompletionBackend>,
    pub next_word: NextWordTables,
}

struct Loaded {
    backend: Arc<dyn CompletionBackend>,
    has_tables: bool,
}

fn try_backend(
    kind: CompletionBackendKind,
    cfg: &CompletionConfig,
    config_dir: Option<&std::path::Path>,
    user: Arc<std::sync::Mutex<UserCache>>,
) -> Result<Loaded> {
    match kind {
        CompletionBackendKind::Dictionary => {
            let path = resolve(config_dir, &cfg.dictionary.wordlist);
            let mut engine =
                super::dictionary::DictionaryEngine::from_path_or_embedded(&path, cfg)?;
            engine.set_overlay(load_type_wordlists(cfg, config_dir), Some(user));
            Ok(Loaded {
                backend: Arc::new(engine),
                has_tables: false,
            })
        }
        CompletionBackendKind::Ngram => {
            let dir = resolve(config_dir, &cfg.ngram.model_dir);
            let extras = load_type_wordlists(cfg, config_dir);
            match super::ngram::NgramEngine::load(&dir, cfg, Arc::clone(&user)) {
                Ok(mut eng) => {
                    let has_tables = eng.has_bigrams();
                    eng.set_overlay(extras);
                    Ok(Loaded {
                        backend: Arc::new(eng),
                        has_tables,
                    })
                }
                Err(e) => {
                    let wordlist = resolve(config_dir, &cfg.dictionary.wordlist);
                    let dict =
                        super::dictionary::DictionaryEngine::from_path_or_embedded(&wordlist, cfg)?;
                    let mut wrapped =
                        super::ngram::NgramEngine::from_dictionary(dict, cfg, Some(user));
                    wrapped.set_overlay(extras);
                    eprintln!(
                        "completion: ngram model at {} not loaded ({e}); unigram-only",
                        dir.display()
                    );
                    Ok(Loaded {
                        backend: Arc::new(wrapped),
                        has_tables: false,
                    })
                }
            }
        }
    }
}

fn resolve(dir: Option<&std::path::Path>, path: &std::path::Path) -> std::path::PathBuf {
    if path.is_absolute() {
        return path.to_path_buf();
    }
    match dir {
        Some(dir) => dir.join(path),
        None => path.to_path_buf(),
    }
}

pub fn load_type_wordlists(
    cfg: &CompletionConfig,
    config_dir: Option<&std::path::Path>,
) -> std::collections::HashMap<String, DictionaryEngine> {
    let mut extras = std::collections::HashMap::new();
    for (name, t) in &cfg.app_types {
        let Some(rel) = t.wordlist.as_ref() else {
            continue;
        };
        let path = resolve(config_dir, rel);
        if !path.exists() {
            eprintln!(
                "completion: app type '{name}' wordlist {} missing",
                path.display()
            );
            crate::user_notify::notify(crate::user_notify::Notice::wordlist_missing(name));
            continue;
        }
        match DictionaryEngine::from_path_or_embedded(&path, cfg) {
            Ok(eng) => {
                extras.insert(name.clone(), eng);
            }
            Err(e) => {
                eprintln!(
                    "completion: app type '{name}' wordlist {} failed ({e})",
                    path.display()
                );
                crate::user_notify::notify(crate::user_notify::Notice::wordlist_failed(name));
            }
        }
    }
    extras
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn unknown_backend_is_compile_time_enum() {
        let cfg = CompletionConfig::default();
        assert_eq!(cfg.backend, CompletionBackendKind::Ngram);
    }

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "kosk-nextword-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn user(cfg: &CompletionConfig, dir: &std::path::Path) -> Arc<std::sync::Mutex<UserCache>> {
        Arc::new(Mutex::new(UserCache::load(
            &cfg.user_cache,
            dir.join("cache.bin"),
        )))
    }

    #[test]
    fn missing_tables_report_missing() {
        let dir = temp_dir("missing");
        let cfg = CompletionConfig::default();
        let load = backend_from_config(&cfg, Some(&dir), user(&cfg, &dir)).unwrap();
        assert_eq!(load.next_word, NextWordTables::Missing);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn packed_bigrams_report_ready() {
        let dir = temp_dir("ready");
        std::fs::write(dir.join("vocab.txt"), "hello\nworld\n").unwrap();
        super::super::ngram::write_unigrams(&dir.join("unigrams.bin"), &[3, 1]).unwrap();
        super::super::ngram::write_count_table(
            &dir.join("bigrams.bin"),
            &[(super::super::ngram::pack2(0, 1), 5)],
        )
        .unwrap();

        let mut cfg = CompletionConfig::default();
        cfg.ngram.model_dir = dir.clone();
        let load = backend_from_config(&cfg, None, user(&cfg, &dir)).unwrap();
        assert_eq!(load.next_word, NextWordTables::Ready);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
