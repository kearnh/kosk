/// Characters completion may insert between typed letters: apostrophe, period,
/// grave (folded to apostrophe on load), and Unicode right single quotation mark.
const INSERTED_PUNCTUATION: &[char] = &['\'', '.', '`', '\u{2019}'];

pub(super) fn fold_apostrophe_marks(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '`' | '\u{2019}' => '\'',
            c => c,
        })
        .collect()
}

pub(super) fn strip_inserted(s: &str) -> String {
    s.chars()
        .filter(|c| !INSERTED_PUNCTUATION.contains(c))
        .collect()
}

/// True when `word` has inserted punctuation and its letters-only form starts
/// with the letters-only token. Exact prefixes return false so the dictionary
/// exact range remains the owner of those hits.
pub(super) fn is_inserted_punct_prefix(token: &str, word: &str) -> bool {
    if token.is_empty() || word.starts_with(token) {
        return false;
    }
    let stripped_word = strip_inserted(word);
    if stripped_word == word {
        return false;
    }
    let stripped_token = strip_inserted(token);
    !stripped_token.is_empty() && stripped_word.starts_with(&stripped_token)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dont_inserts_apostrophe() {
        assert!(is_inserted_punct_prefix("dont", "don't"));
    }

    #[test]
    fn eg_inserts_periods() {
        assert!(is_inserted_punct_prefix("eg", "e.g."));
    }

    #[test]
    fn im_inserts_apostrophe() {
        assert!(is_inserted_punct_prefix("im", "i'm"));
    }

    #[test]
    fn exact_letter_prefix_is_not_inserted_punct() {
        assert!(!is_inserted_punct_prefix("hel", "hello"));
        assert!(!is_inserted_punct_prefix("don", "don't"));
    }

    #[test]
    fn fold_grave_and_right_quote_to_apostrophe() {
        assert_eq!(fold_apostrophe_marks("don`t"), "don't");
        assert_eq!(fold_apostrophe_marks("don’t"), "don't");
    }
}
