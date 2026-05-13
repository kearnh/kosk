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

/// Alphanumeric run ending at the cursor: the suffix of `prefix` that is only `[A-Za-z0-9_]`
/// characters, **only if** the character immediately before the cursor is itself alphanumeric.
///
/// If the cursor sits in whitespace (e.g. `"hello "` before the next word), returns `""`.
pub fn word_prefix_token(prefix: &str) -> &str {
    let end = prefix.len();
    if end == 0 {
        return "";
    }
    let last = prefix[..end].chars().next_back().unwrap();
    if !(last.is_alphanumeric() || last == '_') {
        return "";
    }
    let mut start = end;
    while start > 0 {
        let ch = prefix[..start].chars().next_back().unwrap();
        if ch.is_alphanumeric() || ch == '_' {
            start -= ch.len_utf8();
        } else {
            break;
        }
    }
    &prefix[start..end]
}

/// Inputs for [`super::CompletionEngine`]: text before/after the caret and a cap on results.
pub struct CompletionContext<'a> {
    pub prefix: &'a str,
    pub suffix: &'a str,
    pub max_results: usize,
}

impl<'a> CompletionContext<'a> {
    pub fn new(prefix: &'a str, suffix: &'a str, max_results: usize) -> Self {
        Self {
            prefix,
            suffix,
            max_results,
        }
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
        let i = s.len() - 1; // before 'b'
        assert_eq!(split_at_cursor(s, i), Some(("a😀", "b")));
        assert_eq!(split_at_cursor(s, 1), Some(("a", "😀b")));
    }

    #[test]
    fn split_at_cursor_invalid_boundary() {
        let s = "a😀b";
        // Split inside the emoji UTF-8 sequence
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
    fn word_prefix_token_trailing_space_after_token() {
        assert_eq!(word_prefix_token("hello wor "), "");
    }

    #[test]
    fn word_prefix_token_incomplete_word() {
        assert_eq!(word_prefix_token("hello wor"), "wor");
    }

    #[test]
    fn word_prefix_token_middle() {
        assert_eq!(word_prefix_token("hel"), "hel");
    }

    #[test]
    fn word_prefix_token_underscore() {
        assert_eq!(word_prefix_token("snake_case"), "snake_case");
    }
}
