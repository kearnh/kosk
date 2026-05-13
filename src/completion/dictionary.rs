use super::context::word_prefix_token;
use super::engine::{CompletionEngine, Suggestion};
use super::CompletionContext;

/// Exact-prefix completion over a sorted, deduplicated word list.
#[derive(Debug, Clone)]
pub struct DictionaryEngine {
    words: Vec<String>,
}

impl DictionaryEngine {
    /// Build from unsorted lines; empty lines dropped, sorted and deduplicated.
    pub fn from_wordlist_text(text: &str) -> Self {
        let mut words: Vec<String> = text
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(String::from)
            .collect();
        words.sort();
        words.dedup();
        Self { words }
    }

    /// Small built-in list for demos and tests when no file is provided.
    pub fn embedded_demo() -> Self {
        Self::from_wordlist_text(include_str!("test_words.txt"))
    }

    pub fn words(&self) -> &[String] {
        &self.words
    }
}

impl CompletionEngine for DictionaryEngine {
    fn suggest(&self, ctx: &CompletionContext<'_>) -> Vec<Suggestion> {
        let token = word_prefix_token(ctx.prefix);
        if token.is_empty() {
            return Vec::new();
        }
        let max = ctx.max_results.max(1);
        let start = self.words.partition_point(|w| w.as_str() < token);
        self.words[start..]
            .iter()
            .filter(|w| w.starts_with(token))
            .take(max)
            .map(|w| Suggestion { text: w.clone() })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suggest_prefix() {
        let eng = DictionaryEngine::embedded_demo();
        let ctx = CompletionContext::new("hel", "", 8);
        let s = eng.suggest(&ctx);
        let texts: Vec<_> = s.iter().map(|x| x.text.as_str()).collect();
        assert!(texts.contains(&"hello"));
        assert!(texts.contains(&"help"));
    }

    #[test]
    fn suggest_empty_token_returns_empty() {
        let eng = DictionaryEngine::embedded_demo();
        let ctx = CompletionContext::new("   ", "", 8);
        assert!(eng.suggest(&ctx).is_empty());
    }

    #[test]
    fn suggest_respects_max() {
        let eng = DictionaryEngine::from_wordlist_text("aa\nab\nac\nad");
        let ctx = CompletionContext::new("a", "", 2);
        assert_eq!(eng.suggest(&ctx).len(), 2);
    }
}
