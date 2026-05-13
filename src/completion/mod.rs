//! Word completion for the text-input state (engine only; UI integration later).
mod context;
mod dictionary;
mod engine;

pub use context::{split_at_cursor, word_prefix_token, CompletionContext};
pub use dictionary::DictionaryEngine;
pub use engine::{CompletionEngine, Suggestion};
