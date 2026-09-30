use kosk_config_derive::{config_section, Choice};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq, Choice)]
#[serde(rename_all = "lowercase")]
pub enum CompletionBackendKind {
    #[default]
    #[choice(label = "Smart")]
    Ngram,
    Dictionary,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq, Choice)]
#[serde(rename_all = "lowercase")]
pub enum Preselect {
    #[default]
    None,
    First,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq, Choice)]
#[serde(rename_all = "snake_case")]
pub enum ChipPlacement {
    #[default]
    Between,
    AboveField,
    AboveKeyboard,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq, Choice)]
#[serde(rename_all = "lowercase")]
pub enum ChipWidth {
    #[default]
    Fill,
    Hug,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq, Choice)]
#[serde(rename_all = "lowercase")]
pub enum ChipLabel {
    #[default]
    Full,
    Remainder,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq, Choice)]
#[serde(rename_all = "snake_case")]
pub enum ArmedDotPlacement {
    #[default]
    #[choice(label = "Leading")]
    ChipsLeading,
    #[choice(label = "Trailing")]
    ChipsTrailing,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq, Choice)]
#[serde(rename_all = "snake_case")]
pub enum AcceptVia {
    #[default]
    Suffix,
    #[choice(label = "Backspace")]
    BackspaceReplace,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq, Choice)]
#[serde(rename_all = "lowercase")]
pub enum CurrentWordChip {
    First,
    #[default]
    Last,
}

/// Word suggestions. Relative paths resolve against the config file directory.
/// Not stored in recordings.
#[config_section]
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct CompletionConfig {
    /// `false` hides the suggestions. `true` keeps reserved slots so key
    /// centres do not jump when suggestions are empty or off.
    #[config(default = true)]
    #[setting(
        label = "Suggestions",
        explain = "Hides the suggestions. The empty slots stay reserved, so the keys do not jump."
    )]
    pub enabled: bool,

    /// Draw suggestions in Keyboard mode (from keys sent to the app).
    #[config(default = true)]
    #[setting(
        label = "On the keyboard",
        explain = "Draw suggestions while typing into another app."
    )]
    pub show_in_keyboard: bool,

    /// Draw suggestions in TextInput (the field being composed).
    #[config(default = true)]
    #[setting(
        label = "In the text field",
        explain = "Draw suggestions on the text-input screen."
    )]
    pub show_in_text_input: bool,

    /// Distance-1 typos: neighbor substitution, adjacent transposition,
    /// omitted/extra key.
    #[config(default = true)]
    #[setting(
        label = "Typo corrections",
        explain = "Offer a word one edit away from what you typed."
    )]
    pub typo_tolerance: bool,

    /// Empty token (after a space): suggest the next word.
    #[config(default = true)]
    #[setting(
        label = "Next word after a space",
        explain = "After a space, suggest a word that often follows the one you just finished."
    )]
    pub suggest_next_word: bool,

    /// Highlight at rest: none = no suggestion highlighted until cycle or
    /// click; first = highlight the top suggestion as soon as the list appears.
    #[setting(
        label = "Highlight at rest",
        explain = "None waits until you cycle or click a suggestion. First highlights the top suggestion as soon as the list appears."
    )]
    pub preselect: Preselect,

    #[setting(section)]
    pub ui: CompletionUiConfig,

    #[setting(section)]
    pub keyboard: CompletionKeyboardConfig,

    /// Append a space after an accepted word.
    #[config(default = true)]
    #[setting(
        label = "Space after accepting",
        explain = "Adds a space right after the word you pick, so you can keep typing the next one.",
        advanced
    )]
    pub insert_space_on_accept: bool,

    /// If a highlighted suggestion is gone after refresh, clear it (or apply
    /// preselect). A suggestion still in the list is kept, even if it moved.
    #[config(default = true)]
    #[setting(
        label = "Restart highlight on refresh",
        explain = "When the suggestion list changes, move the highlight back to the start instead of keeping its place.",
        advanced
    )]
    pub reset_highlight_on_refresh: bool,

    /// Cycle wrap: last → first (and reverse). `false` clamps at the ends.
    #[config(default = true)]
    #[setting(
        label = "Highlight wraps around",
        explain = "Moving past the last suggestion jumps back to the first one, and the other way around.",
        advanced
    )]
    pub highlight_wraps: bool,

    /// Mix accepted suggestion text into the user cache.
    #[config(default = true)]
    #[setting(
        label = "Learn picked words",
        explain = "Remember words you pick so they rank higher next time.",
        advanced
    )]
    pub learn_on_accept: bool,

    /// Mix a submitted line (TextInput) or Enter (Keyboard) into the user cache.
    #[config(default = true)]
    #[setting(
        label = "Learn submitted lines",
        explain = "Remember the words in a line you submit, so they rank higher next time.",
        advanced
    )]
    pub learn_on_submit: bool,

    /// Token letters: unicode alphanumeric vs ASCII only.
    #[config(default = true)]
    #[setting(
        label = "Non-English letters",
        explain = "Treat letters with accents and other alphabets as word characters while typing.",
        advanced
    )]
    pub unicode_letters: bool,

    /// Treat equivalent unicode spellings as the same.
    #[config(default = true)]
    #[setting(
        label = "Unify accented letters",
        explain = "Treat accented letters typed different ways as the same letter when matching words.",
        advanced
    )]
    pub normalize_nfc: bool,

    /// Restore model lowercase to HEL / Hel / sentence-start.
    #[config(default = true)]
    #[setting(
        label = "Fix capitalization",
        explain = "Offer the word with the right capital letter when what you typed is close but for the case.",
        advanced
    )]
    pub capitalization: bool,

    /// Predictor: ngram needs vocab.txt + unigrams.bin in `model_dir`
    /// (from completion_build); otherwise it uses the wordlist.
    #[setting(
        label = "Word source",
        explain = "Where suggestions come from. Smart uses your recent writing; Dictionary uses the plain word list.",
        advanced
    )]
    pub backend: CompletionBackendKind,

    /// If the chosen predictor fails to load.
    #[config(default = CompletionBackendKind::Dictionary)]
    #[setting(
        label = "Backup word source",
        explain = "Where suggestions come from when the main source has nothing to offer.",
        advanced
    )]
    pub fallback: CompletionBackendKind,

    /// Suggestion cap. The UI shows at most columns * rows of these.
    #[config(default = 6)]
    #[setting(
        label = "Max suggestions",
        explain = "The most suggestions the list will ever show at once.",
        range = 1..=12,
        step = 1,
        advanced
    )]
    pub max_suggestions: usize,

    /// Ignore tokens shorter than this (characters). 0 allows next-word only.
    #[config(default = 1)]
    #[setting(
        label = "Letters before suggesting",
        explain = "How many letters you must type before any suggestions appear. 0 shows them right away.",
        range = 0..=5,
        step = 1,
        advanced
    )]
    pub min_prefix_len: usize,

    /// Wait this long after typing before refreshing suggestions (ms).
    #[config(default = 16)]
    #[setting(
        label = "Suggestion wait",
        explain = "How long to wait after you stop typing before the list refreshes. Raise it if the list flickers.",
        range = 0..=200,
        step = 5,
        unit = "ms",
        advanced
    )]
    pub debounce_ms: u64,

    /// Min token length (characters) for substitution / omission / extra-key.
    /// Transposition still applies at length 2.
    #[config(default = 3)]
    #[setting(
        label = "Shortest typo fix",
        explain = "Words shorter than this never get typo corrections, only exact matches.",
        range = 2..=8,
        step = 1,
        advanced
    )]
    pub min_fuzzy_len: usize,

    /// Show an unknown typed token as a suggestion; accepting adds a space
    /// and learns it.
    #[setting(
        label = "Current word position",
        explain = "Which end of the suggestion row shows the word you are typing right now.",
        advanced
    )]
    pub current_word_chip: CurrentWordChip,

    /// If true, only transpose keys that are layout neighbors.
    /// `false` also allows wider swaps (e.g. wehn → when).
    #[setting(
        label = "Only next-door swaps",
        explain = "Only suggest a word when the two swapped letters sit right next to each other. Off also allows wider swaps.",
        advanced
    )]
    pub transpose_neighbors_only: bool,

    /// Next key after accept: delete that space if the key is in this set.
    /// Empty string disables.
    #[config(default = ",.!?;:)]}'\"/".to_owned())]
    pub eat_space_before: String,

    /// After eating, insert a space after the mark if it is in this set.
    /// Must also be in `eat_space_before`. Empty string never re-spaces.
    #[config(default = ",.!?;:".to_owned())]
    pub space_after: String,

    /// Extra characters that count as inside a word (apostrophe, underscore).
    #[config(default = "_'".to_owned())]
    pub extra_word_chars: String,

    /// App types: free names, exclusive exe basenames, optional wordlist TSV.
    /// Processes not listed use an implicit catch-all.
    pub app_types: HashMap<String, CompletionAppTypeConfig>,

    #[setting(section)]
    pub dictionary: CompletionDictionaryConfig,

    #[setting(section)]
    pub ngram: CompletionNgramConfig,

    #[setting(section)]
    pub user_cache: CompletionUserCacheConfig,
}

#[config_section]
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct CompletionUiConfig {
    /// Keyboard: always above the keys. TextInput: between = field, strip,
    /// keys; above_field = strip, field, keys; above_keyboard = between.
    #[setting(
        label = "Suggestion position",
        explain = "Where the suggestion row sits: between the keys, above the text field, or above the keyboard.",
        advanced
    )]
    pub placement: ChipPlacement,

    /// Suggestions per row, left to right. Extra hits past columns*rows are
    /// dropped. Raise rows (e.g. 2) and max_suggestions together for a
    /// correction beside the completions.
    #[config(default = 3)]
    #[setting(
        label = "Columns",
        explain = "How many suggestions sit on one row. Suggestions past columns times rows are not shown.",
        range = 1..=6,
        step = 1
    )]
    pub columns: usize,

    /// How many suggestion rows to reserve. Use two for a correction beside
    /// the other suggestions.
    #[config(default = 2)]
    #[setting(
        label = "Rows",
        explain = "How many suggestion rows to reserve. Use two when you want a correction beside the other suggestions.",
        range = 1..=3,
        step = 1
    )]
    pub rows: usize,

    /// Allocate the full grid (and on/off dot) even when nothing is suggested.
    #[config(default = true)]
    #[setting(
        label = "Keep empty slots",
        explain = "Leave blank space where suggestions will appear, so the keys do not jump when the list pops in.",
        advanced
    )]
    pub reserve_slots: bool,

    /// fill = equal share of keyboard content width (minus dot + gaps).
    /// hug = size to text, capped by max_chip_width.
    #[setting(
        label = "Suggestion width",
        explain = "Fill stretches each suggestion across its slot. Hug shrinks each one to fit its word.",
        advanced
    )]
    pub chip_width: ChipWidth,

    /// Hug cap. Fill ignores this.
    #[config(default = 200.0)]
    #[setting(
        label = "Widest suggestion",
        explain = "A suggestion never grows wider than this, no matter how long the word is.",
        range = 80.0..=400.0,
        step = 4.0,
        decimals = 0,
        advanced
    )]
    pub max_chip_width: f32,

    /// Floor for both fill and hug.
    #[config(default = 48.0)]
    #[setting(
        label = "Narrowest suggestion",
        explain = "A suggestion never shrinks narrower than this, no matter how short the word is.",
        range = 24.0..=200.0,
        step = 2.0,
        decimals = 0,
        advanced
    )]
    pub min_chip_width: f32,

    #[config(default = 14.0)]
    #[setting(
        label = "Suggestion text size",
        explain = "How big the suggestion words are drawn.",
        range = 10.0..=32.0,
        step = 1.0,
        decimals = 0,
        advanced
    )]
    pub font_size: f32,

    /// Overrides the theme's suggestion roundness.
    pub corner_radius: Option<f32>,

    /// RGBA 0–255.
    pub background_color: Option<[u8; 4]>,

    pub text_color: Option<[u8; 4]>,

    pub selected_background_color: Option<[u8; 4]>,

    pub selected_text_color: Option<[u8; 4]>,

    /// Overrides the theme's selected suggestion outline width.
    pub selected_outline_width: Option<f32>,

    #[config(default = 10.0)]
    #[setting(
        label = "Side padding",
        explain = "Empty space left and right inside each suggestion.",
        range = 0.0..=32.0,
        step = 1.0,
        decimals = 0,
        advanced
    )]
    pub padding_x: f32,

    #[config(default = 4.0)]
    #[setting(
        label = "Top padding",
        explain = "Empty space above and below inside each suggestion.",
        range = 0.0..=16.0,
        step = 1.0,
        decimals = 0,
        advanced
    )]
    pub padding_y: f32,

    #[config(default = 6.0)]
    #[setting(
        label = "Gap between suggestions",
        explain = "Empty space between one suggestion and the next.",
        range = 0.0..=24.0,
        step = 1.0,
        decimals = 0,
        advanced
    )]
    pub gap: f32,

    /// Suggestion text: full word | remainder only (typed prefix stripped).
    #[setting(
        label = "Suggestion text",
        explain = "Show the whole word, or only the ending you have not typed yet.",
        advanced
    )]
    pub label: ChipLabel,

    /// When label=full, paint the already-typed prefix darker.
    #[config(default = true)]
    #[setting(
        label = "Fade typed part",
        explain = "Draw the part of the word you already typed in a dimmer color.",
        advanced
    )]
    pub dim_typed_prefix: bool,

    /// Color of unused reserved slots.
    pub empty_slot_background: Option<[u8; 4]>,

    /// Paint the ranking score on each suggestion.
    #[setting(
        label = "Show scores",
        explain = "Print each suggestion's match score beside it. Useful when tuning, noisy otherwise.",
        advanced
    )]
    pub show_debug_scores: bool,

    /// Green/gray circle on the strip: suggestions on vs off.
    #[config(default = true)]
    #[setting(
        label = "Listening dot",
        explain = "Show a small dot while the keyboard is watching your typing.",
        advanced
    )]
    pub armed_dot: bool,

    #[config(default = 4.0)]
    #[setting(
        label = "Dot size",
        explain = "How big the listening dot is drawn.",
        range = 1.0..=10.0,
        step = 0.5,
        decimals = 1,
        advanced
    )]
    pub armed_dot_radius: f32,

    /// chips_leading | chips_trailing (first row only).
    #[setting(
        label = "Dot position",
        explain = "Which end of the suggestion row the listening dot sits on.",
        advanced
    )]
    pub armed_dot_placement: ArmedDotPlacement,

    pub armed_color: Option<[u8; 4]>,

    pub disarmed_color: Option<[u8; 4]>,

    /// `+` before a current-word suggestion (unknown token).
    pub new_word_mark_color: Option<[u8; 4]>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Default)]
pub struct CompletionAppTypeConfig {
    pub exes: Vec<String>,

    pub wordlist: Option<PathBuf>,
}

#[config_section]
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct CompletionKeyboardConfig {
    /// How accept injects into the focused app: suffix = send the untyped tail
    /// (hel + hello → lo). backspace_replace = delete the token, then send the
    /// word. Non-prefix suggestions (typo corrections) always use
    /// backspace_replace.
    #[setting(
        label = "How a pick is typed",
        explain = "How the picked word replaces what you typed. One way retypes the ending, the other deletes it first.",
        advanced
    )]
    pub accept_via: AcceptVia,

    /// cancelSuggestion undoes the last accept only while that accept is still
    /// the latest edit and no suggestion is highlighted (retypes a replaced
    /// token). `false`: cancel never undoes an accept.
    #[config(default = true)]
    #[setting(
        label = "Allow suggestion cancellation to undo a pick",
        explain = "The cancel suggestion action can restore your original letters immediately after accepting a suggestion.",
        advanced
    )]
    pub retract_last_accept: bool,

    /// Backspace also forgets that character for suggestions.
    #[config(default = true)]
    #[setting(
        label = "Follow backspace",
        explain = "Keep the suggestion list in step when you delete letters.",
        advanced
    )]
    pub track_backspace: bool,

    /// On Enter, forget suggestion context if still on. Does not turn
    /// suggestions back on.
    #[config(default = true)]
    #[setting(
        label = "Clear on enter",
        explain = "Forget what you typed and start fresh after you submit a line.",
        advanced
    )]
    pub clear_on_enter: bool,

    /// Ctrl/Alt letter chords: ignore for suggestions. Do not turn suggestions
    /// off (only paste does).
    #[config(default = true)]
    #[setting(
        label = "Ignore ctrl and alt",
        explain = "Key presses held with ctrl or alt do not disturb the suggestion list.",
        advanced
    )]
    pub ignore_ctrl_alt: bool,

    /// At start: suggestion context empty and suggestions on.
    #[config(default = true)]
    #[setting(
        label = "Listen from the start",
        explain = "Start watching your typing as soon as the keyboard opens, instead of waiting for the first letter.",
        advanced
    )]
    pub start_armed: bool,

    /// Arrow / Home / End / PageUp / PageDown / Insert / Delete: hide
    /// suggestions until toggleCompletion.
    #[config(default = true)]
    #[setting(
        label = "Arrow keys pause listening",
        explain = "Moving the caret with an arrow key stops suggestions until you type again.",
        advanced
    )]
    pub latch_off_on_arrow: bool,

    /// Paste (Ctrl+V): same, hide suggestions until toggleCompletion.
    #[config(default = true)]
    #[setting(
        label = "Pasting pauses listening",
        explain = "Pasted text stops suggestions until you type again.",
        advanced
    )]
    pub latch_off_on_paste: bool,

    /// Turning suggestions back on wipes old context (it is stale).
    #[config(default = true)]
    #[setting(
        label = "Fresh start on wake",
        explain = "Throw away the remembered keystrokes each time listening starts again.",
        advanced
    )]
    pub clear_log_on_arm: bool,

    /// Drop oldest characters from suggestion context past this length.
    #[config(default = 2048)]
    #[setting(
        label = "Longest tracked word",
        explain = "Letters past this many are forgotten while matching. Lower uses less memory.",
        range = 256..=8192,
        step = 256,
        advanced
    )]
    pub max_chars: usize,

    /// 0 = off. Else forget suggestion context after this idle (ms).
    #[setting(
        label = "Forget when idle",
        explain = "Stop listening after this long with no typing. 0 never stops on its own.",
        range = 0..=5000,
        step = 100,
        unit = "ms",
        advanced
    )]
    pub idle_reset_ms: u64,
}

#[config_section]
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct CompletionDictionaryConfig {
    /// word or word<TAB>count. Used when backend is dictionary, and as ngram
    /// fallback when model_dir has no vocab.txt / unigrams.bin.
    #[config(default = PathBuf::from("data/completion/en/unigrams.tsv"))]
    pub wordlist: PathBuf,
}

#[config_section]
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct CompletionNgramConfig {
    /// Packed tables from completion_build: vocab.txt, unigrams.bin, optional
    /// bigrams.bin / trigrams.bin. Without pair tables, next-word stays
    /// you/i/the. See README "Completion next-word setup".
    #[config(default = PathBuf::from("data/completion/en"))]
    pub model_dir: PathBuf,

    /// How many previous words to use (3 for trigrams).
    #[config(default = 3)]
    #[setting(
        label = "Word memory length",
        explain = "How many previous words the smart source looks at. 3 means the last two words shape the next suggestion.",
        range = 1..=5,
        step = 1,
        advanced
    )]
    pub order: u8,

    /// If a longer pattern was never seen, fall back to a shorter one, scaled
    /// by this.
    #[config(default = 0.4)]
    #[setting(
        label = "Trust in longer memory",
        explain = "How much to trust longer word histories over shorter ones. Higher leans on longer histories.",
        range = 0.0..=1.0,
        step = 0.05,
        decimals = 2,
        advanced
    )]
    pub backoff_alpha: f32,

    /// How much the last two words count when ranking.
    #[config(default = 1.0)]
    #[setting(
        label = "Two-word history weight",
        explain = "How much the last two words count when ranking. Higher trusts recent context more.",
        range = 0.0..=2.0,
        step = 0.05,
        decimals = 2,
        advanced
    )]
    pub lambda_trigram: f32,

    /// How much the last word counts when ranking.
    #[config(default = 0.3)]
    #[setting(
        label = "One-word history weight",
        explain = "How much the last word counts when ranking. Higher trusts the previous word more.",
        range = 0.0..=2.0,
        step = 0.05,
        decimals = 2,
        advanced
    )]
    pub lambda_bigram: f32,

    /// How much plain word popularity counts when ranking.
    #[config(default = 0.2)]
    #[setting(
        label = "Common words weight",
        explain = "How much plain word popularity counts when ranking. Higher favors common words.",
        range = 0.0..=2.0,
        step = 0.05,
        decimals = 2,
        advanced
    )]
    pub lambda_unigram: f32,

    /// Extra score for words that match the token letter for letter.
    #[config(default = 0.1)]
    #[setting(
        label = "Exact match bonus",
        explain = "Extra score for words that match what you typed letter for letter.",
        range = 0.0..=2.0,
        step = 0.05,
        decimals = 2,
        advanced
    )]
    pub lambda_exact: f32,

    /// How much to lower the score of a correction vs an exact prefix.
    #[config(default = -2.0)]
    #[setting(
        label = "Typo penalty",
        explain = "How much to lower the score of a word that differs from what you typed. More negative punishes typos harder.",
        range = -5.0..=0.0,
        step = 0.1,
        decimals = 1,
        advanced
    )]
    pub lambda_typo: f32,

    /// Cap how many words a prefix scan considers.
    #[config(default = 8192)]
    #[setting(
        label = "Words scanned",
        explain = "How many dictionary words to scan for each keystroke. Higher finds more, but can feel slower.",
        range = 512..=32768,
        step = 512,
        advanced
    )]
    pub prefix_scan_limit: usize,

    /// How often to check whether a slow search should give up early.
    #[config(default = 64)]
    #[setting(
        label = "Slow-search cutoff",
        explain = "How often to check whether a slow search should give up early. Lower gives up sooner.",
        range = 16..=512,
        step = 16,
        advanced
    )]
    pub abort_check_every: usize,
}

#[config_section]
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct CompletionUserCacheConfig {
    /// Lifetime counts of accepted / submitted words. Novel words complete
    /// without rebuilding the packed tables. File is next to the config if
    /// relative. Cache mass is mixed into dictionary counts so a few accepts
    /// cannot bury high-frequency English; caps drop the rarest entries.
    #[config(default = true)]
    #[setting(
        label = "Remember my words",
        explain = "Keep a file of words you use so they keep ranking higher between sessions.",
        advanced
    )]
    pub enabled: bool,

    #[config(default = PathBuf::from("completion-cache.bin"))]
    pub path: PathBuf,

    /// How many single words the personal file holds. 0 keeps none.
    #[config(default = 20000)]
    #[setting(
        label = "Single words kept",
        explain = "How many single words your personal file holds. 0 keeps none.",
        range = 0..=100000,
        step = 1000,
        advanced
    )]
    pub max_unigrams: usize,

    /// How many two-word pairs the personal file holds. 0 keeps none.
    #[config(default = 50000)]
    #[setting(
        label = "Word pairs kept",
        explain = "How many two-word pairs your personal file holds. 0 keeps none.",
        range = 0..=100000,
        step = 1000,
        advanced
    )]
    pub max_bigrams: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_table_deserializes_defaults() {
        let cfg: CompletionConfig =
            toml::from_str("[completion]\n").unwrap_or_else(|_| toml::from_str("").unwrap());
        let parsed: CompletionConfig = toml::from_str("").unwrap();
        assert_eq!(parsed.backend, CompletionBackendKind::Ngram);
        assert_eq!(parsed.fallback, CompletionBackendKind::Dictionary);
        assert_eq!(parsed.preselect, Preselect::None);
        assert_eq!(parsed.max_suggestions, 6);
        assert_eq!(cfg.max_suggestions, 6);
        assert!(parsed.capitalization);
        assert_eq!(parsed.eat_space_before, ",.!?;:)]}'\"/");
        assert_eq!(parsed.space_after, ",.!?;:");
        assert_eq!(parsed.current_word_chip, CurrentWordChip::Last);
        assert!(parsed.app_types.is_empty());
    }

    #[test]
    fn current_word_chip_first() {
        let parsed: CompletionConfig = toml::from_str("current_word_chip = \"first\"\n").unwrap();
        assert_eq!(parsed.current_word_chip, CurrentWordChip::First);
    }

    #[test]
    fn app_types_table() {
        let parsed: CompletionConfig = toml::from_str(
            r#"
[app_types.browser]
exes = ["firefox.exe", "chrome.exe"]
wordlist = "data/completion/browser.tsv"
"#,
        )
        .unwrap();
        let t = parsed.app_types.get("browser").unwrap();
        assert_eq!(t.exes, vec!["firefox.exe", "chrome.exe"]);
        assert_eq!(
            t.wordlist.as_deref(),
            Some(std::path::Path::new("data/completion/browser.tsv"))
        );
    }
}
