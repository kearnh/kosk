use memmap2::Mmap;
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::{bail, Context, Result};

use super::backend::{Abort, Candidate, CompletionBackend, Source};
use super::case::restore_case;
use super::context::{normalize_word, CompletionContext};
use super::dictionary::DictionaryEngine;
use super::settings::CompletionConfig;
use super::user_cache::UserCache;

pub const ID_BITS: u32 = 21;
pub const ID_MASK: u64 = (1 << ID_BITS) - 1;
pub const FORMAT_VERSION: u32 = 1;

pub fn pack2(a: u32, b: u32) -> u64 {
    ((a as u64) << ID_BITS) | (b as u64 & ID_MASK)
}

pub fn pack3(a: u32, b: u32, c: u32) -> u64 {
    ((a as u64) << (ID_BITS * 2)) | ((b as u64) << ID_BITS) | (c as u64 & ID_MASK)
}

pub fn lookup_count(table: &[(u64, u32)], key: u64) -> u32 {
    table
        .binary_search_by_key(&key, |x| x.0)
        .ok()
        .map(|i| table[i].1)
        .unwrap_or(0)
}

pub fn range_with_prefix(table: &[(u64, u32)], lo: u64, hi: u64) -> &[(u64, u32)] {
    let start = table.partition_point(|x| x.0 < lo);
    let end = table[start..].partition_point(|x| x.0 < hi) + start;
    &table[start..end]
}

pub fn write_count_table(path: &Path, rows: &[(u64, u32)]) -> Result<()> {
    let mut f = File::create(path).with_context(|| format!("create {}", path.display()))?;
    for (k, c) in rows {
        f.write_all(&k.to_le_bytes())?;
        f.write_all(&c.to_le_bytes())?;
    }
    Ok(())
}

pub fn read_count_table(path: &Path) -> Result<Vec<(u64, u32)>> {
    let file = File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mmap = unsafe { Mmap::map(&file) }.with_context(|| format!("mmap {}", path.display()))?;
    if mmap.len() % 12 != 0 {
        bail!("{}: size {} not multiple of 12", path.display(), mmap.len());
    }
    let mut rows = Vec::with_capacity(mmap.len() / 12);
    for chunk in mmap.chunks_exact(12) {
        let k = u64::from_le_bytes(chunk[0..8].try_into().unwrap());
        let c = u32::from_le_bytes(chunk[8..12].try_into().unwrap());
        rows.push((k, c));
    }
    Ok(rows)
}

pub fn write_unigrams(path: &Path, counts: &[u32]) -> Result<()> {
    let mut f = File::create(path).with_context(|| format!("create {}", path.display()))?;
    for c in counts {
        f.write_all(&c.to_le_bytes())?;
    }
    Ok(())
}

pub fn read_unigrams(path: &Path) -> Result<Vec<u32>> {
    let file = File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mmap = unsafe { Mmap::map(&file) }.with_context(|| format!("mmap {}", path.display()))?;
    if mmap.len() % 4 != 0 {
        bail!("{}: size {} not multiple of 4", path.display(), mmap.len());
    }
    Ok(mmap
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes(c.try_into().unwrap()))
        .collect())
}

pub struct NgramEngine {
    dict: DictionaryEngine,
    vocab: Vec<String>,
    vocab_ids: HashMap<String, u32>,
    unigrams: Vec<u32>,
    unigram_total: u64,
    bigrams: Vec<(u64, u32)>,
    trigrams: Vec<(u64, u32)>,
    alpha: f32,
    lambda_tri: f32,
    lambda_bi: f32,
    lambda_uni: f32,
    lambda_user: f32,
    lambda_exact: f32,
    user: Option<Arc<Mutex<UserCache>>>,
    nfc: bool,
    abort_every: usize,
}

impl NgramEngine {
    pub fn from_dictionary(
        dict: DictionaryEngine,
        cfg: &CompletionConfig,
        user: Option<Arc<Mutex<UserCache>>>,
    ) -> Self {
        Self {
            dict,
            vocab: Vec::new(),
            vocab_ids: HashMap::new(),
            unigrams: Vec::new(),
            unigram_total: 0,
            bigrams: Vec::new(),
            trigrams: Vec::new(),
            alpha: cfg.ngram.backoff_alpha,
            lambda_tri: cfg.ngram.lambda_trigram,
            lambda_bi: cfg.ngram.lambda_bigram,
            lambda_uni: cfg.ngram.lambda_unigram,
            lambda_user: cfg.ngram.lambda_user,
            lambda_exact: cfg.ngram.lambda_exact,
            user,
            nfc: cfg.normalize_nfc,
            abort_every: cfg.ngram.abort_check_every.max(1),
        }
    }

    pub fn load(dir: &Path, cfg: &CompletionConfig, user: Arc<Mutex<UserCache>>) -> Result<Self> {
        let vocab_path = dir.join("vocab.txt");
        let uni_path = dir.join("unigrams.bin");
        if !vocab_path.exists() || !uni_path.exists() {
            bail!("missing vocab.txt or unigrams.bin in {}", dir.display());
        }
        let vocab_text = std::fs::read_to_string(&vocab_path)?;
        let mut vocab_ids = HashMap::new();
        let mut vocab = Vec::new();
        let mut tsv = String::new();
        for (i, line) in vocab_text.lines().enumerate() {
            let w = line.trim();
            if w.is_empty() {
                continue;
            }
            vocab_ids.insert(normalize_word(w, cfg.normalize_nfc), i as u32);
            vocab.push(w.to_string());
            tsv.push_str(w);
            tsv.push('\n');
        }
        let unigrams = read_unigrams(&uni_path)?;
        let unigram_total: u64 = unigrams.iter().map(|c| *c as u64).sum();

        let dict = if dir.join("unigrams.tsv").exists() {
            let raw = std::fs::read_to_string(dir.join("unigrams.tsv"))?;
            DictionaryEngine::from_wordlist_text(&raw, cfg)
        } else {
            DictionaryEngine::from_wordlist_text(&tsv, cfg)
        };

        let bigrams = if dir.join("bigrams.bin").exists() {
            read_count_table(&dir.join("bigrams.bin"))?
        } else {
            Vec::new()
        };
        let trigrams = if dir.join("trigrams.bin").exists() {
            read_count_table(&dir.join("trigrams.bin"))?
        } else {
            Vec::new()
        };

        Ok(Self {
            dict,
            vocab,
            vocab_ids,
            unigrams,
            unigram_total,
            bigrams,
            trigrams,
            alpha: cfg.ngram.backoff_alpha,
            lambda_tri: cfg.ngram.lambda_trigram,
            lambda_bi: cfg.ngram.lambda_bigram,
            lambda_uni: cfg.ngram.lambda_unigram,
            lambda_user: cfg.ngram.lambda_user,
            lambda_exact: cfg.ngram.lambda_exact,
            user: Some(user),
            nfc: cfg.normalize_nfc,
            abort_every: cfg.ngram.abort_check_every.max(1),
        })
    }

    fn id(&self, w: &str) -> Option<u32> {
        self.vocab_ids.get(w).copied()
    }

    fn count1(&self, w: u32) -> u32 {
        self.unigrams.get(w as usize).copied().unwrap_or(0)
    }

    fn stupid(&self, prev: &[u32], w: u32) -> f32 {
        match prev {
            [a, b] => {
                let c3 = lookup_count(&self.trigrams, pack3(*a, *b, w));
                if c3 > 0 {
                    let den = lookup_count(&self.bigrams, pack2(*a, *b)).max(1);
                    return c3 as f32 / den as f32;
                }
                self.alpha * self.stupid(&prev[1..], w)
            }
            [a] => {
                let c2 = lookup_count(&self.bigrams, pack2(*a, w));
                if c2 > 0 {
                    let den = self.count1(*a).max(1);
                    return c2 as f32 / den as f32;
                }
                self.alpha * self.stupid(&[], w)
            }
            _ => {
                let c = self.count1(w) as f32;
                (c / (self.unigram_total.max(1) as f32)).max(1e-12)
            }
        }
    }

    fn prev_ids(&self, ctx: &CompletionContext) -> Vec<u32> {
        ctx.prev_words
            .iter()
            .filter_map(|w| self.id(&normalize_word(w, self.nfc)))
            .collect()
    }

    fn blend(&self, ctx: &CompletionContext, lower: &str) -> f32 {
        let ids = self.prev_ids(ctx);
        let wid = self.id(lower);
        let mut score = 0.0;
        if let Some(w) = wid {
            let tri = if ids.len() >= 2 {
                self.stupid(&ids[ids.len() - 2..], w)
            } else {
                0.0
            };
            let bi = if let Some(a) = ids.last() {
                self.stupid(&[*a], w)
            } else {
                0.0
            };
            let uni = self.stupid(&[], w);
            if tri > 0.0 {
                score += self.lambda_tri * tri.ln();
            }
            if bi > 0.0 {
                score += self.lambda_bi * bi.ln();
            }
            score += self.lambda_uni * uni.ln();
        }
        if let Some(cache) = &self.user {
            let g = cache.lock().unwrap();
            let u = g.unigram(lower);
            let b = if let Some(prev) = ctx.prev_words.last() {
                g.bigram(prev, lower)
            } else {
                0.0
            };
            if u > 0.0 {
                score += self.lambda_user * u.ln();
            }
            if b > 0.0 {
                score += self.lambda_user * b.ln();
            }
        }
        if normalize_word(&ctx.token, self.nfc) == lower {
            score += self.lambda_exact;
        }
        score
    }

    fn next_word_from_tables(
        &self,
        ctx: &CompletionContext,
        abort: &Abort<'_>,
    ) -> Option<Vec<Candidate>> {
        let ids = self.prev_ids(ctx);
        let mut scored: Vec<(String, f32, Source)> = Vec::new();

        if ids.len() >= 2 {
            let a = ids[ids.len() - 2];
            let b = ids[ids.len() - 1];
            let lo = pack3(a, b, 0);
            let hi = pack3(a, b.saturating_add(1), 0);
            let rows = if b == ID_MASK as u32 {
                range_with_prefix(&self.trigrams, lo, u64::MAX)
            } else {
                range_with_prefix(&self.trigrams, lo, hi)
            };
            for (i, (key, _)) in rows.iter().enumerate() {
                if i % self.abort_every == 0 && abort.stale() {
                    return None;
                }
                let w = (*key & ID_MASK) as u32;
                if let Some(word) = self.vocab.get(w as usize) {
                    let s = self.blend(ctx, word);
                    scored.push((word.clone(), s, Source::Ngram));
                }
            }
        }

        if scored.is_empty() {
            if let Some(a) = ids.last() {
                let lo = pack2(*a, 0);
                let hi = pack2(a.saturating_add(1), 0);
                let rows = if *a == ID_MASK as u32 {
                    range_with_prefix(&self.bigrams, lo, u64::MAX)
                } else {
                    range_with_prefix(&self.bigrams, lo, hi)
                };
                for (i, (key, _)) in rows.iter().enumerate() {
                    if i % self.abort_every == 0 && abort.stale() {
                        return None;
                    }
                    let w = (*key & ID_MASK) as u32;
                    if let Some(word) = self.vocab.get(w as usize) {
                        let s = self.blend(ctx, word);
                        scored.push((word.clone(), s, Source::Ngram));
                    }
                }
            }
        }

        if scored.is_empty() {
            return self.dict.suggest(ctx, abort);
        }

        scored.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        scored.truncate(ctx.max_results.max(1));
        Some(
            scored
                .into_iter()
                .map(|(w, score, source)| Candidate {
                    text: restore_case(&ctx.token, &w, ctx.capitalize_sentence),
                    score,
                    source,
                })
                .collect(),
        )
    }
}

impl CompletionBackend for NgramEngine {
    fn suggest(&self, ctx: &CompletionContext, abort: &Abort<'_>) -> Option<Vec<Candidate>> {
        let token_lower = normalize_word(&ctx.token, self.nfc);
        if token_lower.is_empty() {
            return self.next_word_from_tables(ctx, abort);
        }

        let mut cands = self.dict.suggest(ctx, abort)?;
        for c in &mut cands {
            if abort.stale() {
                return None;
            }
            let lower = normalize_word(&c.text, self.nfc);
            c.score = self.blend(ctx, &lower);
            if self.vocab_ids.contains_key(&lower) {
                c.source = Source::Ngram;
            }
        }
        cands.sort_by(|a, b| {
            b.score
                .total_cmp(&a.score)
                .then_with(|| a.text.cmp(&b.text))
        });
        Some(cands)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU64;

    #[test]
    fn backoff_prefers_trigram() {
        let cfg = CompletionConfig::default();
        let dict =
            DictionaryEngine::from_wordlist_text("the\t10\ncat\t10\nsat\t10\nmat\t5\n", &cfg);
        let mut eng = NgramEngine::from_dictionary(dict, &cfg, None);
        eng.vocab = vec!["the".into(), "cat".into(), "sat".into(), "mat".into()];
        eng.vocab_ids.insert("the".into(), 0);
        eng.vocab_ids.insert("cat".into(), 1);
        eng.vocab_ids.insert("sat".into(), 2);
        eng.vocab_ids.insert("mat".into(), 3);
        eng.unigrams = vec![10, 10, 10, 5];
        eng.unigram_total = 35;
        eng.bigrams = vec![(pack2(0, 1), 8)];
        eng.trigrams = vec![(pack3(0, 1, 2), 7)];
        let ctx = CompletionContext::from_buffer("the cat ", 8, &cfg).unwrap();
        let gen = AtomicU64::new(1);
        let abort = Abort {
            mine: 1,
            current: &gen,
        };
        let out = eng.suggest(&ctx, &abort).unwrap();
        assert_eq!(out[0].text, "sat");
    }

    #[test]
    fn backoff_when_trigram_missing() {
        let cfg = CompletionConfig::default();
        let dict =
            DictionaryEngine::from_wordlist_text("the\t10\ncat\t10\nsat\t10\nmat\t5\n", &cfg);
        let mut eng = NgramEngine::from_dictionary(dict, &cfg, None);
        eng.vocab = vec!["the".into(), "cat".into(), "sat".into(), "mat".into()];
        eng.vocab_ids.insert("the".into(), 0);
        eng.vocab_ids.insert("cat".into(), 1);
        eng.vocab_ids.insert("sat".into(), 2);
        eng.vocab_ids.insert("mat".into(), 3);
        eng.unigrams = vec![10, 10, 10, 5];
        eng.unigram_total = 35;
        eng.bigrams = vec![(pack2(1, 2), 6)];
        eng.trigrams = Vec::new();
        let ctx = CompletionContext::from_buffer("the cat ", 8, &cfg).unwrap();
        let gen = AtomicU64::new(1);
        let abort = Abort {
            mine: 1,
            current: &gen,
        };
        let out = eng.suggest(&ctx, &abort).unwrap();
        assert_eq!(out[0].text, "sat");
    }
}
