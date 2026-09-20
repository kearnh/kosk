use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::app_type::CATCH_ALL_TYPE;
use super::settings::CompletionUserCacheConfig;

const PERSIST_VERSION: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistV1 {
    unigrams: Vec<(String, f32, u64)>,
    bigrams: Vec<(String, String, f32, u64)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistBucket {
    unigrams: Vec<(String, f32, u64)>,
    bigrams: Vec<(String, String, f32, u64)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistV2 {
    version: u32,
    by_type: Vec<(String, PersistBucket)>,
}

#[derive(Debug, Clone)]
struct Timed {
    count: f32,
    last: Instant,
}

#[derive(Debug, Clone, Default)]
struct TypeMaps {
    unigrams: HashMap<String, Timed>,
    bigrams: HashMap<(String, String), Timed>,
}

pub struct UserCache {
    by_type: HashMap<String, TypeMaps>,
    cfg: CompletionUserCacheConfig,
    path: PathBuf,
}

impl UserCache {
    pub fn load(cfg: &CompletionUserCacheConfig, path: PathBuf) -> Self {
        let mut cache = Self {
            by_type: HashMap::new(),
            cfg: cfg.clone(),
            path,
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
        let now = Instant::now();
        if let Ok(v2) = postcard::from_bytes::<PersistV2>(&bytes) {
            if v2.version == PERSIST_VERSION {
                for (ty, bucket) in v2.by_type {
                    self.insert_bucket(&ty, bucket, now);
                }
                return Ok(());
            }
        }
        let v1: PersistV1 = postcard::from_bytes(&bytes).context("parse user cache")?;
        self.insert_bucket(
            CATCH_ALL_TYPE,
            PersistBucket {
                unigrams: v1.unigrams,
                bigrams: v1.bigrams,
            },
            now,
        );
        Ok(())
    }

    fn insert_bucket(&mut self, ty: &str, bucket: PersistBucket, now: Instant) {
        let maps = self.by_type.entry(ty.to_string()).or_default();
        for (w, c, unix) in bucket.unigrams {
            maps.unigrams.insert(
                w,
                Timed {
                    count: c,
                    last: instant_from_unix(unix, now),
                },
            );
        }
        for (a, b, c, unix) in bucket.bigrams {
            maps.bigrams.insert(
                (a, b),
                Timed {
                    count: c,
                    last: instant_from_unix(unix, now),
                },
            );
        }
    }

    fn maps_mut(&mut self, ty: &str) -> &mut TypeMaps {
        self.by_type.entry(ty.to_string()).or_default()
    }

    fn maps(&self, ty: &str) -> Option<&TypeMaps> {
        self.by_type.get(ty)
    }

    pub fn learn_words(&mut self, ty: &str, words: &[String]) {
        if !self.cfg.enabled || words.is_empty() {
            return;
        }
        for w in words {
            self.bump_uni(ty, w);
        }
        for pair in words.windows(2) {
            self.bump_bi(ty, &pair[0], &pair[1]);
        }
        self.trim(ty);
        let _ = self.persist();
    }

    fn bump_uni(&mut self, ty: &str, w: &str) {
        let now = Instant::now();
        let tau = self.cfg.decay_tau_hours;
        let e = self
            .maps_mut(ty)
            .unigrams
            .entry(w.to_string())
            .or_insert(Timed {
                count: 0.0,
                last: now,
            });
        e.count = decay(e.count, e.last, tau) + 1.0;
        e.last = now;
    }

    fn bump_bi(&mut self, ty: &str, a: &str, b: &str) {
        let now = Instant::now();
        let tau = self.cfg.decay_tau_hours;
        let e = self
            .maps_mut(ty)
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

    pub fn has_unigram(&self, ty: &str, w: &str) -> bool {
        self.maps(ty).is_some_and(|m| m.unigrams.contains_key(w))
    }

    pub fn unigram(&self, ty: &str, w: &str) -> f32 {
        self.maps(ty)
            .and_then(|m| m.unigrams.get(w))
            .map(|t| self.decay(t.count, t.last))
            .unwrap_or(0.0)
    }

    pub fn bigram(&self, ty: &str, a: &str, b: &str) -> f32 {
        self.maps(ty)
            .and_then(|m| m.bigrams.get(&(a.to_string(), b.to_string())))
            .map(|t| self.decay(t.count, t.last))
            .unwrap_or(0.0)
    }

    pub fn continuations(&self, ty: &str, prev: &str) -> Vec<(String, f32)> {
        let Some(maps) = self.maps(ty) else {
            return Vec::new();
        };
        let mut out: Vec<(String, f32)> = maps
            .bigrams
            .iter()
            .filter(|((a, _), _)| a == prev)
            .map(|((_, b), t)| (b.clone(), self.decay(t.count, t.last)))
            .filter(|(_, c)| *c > 0.0)
            .collect();
        out.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        out
    }

    pub fn prefix_unigrams(&self, ty: &str, prefix: &str) -> Vec<(String, f32)> {
        let Some(maps) = self.maps(ty) else {
            return Vec::new();
        };
        let mut out: Vec<(String, f32)> = maps
            .unigrams
            .iter()
            .filter(|(w, _)| w.starts_with(prefix) && w.as_str() != prefix)
            .map(|(w, t)| (w.clone(), self.decay(t.count, t.last)))
            .filter(|(_, c)| *c > 0.0)
            .collect();
        out.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        out
    }

    fn trim(&mut self, ty: &str) {
        let max_u = self.cfg.max_unigrams;
        let max_b = self.cfg.max_bigrams;
        let Some(maps) = self.by_type.get_mut(ty) else {
            return;
        };
        trim_map(&mut maps.unigrams, max_u);
        trim_map(&mut maps.bigrams, max_b);
    }

    pub fn persist(&mut self) -> Result<()> {
        if !self.cfg.enabled {
            return Ok(());
        }
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let now = Instant::now();
        let types = self
            .by_type
            .iter()
            .map(|(ty, maps)| {
                (
                    ty.clone(),
                    PersistBucket {
                        unigrams: maps
                            .unigrams
                            .iter()
                            .map(|(w, t)| (w.clone(), t.count, unix_from_instant(t.last, now)))
                            .collect(),
                        bigrams: maps
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
                    },
                )
            })
            .collect();
        let persist = PersistV2 {
            version: PERSIST_VERSION,
            by_type: types,
        };
        let bytes = postcard::to_stdvec(&persist).context("serialize user cache")?;
        let tmp = self.path.with_extension("bin.tmp");
        std::fs::write(&tmp, &bytes)?;
        std::fs::rename(&tmp, &self.path)?;
        Ok(())
    }
}

fn decay(count: f32, last: Instant, tau_hours: f32) -> f32 {
    decay_amount(count, last.elapsed(), tau_hours)
}

fn decay_amount(count: f32, dt: Duration, tau_hours: f32) -> f32 {
    let tau = Duration::from_secs_f32(tau_hours.max(0.01) * 3600.0);
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

    fn empty_cache(name: &str) -> UserCache {
        let cfg = CompletionUserCacheConfig::default();
        let mut c = UserCache::load(&cfg, PathBuf::from(format!("target/{name}")));
        c.by_type.clear();
        c
    }

    #[test]
    fn learn_boosts() {
        let mut c = empty_cache("kosk-test-cache.bin");
        c.learn_words(CATCH_ALL_TYPE, &["foo".into(), "bar".into()]);
        assert!(c.unigram(CATCH_ALL_TYPE, "foo") > 0.0);
        assert!(c.bigram(CATCH_ALL_TYPE, "foo", "bar") > 0.0);
    }

    #[test]
    fn continuations_lists_learned_pair() {
        let mut c = empty_cache("kosk-test-cache-cont.bin");
        c.learn_words(CATCH_ALL_TYPE, &["cat".into(), "sat".into()]);
        let next = c.continuations(CATCH_ALL_TYPE, "cat");
        assert_eq!(next[0].0, "sat");
        assert!(c.continuations(CATCH_ALL_TYPE, "the").is_empty());
    }

    #[test]
    fn types_are_isolated() {
        let mut c = empty_cache("kosk-test-cache-iso.bin");
        c.learn_words("browser", &["tab".into()]);
        assert!(c.unigram("browser", "tab") > 0.0);
        assert_eq!(c.unigram("programming", "tab"), 0.0);
        assert_eq!(c.unigram(CATCH_ALL_TYPE, "tab"), 0.0);
        assert!(!c.has_unigram("programming", "tab"));
        assert!(c.has_unigram("browser", "tab"));
    }

    #[test]
    fn prefix_unigrams_skips_identity() {
        let mut c = empty_cache("kosk-test-cache-pref.bin");
        c.learn_words("programming", &["jujutsu".into()]);
        let hits = c.prefix_unigrams("programming", "juju");
        assert_eq!(hits[0].0, "jujutsu");
        assert!(c.prefix_unigrams("programming", "jujutsu").is_empty());
        assert!(c.prefix_unigrams("browser", "juju").is_empty());
    }

    #[test]
    fn learn_survives_reload_before_interval() {
        let path = std::env::temp_dir().join(format!(
            "kosk-cache-learn-{}-{}.bin",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let cfg = CompletionUserCacheConfig {
            persist_interval_s: 3600,
            ..CompletionUserCacheConfig::default()
        };
        {
            let mut c = UserCache::load(&cfg, path.clone());
            c.learn_words(CATCH_ALL_TYPE, &["foobar".into()]);
        }
        let c = UserCache::load(&cfg, path.clone());
        assert!(
            c.has_unigram(CATCH_ALL_TYPE, "foobar"),
            "learn must hit disk before persist_interval_s"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn persist_round_trip_and_v1_migrate() {
        let path = std::env::temp_dir().join(format!(
            "kosk-cache-rt-{}-{}.bin",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        {
            let mut c = UserCache::load(&CompletionUserCacheConfig::default(), path.clone());
            c.by_type.clear();
            c.learn_words("browser", &["hello".into()]);
            c.persist().unwrap();
        }
        let c = UserCache::load(&CompletionUserCacheConfig::default(), path.clone());
        assert!(c.unigram("browser", "hello") > 0.0);
        let _ = std::fs::remove_file(&path);

        let v1 = PersistV1 {
            unigrams: vec![("old".into(), 4.0, 0)],
            bigrams: vec![],
        };
        let bytes = postcard::to_stdvec(&v1).unwrap();
        let v1_path = std::env::temp_dir().join(format!(
            "kosk-cache-v1-{}-{}.bin",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(&v1_path, bytes).unwrap();
        let c = UserCache::load(&CompletionUserCacheConfig::default(), v1_path.clone());
        assert!(c.has_unigram(CATCH_ALL_TYPE, "old"));
        let _ = std::fs::remove_file(&v1_path);
    }

    #[test]
    fn decay_half_life() {
        let hour = Duration::from_secs(3600);
        assert!((decay_amount(8.0, hour, 1.0) - 4.0).abs() < 1e-5);
        assert!((decay_amount(8.0, Duration::ZERO, 2.0) - 8.0).abs() < 1e-5);
    }
}
