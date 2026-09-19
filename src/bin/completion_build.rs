use anyhow::{Context, Result};
use clap::Parser;
use kosk::completion::{pack2, pack3, write_count_table, write_unigrams, FORMAT_VERSION};
use rayon::prelude::*;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "completion_build")]
struct Args {
    /// Word\tcount TSV (or word count).
    #[arg(long)]
    unigrams: PathBuf,
    /// One sentence per line; counted for bigrams/trigrams.
    #[arg(long)]
    corpus: Option<PathBuf>,
    /// Pre-counted bigram TSV: w1 w2 count
    #[arg(long)]
    bigrams: Option<PathBuf>,
    /// Pre-counted trigram TSV: w1 w2 w3 count
    #[arg(long)]
    trigrams: Option<PathBuf>,
    #[arg(long)]
    out: PathBuf,
    #[arg(long, default_value_t = 500_000)]
    max_bigrams: usize,
    #[arg(long, default_value_t = 1_000_000)]
    max_trigrams: usize,
    #[arg(long, default_value = "en")]
    locale: String,
}

fn main() -> Result<()> {
    let args = Args::parse();
    fs::create_dir_all(&args.out)?;

    let uni_raw = fs::read_to_string(&args.unigrams)
        .with_context(|| format!("read {}", args.unigrams.display()))?;
    let mut words: Vec<(String, u32)> = Vec::new();
    for line in uni_raw.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let (w, c) = if let Some((a, b)) = line.split_once('\t') {
            (a.trim().to_lowercase(), b.trim().parse().unwrap_or(0))
        } else if let Some((a, b)) = line.split_once(' ') {
            (a.trim().to_lowercase(), b.trim().parse().unwrap_or(0))
        } else {
            (line.to_lowercase(), 0)
        };
        if w.is_empty() {
            continue;
        }
        words.push((w, c));
    }
    words.sort_by(|a, b| a.0.cmp(&b.0));
    words.dedup_by(|a, b| a.0 == b.0);

    let mut vocab_txt = String::new();
    let mut ids = HashMap::new();
    let mut counts = Vec::with_capacity(words.len());
    for (i, (w, c)) in words.iter().enumerate() {
        ids.insert(w.clone(), i as u32);
        vocab_txt.push_str(w);
        vocab_txt.push('\n');
        counts.push(*c);
    }

    // Pack artifacts are vocab.txt + *.bin. The source unigrams.tsv is input-only.
    fs::write(args.out.join("vocab.txt"), vocab_txt)?;
    write_unigrams(&args.out.join("unigrams.bin"), &counts)?;

    let mut bigrams: HashMap<(u32, u32), u32> = HashMap::new();
    let mut trigrams: HashMap<(u32, u32, u32), u32> = HashMap::new();

    if let Some(path) = &args.corpus {
        let corpus = fs::read_to_string(path)?;
        let lines: Vec<&str> = corpus.lines().collect();
        let parts: Vec<HashMap<(u32, u32), u32>> = lines
            .par_iter()
            .map(|line| count_line_bi(&ids, line))
            .collect();
        for p in parts {
            for (k, v) in p {
                *bigrams.entry(k).or_insert(0) += v;
            }
        }
        let parts: Vec<HashMap<(u32, u32, u32), u32>> = lines
            .par_iter()
            .map(|line| count_line_tri(&ids, line))
            .collect();
        for p in parts {
            for (k, v) in p {
                *trigrams.entry(k).or_insert(0) += v;
            }
        }
    }

    if let Some(path) = &args.bigrams {
        merge_bi(&mut bigrams, &ids, &fs::read_to_string(path)?)?;
    }
    if let Some(path) = &args.trigrams {
        merge_tri(&mut trigrams, &ids, &fs::read_to_string(path)?)?;
    }

    let mut bi_rows: Vec<(u64, u32)> = bigrams
        .into_iter()
        .map(|((a, b), c)| (pack2(a, b), c))
        .collect();
    bi_rows.sort_by_key(|r| std::cmp::Reverse(r.1));
    bi_rows.truncate(args.max_bigrams);
    bi_rows.sort_by_key(|r| r.0);
    write_count_table(&args.out.join("bigrams.bin"), &bi_rows)?;

    let mut tri_rows: Vec<(u64, u32)> = trigrams
        .into_iter()
        .map(|((a, b, c), n)| (pack3(a, b, c), n))
        .collect();
    tri_rows.sort_by_key(|r| std::cmp::Reverse(r.1));
    tri_rows.truncate(args.max_trigrams);
    tri_rows.sort_by_key(|r| r.0);
    write_count_table(&args.out.join("trigrams.bin"), &tri_rows)?;

    let manifest = format!(
        "format_version = {FORMAT_VERSION}\nlocale = {:?}\nvocab_size = {}\norder = 3\n",
        args.locale,
        words.len()
    );
    fs::write(args.out.join("manifest.toml"), manifest)?;
    println!(
        "wrote {} words, {} bigrams, {} trigrams -> {}",
        words.len(),
        bi_rows.len(),
        tri_rows.len(),
        args.out.display()
    );
    if bi_rows.is_empty() {
        eprintln!(
            "warning: 0 bigrams; next-word will stay top unigrams. Pass --bigrams FILE or --corpus FILE."
        );
    }
    Ok(())
}

fn tokens<'a>(ids: &'a HashMap<String, u32>, line: &'a str) -> Vec<u32> {
    line.split(|c: char| !c.is_alphanumeric() && c != '\'' && c != '_')
        .filter(|s| !s.is_empty())
        .filter_map(|s| ids.get(&s.to_lowercase()).copied())
        .collect()
}

fn count_line_bi(ids: &HashMap<String, u32>, line: &str) -> HashMap<(u32, u32), u32> {
    let t = tokens(ids, line);
    let mut m = HashMap::new();
    for w in t.windows(2) {
        *m.entry((w[0], w[1])).or_insert(0) += 1;
    }
    m
}

fn count_line_tri(ids: &HashMap<String, u32>, line: &str) -> HashMap<(u32, u32, u32), u32> {
    let t = tokens(ids, line);
    let mut m = HashMap::new();
    for w in t.windows(3) {
        *m.entry((w[0], w[1], w[2])).or_insert(0) += 1;
    }
    m
}

fn merge_bi(
    map: &mut HashMap<(u32, u32), u32>,
    ids: &HashMap<String, u32>,
    raw: &str,
) -> Result<()> {
    for line in raw.lines() {
        let p: Vec<&str> = line.split_whitespace().collect();
        if p.len() < 3 {
            continue;
        }
        let Some(&a) = ids.get(&p[0].to_lowercase()) else {
            continue;
        };
        let Some(&b) = ids.get(&p[1].to_lowercase()) else {
            continue;
        };
        let c: u32 = p[2].parse().unwrap_or(0);
        *map.entry((a, b)).or_insert(0) += c;
    }
    Ok(())
}

fn merge_tri(
    map: &mut HashMap<(u32, u32, u32), u32>,
    ids: &HashMap<String, u32>,
    raw: &str,
) -> Result<()> {
    for line in raw.lines() {
        let p: Vec<&str> = line.split_whitespace().collect();
        if p.len() < 4 {
            continue;
        }
        let Some(&a) = ids.get(&p[0].to_lowercase()) else {
            continue;
        };
        let Some(&b) = ids.get(&p[1].to_lowercase()) else {
            continue;
        };
        let Some(&c) = ids.get(&p[2].to_lowercase()) else {
            continue;
        };
        let n: u32 = p[3].parse().unwrap_or(0);
        *map.entry((a, b, c)).or_insert(0) += n;
    }
    Ok(())
}
