use std::ops::Range;

pub fn splice(
    text: &str,
    range: Range<usize>,
    replacement: &str,
    insert_space: bool,
) -> (String, usize) {
    let start = range.start.min(text.len());
    let end = range.end.min(text.len());
    let mut out = String::with_capacity(text.len() + replacement.len() + 1);
    out.push_str(&text[..start]);
    out.push_str(replacement);
    if insert_space {
        out.push(' ');
    }
    let cursor = out.len();
    out.push_str(&text[end..]);
    (out, cursor)
}

/// Bytes of `candidate` after a case-insensitive match of `token` as a prefix.
pub fn remainder(token: &str, candidate: &str) -> String {
    let t: Vec<char> = token.chars().collect();
    let c: Vec<char> = candidate.chars().collect();
    if t.len() > c.len() {
        return String::new();
    }
    let matches_prefix = t
        .iter()
        .zip(c.iter())
        .all(|(a, b)| a.eq_ignore_ascii_case(b) || a.to_lowercase().eq(b.to_lowercase()));
    if !matches_prefix {
        return String::new();
    }
    c[t.len()..].iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splice_replaces_range_and_optional_space() {
        let (out, cur) = splice("hello world", 0..5, "hey", false);
        assert_eq!(out, "hey world");
        assert_eq!(cur, 3);

        let (out, cur) = splice("hel", 0..3, "hello", true);
        assert_eq!(out, "hello ");
        assert_eq!(cur, 6);
    }

    #[test]
    fn splice_mid_word_clobbers_suffix() {
        let (out, cur) = splice("hello", 0..5, "help", false);
        assert_eq!(out, "help");
        assert_eq!(cur, 4);
    }

    #[test]
    fn remainder_hel_hello_is_lo() {
        assert_eq!(remainder("hel", "hello"), "lo");
        assert_eq!(remainder("HEL", "HELLO"), "LO");
        assert_eq!(remainder("Hel", "Hello"), "lo");
    }

    #[test]
    fn remainder_mismatch_is_empty() {
        assert_eq!(remainder("xyz", "hello"), "");
    }
}
