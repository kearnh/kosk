use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Lines,
    Tsv,
    Anvaka,
    Wiki,
    CsvIdent,
}

impl Format {
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "lines" => Ok(Self::Lines),
            "tsv" => Ok(Self::Tsv),
            "anvaka" => Ok(Self::Anvaka),
            "wiki" => Ok(Self::Wiki),
            "csv-ident" => Ok(Self::CsvIdent),
            other => bail!("unknown format '{other}'"),
        }
    }
}

pub struct ConvertOpts<'a> {
    pub format: Format,
    pub inputs: &'a [std::path::PathBuf],
    pub exclude: Option<&'a Path>,
    pub max_words: usize,
    pub min_len: usize,
}

pub fn convert(opts: ConvertOpts<'_>) -> Result<Vec<(String, u64)>> {
    let mut counts: HashMap<String, u64> = HashMap::new();
    for path in opts.inputs {
        let raw = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
        merge_file(&mut counts, opts.format, &raw, opts.min_len)?;
    }

    if let Some(ex) = opts.exclude {
        let raw =
            fs::read_to_string(ex).with_context(|| format!("read exclude {}", ex.display()))?;
        let drop = load_exclude(&raw);
        counts.retain(|w, _| !drop.contains(w));
    }

    let mut rows: Vec<(String, u64)> = counts.into_iter().collect();
    rows.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    if opts.max_words > 0 && rows.len() > opts.max_words {
        rows.truncate(opts.max_words);
    }
    Ok(rows)
}

pub fn write_tsv(path: &Path, rows: &[(String, u64)]) -> Result<()> {
    let mut out = String::new();
    for (w, c) in rows {
        out.push_str(w);
        out.push('\t');
        out.push_str(&c.to_string());
        out.push('\n');
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).ok();
    }
    fs::write(path, out).with_context(|| format!("write {}", path.display()))
}

fn merge_file(
    counts: &mut HashMap<String, u64>,
    format: Format,
    raw: &str,
    min_len: usize,
) -> Result<()> {
    match format {
        Format::Lines => parse_lines(counts, raw, min_len),
        Format::Tsv => parse_tsv(counts, raw, min_len),
        Format::Anvaka => parse_anvaka(counts, raw, min_len),
        Format::Wiki => parse_wiki(counts, raw, min_len),
        Format::CsvIdent => parse_csv_ident(counts, raw, min_len),
    }
}

fn parse_lines(counts: &mut HashMap<String, u64>, raw: &str, min_len: usize) -> Result<()> {
    for line in raw.lines() {
        let word = line.trim();
        add_token(counts, word, 1, min_len);
    }
    Ok(())
}

fn parse_tsv(counts: &mut HashMap<String, u64>, raw: &str, min_len: usize) -> Result<()> {
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let (word, count) = split_word_count(line);
        add_token(counts, word, count, min_len);
    }
    Ok(())
}

fn parse_wiki(counts: &mut HashMap<String, u64>, raw: &str, min_len: usize) -> Result<()> {
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let (word, rest) = split_cols(line);
        if word.eq_ignore_ascii_case("[total]") {
            continue;
        }
        let Some(count) = parse_count(rest) else {
            continue;
        };
        add_token(counts, word, count, min_len);
    }
    Ok(())
}

fn parse_csv_ident(counts: &mut HashMap<String, u64>, raw: &str, min_len: usize) -> Result<()> {
    for (i, line) in raw.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some((name, total)) = line.split_once(',') else {
            continue;
        };
        if i == 0 && name.eq_ignore_ascii_case("name") {
            continue;
        }
        let Some(count) = parse_count(total.trim()) else {
            continue;
        };
        let word = name.rsplit('.').next().unwrap_or(name).trim();
        add_token(counts, word, count, min_len);
    }
    Ok(())
}

fn parse_anvaka(counts: &mut HashMap<String, u64>, raw: &str, min_len: usize) -> Result<()> {
    let val: serde_json::Value = serde_json::from_str(raw).context("parse anvaka json")?;
    let Some(arr) = val.as_array() else {
        bail!("anvaka json must be an array");
    };
    for item in arr {
        let Some(word) = item.get("word").and_then(|w| w.as_str()) else {
            continue;
        };
        let mut n = 0u64;
        if let Some(ctx) = item.get("context").and_then(|c| c.as_array()) {
            for pair in ctx {
                let Some(row) = pair.as_array() else {
                    continue;
                };
                if row.len() < 2 {
                    continue;
                }
                n = n.saturating_add(json_count(&row[1]));
            }
        }
        if n == 0 {
            continue;
        }
        add_token(counts, word, n, min_len);
    }
    Ok(())
}

fn json_count(v: &serde_json::Value) -> u64 {
    v.as_u64()
        .or_else(|| v.as_i64().and_then(|n| u64::try_from(n).ok()))
        .or_else(|| v.as_f64().map(|n| n.max(0.0) as u64))
        .unwrap_or(0)
}

fn split_cols(line: &str) -> (&str, &str) {
    if let Some((a, b)) = line.split_once('\t') {
        return (a.trim(), b.trim());
    }
    if let Some((a, b)) = line.split_once(' ') {
        return (a.trim(), b.trim());
    }
    (line, "")
}

fn split_word_count(line: &str) -> (&str, u64) {
    let (word, rest) = split_cols(line);
    (word, parse_count(rest).unwrap_or(1))
}

fn parse_count(s: &str) -> Option<u64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    s.parse().ok()
}

fn load_exclude(raw: &str) -> HashSet<String> {
    let mut set = HashSet::new();
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let (word, _) = split_cols(line);
        let key = word.to_lowercase();
        if !key.is_empty() {
            set.insert(key);
        }
    }
    set
}

fn add_token(counts: &mut HashMap<String, u64>, word: &str, count: u64, min_len: usize) {
    let Some(key) = accept_token(word, min_len) else {
        return;
    };
    if count == 0 {
        return;
    }
    *counts.entry(key).or_insert(0) += count;
}

fn accept_token(word: &str, min_len: usize) -> Option<String> {
    let word = word.trim();
    if word.is_empty() {
        return None;
    }
    if word.contains('.') {
        return None;
    }
    if word.chars().count() < min_len {
        return None;
    }
    if !word
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '\'')
    {
        return None;
    }
    Some(word.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn convert_str(format: Format, raw: &str, exclude: Option<&str>) -> Vec<(String, u64)> {
        convert_many(format, &[raw], exclude, 0)
    }

    fn convert_many(
        format: Format,
        raws: &[&str],
        exclude: Option<&str>,
        max_words: usize,
    ) -> Vec<(String, u64)> {
        let mut counts = HashMap::new();
        for raw in raws {
            merge_file(&mut counts, format, raw, 3).unwrap();
        }
        if let Some(ex) = exclude {
            let drop = load_exclude(ex);
            counts.retain(|w, _| !drop.contains(w));
        }
        let mut rows: Vec<(String, u64)> = counts.into_iter().collect();
        rows.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        if max_words > 0 && rows.len() > max_words {
            rows.truncate(max_words);
        }
        rows
    }

    #[test]
    fn anvaka_sums_context_counts() {
        let raw = r#"[{"word":"async","context":[["a.rs",2],["b.rs",3]]}]"#;
        let rows = convert_str(Format::Anvaka, raw, None);
        assert_eq!(rows, vec![("async".into(), 5)]);
    }

    #[test]
    fn wiki_skips_header_and_total() {
        let raw = "word\tcount\n[TOTAL]\t999\nhello\t10\nworld\t4\n";
        let rows = convert_str(Format::Wiki, raw, None);
        assert_eq!(rows, vec![("hello".into(), 10), ("world".into(), 4)]);
    }

    #[test]
    fn lines_count_one() {
        let raw = "foo\nbar\nfoo\n";
        let rows = convert_str(Format::Lines, raw, None);
        assert_eq!(rows, vec![("foo".into(), 2), ("bar".into(), 1)]);
    }

    #[test]
    fn csv_ident_last_segment() {
        let raw = "name,total\nstd.vec.Vec,42\ncore.option.Option,7\n";
        let rows = convert_str(Format::CsvIdent, raw, None);
        assert_eq!(rows, vec![("vec".into(), 42), ("option".into(), 7)]);
    }

    #[test]
    fn exclude_drops_unigram_hits() {
        let raw = "hello\t10\nxyzzy\t3\n";
        let rows = convert_str(Format::Tsv, raw, Some("hello\t50000\n"));
        assert_eq!(rows, vec![("xyzzy".into(), 3)]);
    }

    #[test]
    fn merge_sums() {
        let rows = convert_many(Format::Tsv, &["foo\t2\n", "foo\t3\nbar\t1\n"], None, 0);
        assert_eq!(rows, vec![("foo".into(), 5), ("bar".into(), 1)]);
    }

    #[test]
    fn reject_dot_path_tokens() {
        let raw = "foo.bar\t9\nbaz\t2\n";
        let rows = convert_str(Format::Tsv, raw, None);
        assert_eq!(rows, vec![("baz".into(), 2)]);
    }
}
