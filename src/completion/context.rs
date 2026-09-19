use std::collections::HashMap;
use std::ops::Range;

use unicode_normalization::UnicodeNormalization;
use unicode_segmentation::UnicodeSegmentation;

use super::settings::CompletionConfig;

/// Split `text` at `cursor_byte`. Returns `None` if `cursor_byte` is not on a UTF-8 scalar boundary.
pub fn split_at_cursor(text: &str, cursor_byte: usize) -> Option<(&str, &str)> {
    if cursor_byte > text.len() {
        return None;
    }
    if !text.is_char_boundary(cursor_byte) {
        return None;
    }
    Some((&text[..cursor_byte], &text[cursor_byte..]))
}

pub fn is_word_char(ch: char, unicode_letters: bool, extra: &str) -> bool {
    if unicode_letters {
        if ch.is_alphanumeric() {
            return true;
        }
    } else if ch.is_ascii_alphanumeric() {
        return true;
    }
    extra.chars().any(|e| e == ch)
}

fn word_run_end(s: &str, unicode_letters: bool, extra: &str) -> usize {
    let mut end = 0;
    for (i, ch) in s.char_indices() {
        if is_word_char(ch, unicode_letters, extra) {
            end = i + ch.len_utf8();
        } else {
            break;
        }
    }
    end
}

fn word_run_start(s: &str, unicode_letters: bool, extra: &str) -> usize {
    let mut start = s.len();
    for (i, ch) in s.char_indices().rev() {
        if is_word_char(ch, unicode_letters, extra) {
            start = i;
        } else {
            break;
        }
    }
    start
}

/// Owned snapshot sent to a backend (and across the predictor thread).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionContext {
    pub prefix: String,
    pub suffix: String,
    pub token: String,
    pub token_range: Range<usize>,
    pub prev_words: Vec<String>,
    pub max_results: usize,
    pub capitalize_sentence: bool,
    pub neighbors: HashMap<char, Vec<char>>,
}

impl CompletionContext {
    pub fn from_buffer(text: &str, cursor_byte: usize, cfg: &CompletionConfig) -> Option<Self> {
        let (prefix, suffix) = split_at_cursor(text, cursor_byte)?;
        let extra = cfg.extra_word_chars.as_str();
        let ul = cfg.unicode_letters;

        let pre_start = word_run_start(prefix, ul, extra);
        let suf_end = word_run_end(suffix, ul, extra);
        let token_range = pre_start..cursor_byte + suf_end;
        let token = text[token_range.clone()].to_string();

        let before_token = &text[..token_range.start];
        let prev_words = prev_words(
            before_token,
            cfg.ngram.order.saturating_sub(1) as usize,
            ul,
            extra,
            cfg.normalize_nfc,
        );

        let capitalize_sentence = cfg.capitalization && sentence_start(before_token);

        Some(Self {
            prefix: prefix.to_string(),
            suffix: suffix.to_string(),
            token,
            token_range,
            prev_words,
            max_results: cfg.max_suggestions.max(1),
            capitalize_sentence,
            neighbors: HashMap::new(),
        })
    }
}

fn sentence_start(before: &str) -> bool {
    let trimmed = before.trim_end();
    if trimmed.is_empty() {
        return true;
    }
    matches!(trimmed.chars().next_back(), Some('.' | '!' | '?'))
}

fn prev_words(
    before_token: &str,
    n: usize,
    unicode_letters: bool,
    extra: &str,
    nfc: bool,
) -> Vec<String> {
    if n == 0 {
        return Vec::new();
    }
    let mut words: Vec<String> = Vec::new();
    for word in before_token.unicode_words() {
        if word
            .chars()
            .any(|c| is_word_char(c, unicode_letters, extra))
        {
            words.push(normalize_word(word, nfc));
        }
    }
    if words.len() > n {
        words.drain(0..words.len() - n);
    }
    words
}

pub fn tokens_in(text: &str, cfg: &CompletionConfig) -> Vec<String> {
    prev_words(
        text,
        usize::MAX,
        cfg.unicode_letters,
        &cfg.extra_word_chars,
        cfg.normalize_nfc,
    )
}

pub fn normalize_word(word: &str, nfc: bool) -> String {
    let lower = word.to_lowercase();
    if nfc {
        lower.nfc().collect()
    } else {
        lower
    }
}

/// Alphanumeric (plus configured extras) run ending at `prefix`. Empty if the last char is not a word char.
pub fn word_prefix_token(prefix: &str) -> &str {
    word_prefix_token_ex(prefix, true, "_")
}

pub fn word_prefix_token_ex<'a>(prefix: &'a str, unicode_letters: bool, extra: &str) -> &'a str {
    let start = word_run_start(prefix, unicode_letters, extra);
    if start == prefix.len() {
        ""
    } else {
        &prefix[start..]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_at_cursor_ascii() {
        let s = "hello";
        assert_eq!(split_at_cursor(s, 0), Some(("", "hello")));
        assert_eq!(split_at_cursor(s, 5), Some(("hello", "")));
        assert_eq!(split_at_cursor(s, 3), Some(("hel", "lo")));
    }

    #[test]
    fn split_at_cursor_emoji() {
        let s = "a😀b";
        let i = s.len() - 1;
        assert_eq!(split_at_cursor(s, i), Some(("a😀", "b")));
        assert_eq!(split_at_cursor(s, 1), Some(("a", "😀b")));
    }

    #[test]
    fn split_at_cursor_invalid_boundary() {
        let s = "a😀b";
        assert!(split_at_cursor(s, 2).is_none());
    }

    #[test]
    fn split_at_cursor_out_of_range() {
        assert!(split_at_cursor("x", 2).is_none());
    }

    #[test]
    fn word_prefix_token_after_completed_word_and_space() {
        assert_eq!(word_prefix_token("hello "), "");
    }

    #[test]
    fn word_prefix_token_incomplete_word() {
        assert_eq!(word_prefix_token("hello wor"), "wor");
    }

    #[test]
    fn word_prefix_token_underscore() {
        assert_eq!(word_prefix_token("snake_case"), "snake_case");
    }

    #[test]
    fn apostrophe_is_word_char() {
        let cfg = CompletionConfig::default();
        let ctx = CompletionContext::from_buffer("don't", 5, &cfg).unwrap();
        assert_eq!(ctx.token, "don't");
    }

    #[test]
    fn mid_word_token_includes_suffix() {
        let cfg = CompletionConfig::default();
        let text = "hello";
        let ctx = CompletionContext::from_buffer(text, 2, &cfg).unwrap();
        assert_eq!(ctx.token, "hello");
        assert_eq!(ctx.token_range, 0..5);
    }

    #[test]
    fn space_yields_empty_token_and_prev_word() {
        let cfg = CompletionConfig::default();
        let ctx = CompletionContext::from_buffer("the cat ", 8, &cfg).unwrap();
        assert_eq!(ctx.token, "");
        assert_eq!(ctx.prev_words, vec!["the".to_string(), "cat".to_string()]);
    }

    #[test]
    fn punctuation_ends_token() {
        let cfg = CompletionConfig::default();
        let ctx = CompletionContext::from_buffer("hello, wor", 10, &cfg).unwrap();
        assert_eq!(ctx.token, "wor");
        assert_eq!(ctx.prev_words, vec!["hello".to_string()]);
    }
}
