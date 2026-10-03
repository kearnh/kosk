use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::{self, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{bail, ensure, Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};

const CACHE_VERSION: u32 = 2;
const STANDARD_STREAM: &str = "-";

#[derive(Parser)]
#[command(about = "Inspect and edit KOSK completion caches as JSON")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Export a binary cache as JSON. Filters combine with AND.
    Export {
        input: PathBuf,
        #[command(flatten)]
        filter: Filter,
        /// Write JSON to a file instead of stdout.
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Emit compact JSON.
        #[arg(long)]
        compact: bool,
    },
    /// Validate JSON and write a binary cache. Quit KOSK before replacing its cache.
    Import {
        /// JSON file, or '-' for stdin.
        #[arg(default_value = STANDARD_STREAM)]
        input: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
        /// Replace an existing cache, preserving it as <output>.bak.
        #[arg(long)]
        replace: bool,
        /// Validate without writing or creating a backup.
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Clone, Copy, Default, ValueEnum)]
enum Kind {
    #[default]
    All,
    Unigram,
    Bigram,
}

#[derive(Clone, Copy, Default, ValueEnum)]
enum Sort {
    #[default]
    Word,
    Count,
    Time,
}

#[derive(Args, Default)]
struct Filter {
    /// Include these application types (repeatable; '*' is the catch-all).
    #[arg(long = "type")]
    types: Vec<String>,
    #[arg(long, value_enum, default_value = "all")]
    kind: Kind,
    /// Exact word match; matches either word of a bigram.
    #[arg(long)]
    word: Option<String>,
    /// Word prefix; matches either word of a bigram.
    #[arg(long)]
    prefix: Option<String>,
    /// Word substring; matches either word of a bigram.
    #[arg(long)]
    contains: Option<String>,
    /// Exact previous word; excludes unigrams.
    #[arg(long)]
    previous: Option<String>,
    #[arg(long)]
    min_count: Option<f32>,
    #[arg(long)]
    max_count: Option<f32>,
    /// Inclusive last-seen Unix timestamp in seconds.
    #[arg(long)]
    since: Option<u64>,
    /// Inclusive last-seen Unix timestamp in seconds.
    #[arg(long)]
    until: Option<u64>,
    /// Ascending order within each bucket and kind; ties use word order.
    #[arg(long, value_enum, default_value = "word")]
    sort: Sort,
    /// Reverse the selected sort order (e.g. highest counts first).
    #[arg(long)]
    reverse: bool,
    /// Maximum entries per application type and kind, after sorting.
    #[arg(long)]
    limit: Option<usize>,
}

// Field order matches Persist/PersistBucket in completion/user_cache.rs.
// Postcard encodes these structs identically to the cache's tuples.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cache {
    version: u32,
    buckets: Vec<Bucket>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Bucket {
    app_type: String,
    unigrams: Vec<Unigram>,
    bigrams: Vec<Bigram>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Unigram {
    word: String,
    count: f32,
    last_seen: u64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Bigram {
    previous_word: String,
    word: String,
    count: f32,
    last_seen: u64,
}

impl Cache {
    fn decode(bytes: &[u8]) -> Result<Self> {
        let (cache, remainder): (Self, _) =
            postcard::take_from_bytes(bytes).context("decode completion cache")?;
        ensure!(remainder.is_empty(), "cache contains trailing bytes");
        cache.validate()?;
        Ok(cache)
    }

    fn validate(&self) -> Result<()> {
        ensure!(
            self.version == CACHE_VERSION,
            "unsupported cache version {}",
            self.version
        );
        let mut types = HashSet::new();
        for bucket in &self.buckets {
            ensure!(!bucket.app_type.is_empty(), "empty application type");
            ensure!(
                types.insert(&bucket.app_type),
                "duplicate application type: {}",
                bucket.app_type
            );
            let mut words = HashSet::new();
            for entry in &bucket.unigrams {
                validate_entry(&entry.word, entry.count)?;
                ensure!(
                    words.insert(&entry.word),
                    "duplicate unigram: {}",
                    entry.word
                );
            }
            let mut pairs = HashSet::new();
            for entry in &bucket.bigrams {
                validate_entry(&entry.previous_word, entry.count)?;
                validate_entry(&entry.word, entry.count)?;
                ensure!(
                    pairs.insert((&entry.previous_word, &entry.word)),
                    "duplicate bigram: {} {}",
                    entry.previous_word,
                    entry.word
                );
            }
        }
        Ok(())
    }

    fn filter(&mut self, filter: &Filter) -> Result<()> {
        filter.validate()?;
        self.buckets
            .retain(|b| filter.types.is_empty() || filter.types.contains(&b.app_type));
        self.buckets.sort_by(|a, b| a.app_type.cmp(&b.app_type));
        for bucket in &mut self.buckets {
            bucket.unigrams.retain(|e| {
                !matches!(filter.kind, Kind::Bigram)
                    && filter.previous.is_none()
                    && filter.matches(&[&e.word], e.count, e.last_seen)
            });
            bucket.bigrams.retain(|e| {
                !matches!(filter.kind, Kind::Unigram)
                    && filter
                        .previous
                        .as_ref()
                        .is_none_or(|p| p == &e.previous_word)
                    && filter.matches(&[&e.previous_word, &e.word], e.count, e.last_seen)
            });
            bucket.unigrams.sort_by(|a, b| {
                filter
                    .compare(a.count, a.last_seen, b.count, b.last_seen)
                    .then_with(|| a.word.cmp(&b.word))
            });
            bucket.bigrams.sort_by(|a, b| {
                filter
                    .compare(a.count, a.last_seen, b.count, b.last_seen)
                    .then_with(|| (&a.previous_word, &a.word).cmp(&(&b.previous_word, &b.word)))
            });
            if filter.reverse {
                bucket.unigrams.reverse();
                bucket.bigrams.reverse();
            }
            if let Some(limit) = filter.limit {
                bucket.unigrams.truncate(limit);
                bucket.bigrams.truncate(limit);
            }
        }
        Ok(())
    }
}

fn validate_entry(word: &str, count: f32) -> Result<()> {
    ensure!(
        !word.is_empty() && !word.chars().any(char::is_whitespace),
        "invalid word: {word:?}"
    );
    ensure!(
        count.is_finite() && count >= 0.0,
        "invalid count for {word:?}: {count}"
    );
    Ok(())
}

impl Filter {
    fn validate(&self) -> Result<()> {
        for count in [self.min_count, self.max_count].into_iter().flatten() {
            ensure!(
                count.is_finite() && count >= 0.0,
                "count bounds must be finite and nonnegative"
            );
        }
        if let (Some(min), Some(max)) = (self.min_count, self.max_count) {
            ensure!(min <= max, "min-count exceeds max-count");
        }
        if let (Some(since), Some(until)) = (self.since, self.until) {
            ensure!(since <= until, "since exceeds until");
        }
        Ok(())
    }

    fn matches(&self, words: &[&str], count: f32, time: u64) -> bool {
        self.word
            .as_ref()
            .is_none_or(|v| words.contains(&v.as_str()))
            && self
                .prefix
                .as_ref()
                .is_none_or(|v| words.iter().any(|w| w.starts_with(v)))
            && self
                .contains
                .as_ref()
                .is_none_or(|v| words.iter().any(|w| w.contains(v)))
            && self.min_count.is_none_or(|v| count >= v)
            && self.max_count.is_none_or(|v| count <= v)
            && self.since.is_none_or(|v| time >= v)
            && self.until.is_none_or(|v| time <= v)
    }

    fn compare(&self, a_count: f32, a_time: u64, b_count: f32, b_time: u64) -> std::cmp::Ordering {
        match self.sort {
            Sort::Word => std::cmp::Ordering::Equal,
            Sort::Count => a_count.total_cmp(&b_count),
            Sort::Time => a_time.cmp(&b_time),
        }
    }
}

fn read_json(path: &Path) -> Result<Cache> {
    let mut bytes = Vec::new();
    if path == Path::new(STANDARD_STREAM) {
        io::stdin().lock().read_to_end(&mut bytes)?;
    } else {
        bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    }
    let cache: Cache = serde_json::from_slice(&bytes).context("parse cache JSON")?;
    cache.validate()?;
    Ok(cache)
}

fn write_cache(path: &Path, bytes: &[u8], replace: bool) -> Result<()> {
    if path.exists() {
        ensure!(
            replace,
            "{} exists; use --replace to save a backup and replace it",
            path.display()
        );
        let mut backup = path.as_os_str().to_os_string();
        backup.push(".bak");
        let backup = PathBuf::from(backup);
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&backup)
            .with_context(|| {
                format!(
                    "create backup {}; move any existing backup first",
                    backup.display()
                )
            })?;
        io::copy(&mut fs::File::open(path)?, &mut file)?;
        file.sync_all()?;
    }
    let mut temporary = path.as_os_str().to_os_string();
    temporary.push(".tmp");
    let temporary = PathBuf::from(temporary);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .with_context(|| format!("create {}", temporary.display()))?;
    let result = (|| -> Result<()> {
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        if !replace && path.exists() {
            bail!("output appeared while writing: {}", path.display());
        }
        fs::rename(&temporary, path).with_context(|| format!("replace {}", path.display()))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn run() -> Result<()> {
    match Cli::parse().command {
        Command::Export {
            input,
            filter,
            output,
            compact,
        } => {
            let bytes = fs::read(&input).with_context(|| format!("read {}", input.display()))?;
            let mut cache = Cache::decode(&bytes)?;
            cache.filter(&filter)?;
            let writer: Box<dyn Write> = match output {
                Some(path) if path != Path::new(STANDARD_STREAM) => Box::new(
                    OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&path)?,
                ),
                _ => Box::new(io::stdout().lock()),
            };
            let mut writer = BufWriter::new(writer);
            if compact {
                serde_json::to_writer(&mut writer, &cache)?;
            } else {
                serde_json::to_writer_pretty(&mut writer, &cache)?;
            }
            writeln!(writer)?;
            writer.flush()?;
        }
        Command::Import {
            input,
            output,
            replace,
            dry_run,
        } => {
            let cache = read_json(&input)?;
            let bytes = postcard::to_stdvec(&cache)?;
            if !dry_run {
                write_cache(&output, &bytes, replace)?;
            }
            eprintln!(
                "{}: {} buckets, {} bytes",
                if dry_run { "Validated" } else { "Written" },
                cache.buckets.len(),
                bytes.len()
            );
        }
    }
    Ok(())
}

fn main() -> Result<()> {
    run()
}

#[cfg(test)]
mod tests {
    use super::*;
    use kosk::completion::{settings::CompletionUserCacheConfig, UserCache};
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TestFile(PathBuf);

    impl TestFile {
        fn new() -> Self {
            Self(std::env::temp_dir().join(format!(
                "kosk-cache-tool-{}-{}.bin", std::process::id(),
                SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
            )))
        }
    }

    impl Drop for TestFile {
        fn drop(&mut self) {
            for suffix in ["", ".bak", ".tmp"] {
                let mut path = self.0.as_os_str().to_os_string();
                path.push(suffix);
                let _ = fs::remove_file(PathBuf::from(path));
            }
        }
    }

    fn sample() -> Cache {
        Cache {
            version: CACHE_VERSION,
            buckets: vec![Bucket {
                app_type: "*".into(),
                unigrams: vec![
                    Unigram {
                        word: "hello".into(),
                        count: 2.0,
                        last_seen: 100,
                    },
                    Unigram {
                        word: "world".into(),
                        count: 5.0,
                        last_seen: 200,
                    },
                ],
                bigrams: vec![Bigram {
                    previous_word: "hello".into(),
                    word: "world".into(),
                    count: 1.0,
                    last_seen: 150,
                }],
            }],
        }
    }

    #[test]
    fn reads_application_cache_and_application_reads_edited_json() {
        let path = TestFile::new();
        let config = CompletionUserCacheConfig::default();
        let mut application = UserCache::load(&config, path.0.clone());
        application.learn_words("*", &["hello".into(), "world".into()]);
        let exported = Cache::decode(&fs::read(&path.0).unwrap()).unwrap();
        let mut json = serde_json::to_value(&exported).unwrap();
        let bucket = &mut json["buckets"][0];
        assert_eq!(bucket["app_type"], "*");
        assert_eq!(bucket["bigrams"][0]["previous_word"], "hello");
        bucket["bigrams"][0]["count"] = serde_json::json!(7.5);
        bucket["unigrams"]
            .as_array_mut()
            .unwrap()
            .retain(|e| e["word"] != "hello");
        let edited: Cache = serde_json::from_value(json).unwrap();
        edited.validate().unwrap();
        write_cache(&path.0, &postcard::to_stdvec(&edited).unwrap(), true).unwrap();
        let application = UserCache::load(&config, path.0.clone());
        assert_eq!(application.bigram("*", "hello", "world"), 7.5);
        assert_eq!(application.unigram("*", "hello"), 0.0);
        assert_eq!(application.unigram("*", "world"), 1.0);
    }

    #[test]
    fn combined_filters_sort_and_limit() {
        let mut cache = sample();
        cache
            .filter(&Filter {
                types: vec!["*".into()],
                prefix: Some("w".into()),
                contains: Some("orl".into()),
                min_count: Some(1.0),
                max_count: Some(5.0),
                since: Some(150),
                until: Some(200),
                sort: Sort::Count,
                reverse: true,
                limit: Some(1),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(cache.buckets[0].unigrams[0].word, "world");
        assert_eq!(cache.buckets[0].bigrams.len(), 1);
        cache
            .filter(&Filter {
                previous: Some("hello".into()),
                word: Some("world".into()),
                ..Default::default()
            })
            .unwrap();
        assert!(cache.buckets[0].unigrams.is_empty());
        assert_eq!(cache.buckets[0].bigrams.len(), 1);
        cache
            .filter(&Filter {
                kind: Kind::Unigram,
                ..Default::default()
            })
            .unwrap();
        assert!(cache.buckets[0].bigrams.is_empty());
    }

    #[test]
    fn rejects_invalid_documents_and_bounds() {
        let mut cache = sample();
        cache.version += 1;
        assert!(cache.validate().is_err());
        cache.version = CACHE_VERSION;
        cache.buckets[0].unigrams[0].count = f32::NAN;
        assert!(cache.validate().is_err());
        cache.buckets[0].unigrams[0].count = -1.0;
        assert!(cache.validate().is_err());
        cache.buckets[0].unigrams[0].count = 1.0;
        cache.buckets[0].unigrams[1].word = "hello".into();
        assert!(cache.validate().is_err());
        assert!(Filter {
            since: Some(200),
            until: Some(100),
            ..Default::default()
        }
        .validate()
        .is_err());
        assert!(Filter {
            min_count: Some(5.0),
            max_count: Some(1.0),
            ..Default::default()
        }
        .validate()
        .is_err());
        assert!(
            serde_json::from_str::<Cache>(r#"{"version":2,"buckets":[],"typo":true}"#).is_err()
        );
    }

    #[test]
    fn rejects_truncated_or_trailing_binary() {
        let mut bytes = postcard::to_stdvec(&sample()).unwrap();
        assert!(Cache::decode(&bytes[..bytes.len() - 1]).is_err());
        bytes.push(0);
        assert!(Cache::decode(&bytes).is_err());
        assert_eq!(
            postcard::to_stdvec(&Cache {
                version: CACHE_VERSION,
                buckets: vec![]
            })
            .unwrap(),
            [2, 0]
        );
    }

    #[test]
    fn replacement_preserves_backup_and_refuses_existing_backup() {
        let path = TestFile::new();
        write_cache(&path.0, b"original", false).unwrap();
        assert!(write_cache(&path.0, b"edited", false).is_err());
        write_cache(&path.0, b"edited", true).unwrap();
        assert_eq!(fs::read(&path.0).unwrap(), b"edited");
        let mut backup = path.0.as_os_str().to_os_string();
        backup.push(".bak");
        assert_eq!(fs::read(PathBuf::from(backup)).unwrap(), b"original");
        assert!(write_cache(&path.0, b"again", true).is_err());
        assert_eq!(fs::read(&path.0).unwrap(), b"edited");
    }
}
