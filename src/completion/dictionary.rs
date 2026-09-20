use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};
use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};

use super::backend::{Abort, Candidate, CompletionBackend, MatchKind, Source};
use super::case::restore_case;
use super::context::{normalize_word, CompletionContext};
use super::fuzzy::is_fuzzy_prefix;
use super::insert::{
    fold_apostrophe_marks, is_inserted_punct_fill, is_inserted_punct_prefix, strip_inserted,
};
use super::settings::CompletionConfig;
use super::user_cache::{mixed_count, UserCache};

#[derive(Clone)]
struct PunctIndexEntry {
    stripped: String,
    idx: usize,
}

#[derive(Debug, Clone)]
struct Entry {
    lower: String,
    surface: String,
    count: u32,
}

#[derive(Clone)]
struct Ranked {
    score: f32,
    idx: usize,
    lower: String,
    kind: MatchKind,
}

impl PartialEq for Ranked {
    fn eq(&self, other: &Self) -> bool {
        self.score == other.score && self.lower == other.lower
    }
}
impl Eq for Ranked {}
impl PartialOrd for Ranked {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Ranked {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .score
            .total_cmp(&self.score)
            .then_with(|| self.lower.cmp(&other.lower))
    }
}

/// Frequency-ranked prefix matcher. Lex order is only a tie-breaker.
#[derive(Clone)]
pub struct DictionaryEngine {
    entries: Vec<Entry>,
    suggest_next_word: bool,
    min_prefix_len: usize,
    normalize_nfc: bool,
    abort_every: usize,
    scan_limit: usize,
    typo_tolerance: bool,
    min_fuzzy_len: usize,
    transpose_neighbors_only: bool,
    lambda_typo: f32,
    extras: HashMap<String, DictionaryEngine>,
    user: Option<Arc<Mutex<UserCache>>>,
    count_total: u64,
    punct_index: Vec<PunctIndexEntry>,
}

impl DictionaryEngine {
    pub fn from_wordlist_text(text: &str, cfg: &CompletionConfig) -> Self {
        let mut entries: Vec<Entry> = text
            .lines()
            .filter_map(|line| parse_line(line, cfg.normalize_nfc))
            .collect();
        entries.sort_by(|a, b| a.lower.cmp(&b.lower).then_with(|| b.count.cmp(&a.count)));
        entries.dedup_by(|a, b| a.lower == b.lower);
        let punct_index = punct_index_from_entries(&entries);
        let count_total: u64 = entries.iter().map(|e| e.count as u64).sum();
        Self {
            entries,
            suggest_next_word: cfg.suggest_next_word,
            min_prefix_len: cfg.min_prefix_len,
            normalize_nfc: cfg.normalize_nfc,
            abort_every: cfg.ngram.abort_check_every.max(1),
            scan_limit: cfg.ngram.prefix_scan_limit.max(1),
            typo_tolerance: cfg.typo_tolerance,
            min_fuzzy_len: cfg.min_fuzzy_len.max(1),
            transpose_neighbors_only: cfg.transpose_neighbors_only,
            lambda_typo: cfg.ngram.lambda_typo,
            extras: HashMap::new(),
            user: None,
            count_total,
            punct_index,
        }
    }

    pub fn set_overlay(
        &mut self,
        extras: HashMap<String, DictionaryEngine>,
        user: Option<Arc<Mutex<UserCache>>>,
    ) {
        self.extras = extras;
        self.user = user;
    }

    pub fn contains_lower(&self, lower: &str) -> bool {
        self.entries
            .binary_search_by(|e| e.lower.as_str().cmp(lower))
            .is_ok()
    }

    pub fn count_lower(&self, lower: &str) -> u32 {
        let Ok(i) = self
            .entries
            .binary_search_by(|e| e.lower.as_str().cmp(lower))
        else {
            return 0;
        };
        self.entries[i].count
    }

    fn extra(&self, ty: &str) -> Option<&DictionaryEngine> {
        self.extras.get(ty)
    }

    fn overlay_dict_count(&self, ty: &str, lower: &str) -> f32 {
        let base = self.count_lower(lower) as f32;
        let extra = self.extra(ty).map(|e| e.count_lower(lower)).unwrap_or(0) as f32;
        base + extra
    }

    fn mixed_user_score(&self, ty: &str, lower: &str, user_count: f32) -> f32 {
        let mixed = mixed_count(
            self.overlay_dict_count(ty, lower),
            user_count,
            self.count_total as f32,
        );
        if mixed > 0.0 {
            mixed.ln()
        } else {
            0.0
        }
    }

    fn merge_overlay(
        &self,
        ctx: &CompletionContext,
        abort: &Abort<'_>,
        mut cands: Vec<Candidate>,
    ) -> Option<Vec<Candidate>> {
        let nfc = self.normalize_nfc;
        let token_lower = normalize_word(&ctx.token, nfc);
        let mut seen: std::collections::HashSet<String> =
            cands.iter().map(|c| normalize_word(&c.text, nfc)).collect();

        if let Some(extra) = self.extra(&ctx.app_type) {
            let extra_cands = extra.suggest(ctx, abort)?;
            for c in extra_cands {
                let lower = normalize_word(&c.text, nfc);
                if !seen.insert(lower) {
                    continue;
                }
                cands.push(c);
            }
        }

        if let Some(user) = &self.user {
            let g = user.lock().unwrap();
            let hits = if token_lower.is_empty() {
                ctx.prev_words
                    .last()
                    .map(|prev| g.continuations(&ctx.app_type, prev))
                    .unwrap_or_default()
            } else {
                g.prefix_unigrams(&ctx.app_type, &token_lower)
            };
            for (word, count) in hits {
                if abort.stale() {
                    return None;
                }
                let score = self.mixed_user_score(&ctx.app_type, &word, count);
                if let Some(c) = cands
                    .iter_mut()
                    .find(|c| normalize_word(&c.text, nfc) == word)
                {
                    c.score = score;
                    continue;
                }
                if !seen.insert(word.clone()) {
                    continue;
                }
                cands.push(Candidate {
                    text: restore_case(&ctx.token, &word, ctx.capitalize_sentence),
                    score,
                    source: Source::UserCache,
                    kind: MatchKind::ExactPrefix,
                });
            }
        }

        cands.sort_by(|a, b| {
            b.score
                .total_cmp(&a.score)
                .then_with(|| a.text.cmp(&b.text))
        });
        Some(mix_candidate_slots(
            cands,
            ctx.max_results.max(1),
            &token_lower,
        ))
    }

    pub fn from_path_or_embedded(path: &Path, cfg: &CompletionConfig) -> Result<Self> {
        if path.exists() {
            let raw = std::fs::read_to_string(path)
                .with_context(|| format!("read wordlist {}", path.display()))?;
            Ok(Self::from_wordlist_text(&raw, cfg))
        } else {
            Ok(Self::embedded_demo(cfg))
        }
    }

    pub fn embedded_demo(cfg: &CompletionConfig) -> Self {
        Self::from_wordlist_text(include_str!("fixtures/demo.tsv"), cfg)
    }

    pub fn entries(&self) -> impl Iterator<Item = (&str, u32)> {
        self.entries.iter().map(|e| (e.surface.as_str(), e.count))
    }

    fn prefix_range(&self, token_lower: &str) -> std::ops::Range<usize> {
        let start = self
            .entries
            .partition_point(|e| e.lower.as_str() < token_lower);
        let end = self.entries[start..]
            .iter()
            .position(|e| !e.lower.starts_with(token_lower))
            .map(|i| start + i)
            .unwrap_or(self.entries.len());
        start..end
    }

    fn punct_prefix_range(&self, stripped_token: &str) -> std::ops::Range<usize> {
        let start = self
            .punct_index
            .partition_point(|e| e.stripped.as_str() < stripped_token);
        let end = self.punct_index[start..]
            .iter()
            .position(|e| !e.stripped.starts_with(stripped_token))
            .map(|i| start + i)
            .unwrap_or(self.punct_index.len());
        start..end
    }

    fn rank(&self, idx: usize, e: &Entry, kind: MatchKind, penalty: f32) -> Ranked {
        let score = if e.count > 0 {
            (e.count as f32).ln() + penalty
        } else {
            -(e.lower.len() as f32) + penalty
        };
        Ranked {
            score,
            idx,
            lower: e.lower.clone(),
            kind,
        }
    }
}

fn mix_slots(
    mut exact: Vec<Ranked>,
    mut corrections: Vec<Ranked>,
    mut inserted: Vec<Ranked>,
    mut fuzzy: Vec<Ranked>,
    max: usize,
) -> Vec<Ranked> {
    let by_score = |a: &Ranked, b: &Ranked| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| a.lower.cmp(&b.lower))
    };
    exact.sort_by(by_score);
    corrections.sort_by(by_score);
    inserted.sort_by(by_score);
    fuzzy.sort_by(by_score);

    let mut out = Vec::with_capacity(max);
    let mut seen = std::collections::HashSet::new();
    if let Some(c) = corrections.into_iter().next() {
        seen.insert(c.lower.clone());
        out.push(c);
    }
    if out.len() < max {
        if let Some(c) = inserted.into_iter().next() {
            if seen.insert(c.lower.clone()) {
                out.push(c);
            }
        }
    }
    for r in exact.into_iter().chain(fuzzy) {
        if out.len() >= max {
            break;
        }
        if !seen.insert(r.lower.clone()) {
            continue;
        }
        out.push(r);
    }
    out
}

pub(crate) fn mix_candidate_slots(
    cands: Vec<Candidate>,
    max: usize,
    token: &str,
) -> Vec<Candidate> {
    let mut exact = Vec::new();
    let mut corrections = Vec::new();
    let mut inserted = Vec::new();
    let mut fuzzy = Vec::new();
    let token_lower = token.to_lowercase();
    for c in cands {
        match c.kind {
            MatchKind::Correction => corrections.push(c),
            MatchKind::Fuzzy => fuzzy.push(c),
            MatchKind::ExactPrefix => {
                if is_inserted_punct_fill(&token_lower, &c.text.to_lowercase()) {
                    inserted.push(c);
                } else {
                    exact.push(c);
                }
            }
        }
    }
    let by_score = |a: &Candidate, b: &Candidate| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| a.text.cmp(&b.text))
    };
    exact.sort_by(by_score);
    corrections.sort_by(by_score);
    inserted.sort_by(by_score);
    fuzzy.sort_by(by_score);
    let mut out = Vec::with_capacity(max);
    let mut seen = std::collections::HashSet::new();
    if let Some(c) = corrections.into_iter().next() {
        seen.insert(c.text.clone());
        out.push(c);
    }
    if out.len() < max {
        if let Some(c) = inserted.into_iter().next() {
            if seen.insert(c.text.clone()) {
                out.push(c);
            }
        }
    }
    for c in exact.into_iter().chain(fuzzy) {
        if out.len() >= max {
            break;
        }
        if !seen.insert(c.text.clone()) {
            continue;
        }
        out.push(c);
    }
    out
}

fn parse_line(line: &str, nfc: bool) -> Option<Entry> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    let (word, count) = match line.split_once('\t') {
        Some((w, c)) => (w.trim(), c.trim().parse().unwrap_or(0)),
        None => match line.split_once(' ') {
            Some((w, c)) if c.bytes().all(|b| b.is_ascii_digit()) => {
                (w.trim(), c.trim().parse().unwrap_or(0))
            }
            _ => (line, 0),
        },
    };
    if word.is_empty() {
        return None;
    }
    let word = fold_apostrophe_marks(word);
    Some(Entry {
        lower: normalize_word(&word, nfc),
        surface: word,
        count,
    })
}

fn punct_index_from_entries(entries: &[Entry]) -> Vec<PunctIndexEntry> {
    let mut index: Vec<PunctIndexEntry> = entries
        .iter()
        .enumerate()
        .filter_map(|(idx, e)| {
            let stripped = strip_inserted(&e.lower);
            if stripped.is_empty() || stripped == e.lower {
                None
            } else {
                Some(PunctIndexEntry { stripped, idx })
            }
        })
        .collect();
    index.sort_by(|a, b| a.stripped.cmp(&b.stripped).then_with(|| a.idx.cmp(&b.idx)));
    index
}

impl CompletionBackend for DictionaryEngine {
    fn suggest(&self, ctx: &CompletionContext, abort: &Abort<'_>) -> Option<Vec<Candidate>> {
        let token_lower = normalize_word(&ctx.token, self.normalize_nfc);
        if token_lower.chars().count() < self.min_prefix_len && !token_lower.is_empty() {
            return Some(Vec::new());
        }

        if token_lower.is_empty() {
            if !self.suggest_next_word {
                return Some(Vec::new());
            }
            let cands = self.top_unigrams(ctx, abort)?;
            return self.merge_overlay(ctx, abort, cands);
        }

        let range = self.prefix_range(&token_lower);
        let mut exact: Vec<Ranked> = Vec::new();
        let max = ctx.max_results.max(1);

        for (i, e) in self.entries[range.clone()].iter().enumerate() {
            if i & (self.abort_every - 1) == 0 && abort.stale() {
                return None;
            }
            if i >= self.scan_limit {
                break;
            }
            if e.lower == token_lower {
                continue;
            }
            exact.push(self.rank(range.start + i, e, MatchKind::ExactPrefix, 0.0));
        }

        let stripped_token = strip_inserted(&token_lower);
        let mut inserted: Vec<Ranked> = Vec::new();
        if !stripped_token.is_empty() {
            let punct_range = self.punct_prefix_range(&stripped_token);
            for (i, p) in self.punct_index[punct_range].iter().enumerate() {
                if i & (self.abort_every - 1) == 0 && abort.stale() {
                    return None;
                }
                if i >= self.scan_limit {
                    break;
                }
                let e = &self.entries[p.idx];
                if e.lower == token_lower || e.lower.starts_with(&token_lower) {
                    continue;
                }
                let ranked = self.rank(p.idx, e, MatchKind::ExactPrefix, 0.0);
                if is_inserted_punct_fill(&token_lower, &e.lower) {
                    inserted.push(ranked);
                } else {
                    exact.push(ranked);
                }
            }
        }

        let mut corrections: Vec<Ranked> = Vec::new();
        let mut fuzzy: Vec<Ranked> = Vec::new();
        if self.typo_tolerance {
            let token_chars = token_lower.chars().count();
            for (i, e) in self.entries.iter().enumerate() {
                if i & (self.abort_every - 1) == 0 && abort.stale() {
                    return None;
                }
                if e.lower == token_lower
                    || e.lower.starts_with(&token_lower)
                    || is_inserted_punct_prefix(&token_lower, &e.lower)
                {
                    continue;
                }
                if is_fuzzy_prefix(
                    &token_lower,
                    &e.lower,
                    &ctx.neighbors,
                    self.min_fuzzy_len,
                    self.transpose_neighbors_only,
                )
                .is_none()
                {
                    continue;
                }
                let word_chars = e.lower.chars().count();
                let match_kind = if word_chars <= token_chars {
                    MatchKind::Correction
                } else {
                    MatchKind::Fuzzy
                };
                let ranked = self.rank(i, e, match_kind, self.lambda_typo);
                if match_kind == MatchKind::Correction {
                    corrections.push(ranked);
                } else {
                    fuzzy.push(ranked);
                }
            }
        }

        let ranked = mix_slots(exact, corrections, inserted, fuzzy, max);
        let cands = ranked
            .into_iter()
            .map(|r| {
                let e = &self.entries[r.idx];
                Candidate {
                    text: restore_case(&ctx.token, &e.surface, ctx.capitalize_sentence),
                    score: r.score,
                    source: Source::Dictionary,
                    kind: r.kind,
                }
            })
            .collect();
        self.merge_overlay(ctx, abort, cands)
    }

    fn knows_word(&self, ctx: &CompletionContext) -> bool {
        let lower = normalize_word(&ctx.token, self.normalize_nfc);
        if lower.is_empty() {
            return false;
        }
        if self.contains_lower(&lower) {
            return true;
        }
        if self
            .extra(&ctx.app_type)
            .is_some_and(|e| e.contains_lower(&lower))
        {
            return true;
        }
        self.user
            .as_ref()
            .is_some_and(|u| u.lock().unwrap().has_unigram(&ctx.app_type, &lower))
    }
}

impl DictionaryEngine {
    fn top_unigrams(&self, ctx: &CompletionContext, abort: &Abort<'_>) -> Option<Vec<Candidate>> {
        let mut heap: BinaryHeap<Ranked> = BinaryHeap::new();
        let max = ctx.max_results.max(1);
        for (i, e) in self.entries.iter().enumerate() {
            if i & (self.abort_every - 1) == 0 && abort.stale() {
                return None;
            }
            let score = if e.count > 0 {
                (e.count as f32).ln()
            } else {
                -(e.lower.len() as f32)
            };
            heap.push(Ranked {
                score,
                idx: i,
                lower: e.lower.clone(),
                kind: MatchKind::ExactPrefix,
            });
            if heap.len() > max {
                heap.pop();
            }
        }
        let mut ranked: Vec<Ranked> = heap.into_vec();
        ranked.sort_by(|a, b| {
            b.score
                .total_cmp(&a.score)
                .then_with(|| a.lower.cmp(&b.lower))
        });
        Some(
            ranked
                .into_iter()
                .map(|r| {
                    let e = &self.entries[r.idx];
                    Candidate {
                        text: restore_case(&ctx.token, &e.surface, ctx.capitalize_sentence),
                        score: r.score,
                        source: Source::Dictionary,
                        kind: r.kind,
                    }
                })
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU64;

    fn abort() -> (AtomicU64, u64) {
        let g = AtomicU64::new(1);
        (g, 1)
    }

    #[test]
    fn frequency_beats_lex() {
        let cfg = CompletionConfig::default();
        let eng = DictionaryEngine::from_wordlist_text("aardvark\t1\nand\t1000\napple\t10\n", &cfg);
        let ctx = CompletionContext::from_buffer("a", 1, &cfg).unwrap();
        let (gen, mine) = abort();
        let abort = Abort {
            mine,
            current: &gen,
        };
        let s = eng.suggest(&ctx, &abort).unwrap();
        assert_eq!(s[0].text, "and");
        assert!(!s.iter().any(|c| c.text == "aardvark") || s[0].text != "aardvark");
    }

    #[test]
    fn respects_max() {
        let cfg = CompletionConfig {
            max_suggestions: 2,
            ..CompletionConfig::default()
        };
        let eng = DictionaryEngine::from_wordlist_text("aa\t3\nab\t2\nac\t1\nad\t1\n", &cfg);
        let ctx = CompletionContext::from_buffer("a", 1, &cfg).unwrap();
        let (gen, mine) = abort();
        let abort = Abort {
            mine,
            current: &gen,
        };
        assert_eq!(eng.suggest(&ctx, &abort).unwrap().len(), 2);
    }

    #[test]
    fn empty_token_without_next_word() {
        let cfg = CompletionConfig {
            suggest_next_word: false,
            ..CompletionConfig::default()
        };
        let eng = DictionaryEngine::from_wordlist_text("the\t100\n", &cfg);
        let ctx = CompletionContext::from_buffer("hello ", 6, &cfg).unwrap();
        let (gen, mine) = abort();
        let abort = Abort {
            mine,
            current: &gen,
        };
        assert!(eng.suggest(&ctx, &abort).unwrap().is_empty());
    }

    fn suggest_on(
        eng: &DictionaryEngine,
        text: &str,
        neighbors: Vec<(char, char)>,
    ) -> Vec<Candidate> {
        let cfg = CompletionConfig::default();
        let mut ctx = CompletionContext::from_buffer(text, text.len(), &cfg).unwrap();
        for (a, b) in neighbors {
            ctx.neighbors.entry(a).or_default().push(b);
            ctx.neighbors.entry(b).or_default().push(a);
        }
        let (gen, mine) = abort();
        let abort = Abort {
            mine,
            current: &gen,
        };
        eng.suggest(&ctx, &abort).unwrap()
    }

    #[test]
    fn the_is_not_suggested_for_the() {
        let cfg = CompletionConfig {
            max_suggestions: 3,
            ..CompletionConfig::default()
        };
        let eng = DictionaryEngine::from_wordlist_text("the\t100\nthere\t50\n", &cfg);
        let s = suggest_on(&eng, "the", vec![]);
        assert!(!s.iter().any(|c| c.text == "the"));
        assert!(s.iter().any(|c| c.text == "there"));
    }

    #[test]
    fn thr_reserves_the_correction() {
        let cfg = CompletionConfig {
            max_suggestions: 3,
            ..CompletionConfig::default()
        };
        let eng = DictionaryEngine::from_wordlist_text(
            "the\t1000\nthree\t100\nthrough\t90\nthrow\t80\n",
            &cfg,
        );
        let s = suggest_on(&eng, "thr", vec![('r', 'e')]);
        assert!(s
            .iter()
            .any(|c| c.text == "the" && c.kind == MatchKind::Correction));
        assert!(s.iter().any(|c| c.text == "three"));
    }

    #[test]
    fn hel_exact_not_displaced_by_gel() {
        let cfg = CompletionConfig {
            max_suggestions: 3,
            ..CompletionConfig::default()
        };
        let eng = DictionaryEngine::from_wordlist_text(
            "hello\t100\nhelp\t90\nhell\t80\ngel\t10000\n",
            &cfg,
        );
        let s = suggest_on(&eng, "hel", vec![('h', 'g')]);
        assert!(s.iter().any(|c| c.text == "hello"));
        let exact: Vec<_> = s
            .iter()
            .filter(|c| c.kind == MatchKind::ExactPrefix)
            .map(|c| c.text.as_str())
            .collect();
        assert!(exact.contains(&"hello"));
    }

    #[test]
    fn wen_suggests_when() {
        let cfg = CompletionConfig::default();
        let eng = DictionaryEngine::from_wordlist_text("when\t50\nwen\t1\n", &cfg);
        let s = suggest_on(&eng, "wen", vec![]);
        assert!(s.iter().any(|c| c.text == "when"));
    }

    #[test]
    fn wehn_and_no_transpose() {
        let cfg = CompletionConfig::default();
        let eng = DictionaryEngine::from_wordlist_text("when\t50\non\t40\n", &cfg);
        let s = suggest_on(&eng, "wehn", vec![]);
        assert!(s.iter().any(|c| c.text == "when"));
        let s = suggest_on(&eng, "no", vec![]);
        assert!(s.iter().any(|c| c.text == "on"));
    }

    #[test]
    fn far_subst_not_when() {
        let cfg = CompletionConfig::default();
        let eng = DictionaryEngine::from_wordlist_text("when\t50\n", &cfg);
        let s = suggest_on(&eng, "whqn", vec![]);
        assert!(!s.iter().any(|c| c.text == "when"));
    }

    #[test]
    fn dont_suggests_dont_apostrophe() {
        let cfg = CompletionConfig::default();
        let eng = DictionaryEngine::from_wordlist_text("don't\t50\n", &cfg);
        let s = suggest_on(&eng, "dont", vec![]);
        assert!(s.iter().any(|c| c.text.contains('\'')));
    }

    #[test]
    fn dont_inserts_apostrophe_without_typo_tolerance() {
        let cfg = CompletionConfig {
            typo_tolerance: false,
            ..CompletionConfig::default()
        };
        let eng = DictionaryEngine::from_wordlist_text("don't\t50\n", &cfg);
        let s = suggest_on(&eng, "dont", vec![]);
        assert!(s
            .iter()
            .any(|c| c.text == "don't" && c.kind == MatchKind::ExactPrefix));
    }

    #[test]
    fn eg_inserts_periods_without_typo_tolerance() {
        let cfg = CompletionConfig {
            typo_tolerance: false,
            ..CompletionConfig::default()
        };
        let eng = DictionaryEngine::from_wordlist_text("e.g.\t50\n", &cfg);
        let s = suggest_on(&eng, "eg", vec![]);
        assert!(s
            .iter()
            .any(|c| c.text == "e.g." && c.kind == MatchKind::ExactPrefix));
    }

    #[test]
    fn backtick_line_loads_as_apostrophe() {
        let cfg = CompletionConfig {
            typo_tolerance: false,
            ..CompletionConfig::default()
        };
        let eng = DictionaryEngine::from_wordlist_text("don`t\t50\n", &cfg);
        let s = suggest_on(&eng, "dont", vec![]);
        assert!(s.iter().any(|c| c.text == "don't"));
    }

    #[test]
    fn im_reserves_im_apostrophe_against_frequent_prefixes() {
        let cfg = CompletionConfig {
            max_suggestions: 3,
            typo_tolerance: false,
            ..CompletionConfig::default()
        };
        let eng = DictionaryEngine::from_wordlist_text(
            "i'm\t10\nimportant\t1000\nimagine\t900\nimmediately\t800\n",
            &cfg,
        );
        let s = suggest_on(&eng, "im", vec![]);
        assert!(s.iter().any(|c| c.text == "i'm"), "{s:?}");
    }

    #[test]
    fn ill_reserves_ill_apostrophe_against_frequent_prefixes() {
        let cfg = CompletionConfig {
            max_suggestions: 3,
            typo_tolerance: false,
            ..CompletionConfig::default()
        };
        let eng = DictionaryEngine::from_wordlist_text(
            "i'll\t10\nillegal\t1000\nillness\t900\nillusion\t800\n",
            &cfg,
        );
        let s = suggest_on(&eng, "ill", vec![]);
        assert!(s.iter().any(|c| c.text == "i'll"), "{s:?}");
    }

    #[test]
    fn extra_wordlist_only_for_that_type() {
        let cfg = CompletionConfig::default();
        let extra = DictionaryEngine::from_wordlist_text("jujutsu\t10\n", &cfg);
        let mut extras = std::collections::HashMap::new();
        extras.insert("programming".into(), extra);
        let mut eng = DictionaryEngine::from_wordlist_text("hello\t10\nhelp\t5\n", &cfg);
        eng.set_overlay(extras, None);

        let (gen, mine) = abort();
        let abort = Abort {
            mine,
            current: &gen,
        };
        let mut ctx = CompletionContext::from_buffer("juju", 4, &cfg).unwrap();
        ctx.app_type = "programming".into();
        let s = eng.suggest(&ctx, &abort).unwrap();
        assert!(s.iter().any(|c| c.text == "jujutsu"));
        assert!(!s.iter().any(|c| c.text == "juju"));

        ctx.app_type = "browser".into();
        let s = eng.suggest(&ctx, &abort).unwrap();
        assert!(!s.iter().any(|c| c.text == "jujutsu"));

        let mut hel = CompletionContext::from_buffer("hel", 3, &cfg).unwrap();
        hel.app_type = "browser".into();
        let s = eng.suggest(&hel, &abort).unwrap();
        assert!(s.iter().any(|c| c.text == "hello"));
    }
}
