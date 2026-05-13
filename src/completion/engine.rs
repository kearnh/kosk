/// One completion candidate (full word to substitute for the current token).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion {
    /// Completed word (typically replaces the alphanumeric token under the cursor).
    pub text: String,
}

use super::CompletionContext;

/// Pluggable completion source for the text-input field.
pub trait CompletionEngine {
    fn suggest(&self, ctx: &CompletionContext<'_>) -> Vec<Suggestion>;
}
