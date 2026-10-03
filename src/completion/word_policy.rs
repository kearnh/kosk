use unicode_normalization::{char::is_combining_mark, UnicodeNormalization};

const REJECTED_LETTER_RUN_LENGTH: usize = 3;
const MIXED_CASE_SECRET_MIN_CHARACTERS: usize = 8;
const HEX_SECRET_MIN_CHARACTERS: usize = 16;

#[derive(Clone, Copy)]
pub(super) struct WordPolicy {
    max_characters: usize,
}

impl WordPolicy {
    pub(super) fn new(max_characters: usize) -> Self {
        Self { max_characters }
    }

    pub(super) fn suggestion(self, word: &str) -> bool {
        let normalized: String = word.nfc().collect();
        let letters: Vec<char> = normalized.chars().collect();
        if letters.is_empty() || letters.len() > self.max_characters {
            return false;
        }
        for (index, &letter) in letters.iter().enumerate() {
            if letter.is_alphabetic() && !is_combining_mark(letter) {
                continue;
            }
            if is_combining_mark(letter)
                && letters[..index]
                    .iter()
                    .rev()
                    .find(|c| !is_combining_mark(**c))
                    .is_some_and(|c| c.is_alphabetic())
            {
                continue;
            }
            let internal = matches!(letter, '\'' | '’' | '-' | '‐' | '‑')
                && index > 0
                && index + 1 < letters.len()
                && letters[..index]
                    .iter()
                    .rev()
                    .find(|c| !is_combining_mark(**c))
                    .is_some_and(|c| c.is_alphabetic())
                && letters[index + 1].is_alphabetic();
            if !internal {
                return false;
            }
        }

        let mut previous = None;
        let mut run_length = 0;
        for letter in normalized.chars().flat_map(char::to_lowercase) {
            run_length = if previous == Some(letter) {
                run_length + 1
            } else {
                1
            };
            previous = Some(letter);
            if letter.is_alphabetic() && run_length >= REJECTED_LETTER_RUN_LENGTH {
                return false;
            }
        }
        true
    }

    pub(super) fn learn(self, word: &str) -> bool {
        if !self.suggestion(word) {
            return false;
        }
        let normalized: String = word.nfc().collect();
        let length = normalized.chars().count();
        if length >= HEX_SECRET_MIN_CHARACTERS && normalized.chars().all(|c| c.is_ascii_hexdigit())
        {
            return false;
        }
        let upper = normalized.chars().any(char::is_uppercase);
        let lower = normalized.chars().any(char::is_lowercase);
        let mut letters = normalized.chars();
        let title_case =
            letters.next().is_some_and(char::is_uppercase) && letters.all(|c| !c.is_uppercase());
        length < MIXED_CASE_SECRET_MIN_CHARACTERS || !upper || !lower || title_case
    }

    pub(super) fn current_span(self, text: &str, cursor: usize) -> bool {
        let Some(before) = text.get(..cursor) else {
            return false;
        };
        let Some(after) = text.get(cursor..) else {
            return false;
        };
        let start = before
            .char_indices()
            .rev()
            .find(|(_, c)| c.is_whitespace())
            .map_or(0, |(i, c)| i + c.len_utf8());
        let end = after
            .char_indices()
            .find(|(_, c)| c.is_whitespace())
            .map_or(text.len(), |(i, _)| cursor + i);
        start == end || self.learn(&text[start..end])
    }

    pub(super) fn recordable(self, text: &str) -> bool {
        text.split_whitespace().all(|word| self.learn(word))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spelling_and_password_rules() {
        let policy = WordPolicy::new(16);
        for word in [
            "don't",
            "mother-in-law",
            "l’esprit",
            "你好",
            "Bookkeeper",
            "NASA",
            "école",
            "e\u{301}cole",
            "a\u{330}\u{331}'word",
        ] {
            assert!(policy.learn(word), "{word}");
        }
        for word in [
            "hello!",
            "foo_bar",
            "8gb",
            "123456",
            "two words",
            "-word",
            "word-",
            "two--words",
            "heLlL",
            "ééé",
            "aBcDefGh",
            "deadbeefdeadbeef",
            "\u{345}word",
        ] {
            assert!(!policy.learn(word), "{word}");
        }
        assert!(policy.learn("abcdefghijklmnop"));
        assert!(!policy.learn("abcdefghijklmnopq"));
        assert!(WordPolicy::new(17).learn("abcdefghijklmnopq"));
        assert!(WordPolicy::new(1).learn("e\u{301}"));
    }

    #[test]
    fn current_span_and_recording_preserve_raw_boundaries() {
        let policy = WordPolicy::new(16);
        assert!(!policy.current_span("safe Sunshine!42 next", 13));
        assert!(policy.current_span("safe hello next", 7));
        assert!(policy.current_span("safe ", 5));
        assert!(!policy.recordable("safe hunter2 next"));
        assert!(policy.recordable("safe hello next"));
    }
}
