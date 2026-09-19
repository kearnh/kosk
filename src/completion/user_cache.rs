use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::settings::CompletionUserCacheConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Persist {
    unigrams: Vec<(String, f32, u64)>,
    bigrams: Vec<(String, String, f32, u64)>,
}

#[derive(Debug, Clone)]
struct Timed {
    count: f32,
    last: Instant,
}

pub struct UserCache {
    unigrams: HashMap<String, Timed>,
    bigrams: HashMap<(String, String), Timed>,
    cfg: CompletionUserCacheConfig,
    path: PathBuf,
    last_persist: Instant,
}

impl UserCache {
    pub fn load(cfg: &CompletionUserCacheConfig, path: PathBuf) -> Self {
        let mut cache = Self {
            unigrams: HashMap::new(),
            bigrams: HashMap::new(),
            cfg: cfg.clone(),
            path,
            last_persist: Instant::now(),
        };
        if cfg.enabled {
            let _ = cache.read_disk();
        }
        cache
    }

    fn read_disk(&mut self) -> Result<()> {
        if !self.path.exists() {
            return Ok(());
        }
        let bytes =
            std::fs::read(&self.path).with_context(|| format!("read {}", self.path.display()))?;
        let persist: Persist = postcard::from_bytes(&bytes).context("parse user cache")?;
        let now = Instant::now();
        for (w, c, unix) in persist.unigrams {
            self.unigrams.insert(
                w,
                Timed {
                    count: c,
                    last: instant_from_unix(unix, now),
                },
            );
        }
        for (a, b, c, unix) in persist.bigrams {
            self.bigrams.insert(
                (a, b),
                Timed {
                    count: c,
                    last: instant_from_unix(unix, now),
                },
            );
        }
        Ok(())
    }

    pub fn learn_words(&mut self, words: &[String]) {
        if !self.cfg.enabled || words.is_empty() {
            return;
        }
        for w in words {
            self.bump_uni(w);
        }
        for pair in words.windows(2) {
            self.bump_bi(&pair[0], &pair[1]);
        }
        self.trim();
        self.maybe_persist();
    }

    fn bump_uni(&mut self, w: &str) {
        let now = Instant::now();
        let tau = self.cfg.decay_tau_hours;
        let e = self.unigrams.entry(w.to_string()).or_insert(Timed {
            count: 0.0,
            last: now,
        });
        e.count = decay(e.count, e.last, tau) + 1.0;
        e.last = now;
    }

    fn bump_bi(&mut self, a: &str, b: &str) {
        let now = Instant::now();
        let tau = self.cfg.decay_tau_hours;
        let e = self
            .bigrams
            .entry((a.to_string(), b.to_string()))
            .or_insert(Timed {
                count: 0.0,
                last: now,
            });
        e.count = decay(e.count, e.last, tau) + 1.0;
        e.last = now;
    }

    fn decay(&self, count: f32, last: Instant) -> f32 {
        decay(count, last, self.cfg.decay_tau_hours)
    }

    pub fn unigram(&self, w: &str) -> f32 {
        self.unigrams
            .get(w)
            .map(|t| self.decay(t.count, t.last))
            .unwrap_or(0.0)
    }

    pub fn bigram(&self, a: &str, b: &str) -> f32 {
        self.bigrams
            .get(&(a.to_string(), b.to_string()))
            .map(|t| self.decay(t.count, t.last))
            .unwrap_or(0.0)
    }

    pub fn continuations(&self, prev: &str) -> Vec<(String, f32)> {
        let mut out: Vec<(String, f32)> = self
            .bigrams
            .iter()
            .filter(|((a, _), _)| a == prev)
            .map(|((_, b), t)| (b.clone(), self.decay(t.count, t.last)))
            .filter(|(_, c)| *c > 0.0)
            .collect();
        out.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        out
    }

    fn trim(&mut self) {
        trim_map(&mut self.unigrams, self.cfg.max_unigrams);
        trim_map(&mut self.bigrams, self.cfg.max_bigrams);
    }

    fn maybe_persist(&mut self) {
        if self.last_persist.elapsed().as_secs() < self.cfg.persist_interval_s {
            return;
        }
        let _ = self.persist();
    }

    pub fn persist(&mut self) -> Result<()> {
        if !self.cfg.enabled {
            return Ok(());
        }
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let now = Instant::now();
        let persist = Persist {
            unigrams: self
                .unigrams
                .iter()
                .map(|(w, t)| (w.clone(), t.count, unix_from_instant(t.last, now)))
                .collect(),
            bigrams: self
                .bigrams
                .iter()
                .map(|((a, b), t)| {
                    (
                        a.clone(),
                        b.clone(),
                        t.count,
                        unix_from_instant(t.last, now),
                    )
                })
                .collect(),
        };
        let bytes = postcard::to_stdvec(&persist).context("serialize user cache")?;
        let tmp = self.path.with_extension("bin.tmp");
        std::fs::write(&tmp, &bytes)?;
        std::fs::rename(&tmp, &self.path)?;
        self.last_persist = Instant::now();
        Ok(())
    }
}

fn decay(count: f32, last: Instant, tau_hours: f32) -> f32 {
    let tau = Duration::from_secs_f32(tau_hours.max(0.01) * 3600.0);
    let dt = last.elapsed();
    let steps = dt.as_secs_f32() / tau.as_secs_f32();
    count * 0.5f32.powf(steps)
}

fn trim_map<K: Eq + std::hash::Hash>(map: &mut HashMap<K, Timed>, max: usize) {
    if map.len() <= max {
        return;
    }
    let mut items: Vec<(K, Timed)> = map.drain().collect();
    items.sort_by(|a, b| b.1.count.total_cmp(&a.1.count));
    items.truncate(max);
    *map = items.into_iter().collect();
}

fn instant_from_unix(unix: u64, now: Instant) -> Instant {
    let now_unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    if unix >= now_unix {
        now
    } else {
        now.checked_sub(Duration::from_secs(now_unix - unix))
            .unwrap_or(now)
    }
}

fn unix_from_instant(last: Instant, now: Instant) -> u64 {
    let now_unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let ago = now.saturating_duration_since(last).as_secs();
    now_unix.saturating_sub(ago)
}

pub fn resolve_cache_path(config_dir: Option<&Path>, rel: &Path) -> std::path::PathBuf {
    if rel.is_absolute() {
        rel.to_path_buf()
    } else if let Some(d) = config_dir {
        d.join(rel)
    } else {
        rel.to_path_buf()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::completion::settings::CompletionUserCacheConfig;

    #[test]
    fn learn_boosts() {
        let cfg = CompletionUserCacheConfig::default();
        let mut c = UserCache::load(&cfg, PathBuf::from("target/kosk-test-cache.bin"));
        c.unigrams.clear();
        c.learn_words(&["foo".into(), "bar".into()]);
        assert!(c.unigram("foo") > 0.0);
        assert!(c.bigram("foo", "bar") > 0.0);
    }

    #[test]
    fn continuations_lists_learned_pair() {
        let cfg = CompletionUserCacheConfig::default();
        let mut c = UserCache::load(&cfg, PathBuf::from("target/kosk-test-cache-cont.bin"));
        c.unigrams.clear();
        c.bigrams.clear();
        c.learn_words(&["cat".into(), "sat".into()]);
        let next = c.continuations("cat");
        assert_eq!(next[0].0, "sat");
        assert!(c.continuations("the").is_empty());
    }
}
