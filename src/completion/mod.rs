//! Word and next-token prediction for keyboard and text-input modes.
mod app_type;
mod apply;
mod backend;
mod case;
mod context;
mod dictionary;
mod fuzzy;
mod ngram;
mod session;
pub mod settings;
mod typed_log;
mod user_cache;

pub use app_type::{validate_app_types, AppTypeMap, CATCH_ALL_TYPE};
pub use apply::{is_case_insensitive_prefix, remainder, splice};
pub use backend::{
    backend_from_config, load_type_wordlists, Abort, Candidate, CompletionBackend, MatchKind,
    Source,
};
pub use case::restore_case;
pub use context::{split_at_cursor, tokens_in, word_prefix_token, CompletionContext};
pub use dictionary::DictionaryEngine;
pub use ngram::NgramEngine;
pub use session::{ensure, init, with_mut, AcceptOutcome, EatAcceptSpace, Session};
pub use settings::CompletionConfig;
pub use typed_log::LogEvent;
pub use user_cache::{resolve_cache_path, UserCache};

pub use ngram::{
    pack2, pack3, read_count_table, read_unigrams, write_count_table, write_unigrams,
    FORMAT_VERSION, ID_BITS,
};
