use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CompletionBackendKind {
    #[default]
    Ngram,
    Dictionary,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Preselect {
    #[default]
    None,
    First,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChipPlacement {
    #[default]
    Between,
    AboveField,
    AboveKeyboard,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ChipWidth {
    #[default]
    Fill,
    Hug,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ChipLabel {
    #[default]
    Full,
    Remainder,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArmedDotPlacement {
    #[default]
    ChipsLeading,
    ChipsTrailing,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AcceptVia {
    #[default]
    Suffix,
    BackspaceReplace,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CurrentWordChip {
    First,
    #[default]
    Last,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct CompletionConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,

    #[serde(default = "default_true")]
    pub show_in_keyboard: bool,

    #[serde(default = "default_true")]
    pub show_in_text_input: bool,

    #[serde(default)]
    pub backend: CompletionBackendKind,

    #[serde(default = "default_fallback")]
    pub fallback: CompletionBackendKind,

    #[serde(default = "default_max_suggestions")]
    pub max_suggestions: usize,

    #[serde(default = "default_min_prefix_len")]
    pub min_prefix_len: usize,

    #[serde(default = "default_debounce_ms")]
    pub debounce_ms: u64,

    #[serde(default = "default_true")]
    pub suggest_next_word: bool,

    #[serde(default = "default_true")]
    pub insert_space_on_accept: bool,

    #[serde(default = "default_eat_space_before")]
    pub eat_space_before: String,

    #[serde(default = "default_space_after")]
    pub space_after: String,

    #[serde(default)]
    pub preselect: Preselect,

    #[serde(default = "default_true")]
    pub reset_highlight_on_refresh: bool,

    #[serde(default = "default_true")]
    pub highlight_wraps: bool,

    #[serde(default = "default_true")]
    pub learn_on_accept: bool,

    #[serde(default = "default_true")]
    pub learn_on_submit: bool,

    #[serde(default = "default_true")]
    pub unicode_letters: bool,

    #[serde(default = "default_extra_word_chars")]
    pub extra_word_chars: String,

    #[serde(default = "default_true")]
    pub normalize_nfc: bool,

    #[serde(default = "default_true")]
    pub capitalization: bool,

    #[serde(default = "default_true")]
    pub typo_tolerance: bool,

    #[serde(default = "default_min_fuzzy_len")]
    pub min_fuzzy_len: usize,

    #[serde(default)]
    pub transpose_neighbors_only: bool,

    #[serde(default)]
    pub current_word_chip: CurrentWordChip,

    #[serde(default)]
    pub app_types: HashMap<String, CompletionAppTypeConfig>,

    #[serde(default)]
    pub ui: CompletionUiConfig,

    #[serde(default)]
    pub keyboard: CompletionKeyboardConfig,

    #[serde(default)]
    pub dictionary: CompletionDictionaryConfig,

    #[serde(default)]
    pub ngram: CompletionNgramConfig,

    #[serde(default)]
    pub user_cache: CompletionUserCacheConfig,
}

impl Default for CompletionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            show_in_keyboard: true,
            show_in_text_input: true,
            backend: CompletionBackendKind::default(),
            fallback: CompletionBackendKind::Dictionary,
            max_suggestions: default_max_suggestions(),
            min_prefix_len: default_min_prefix_len(),
            debounce_ms: default_debounce_ms(),
            suggest_next_word: true,
            insert_space_on_accept: true,
            eat_space_before: default_eat_space_before(),
            space_after: default_space_after(),
            preselect: Preselect::None,
            reset_highlight_on_refresh: true,
            highlight_wraps: true,
            learn_on_accept: true,
            learn_on_submit: true,
            unicode_letters: true,
            extra_word_chars: default_extra_word_chars(),
            normalize_nfc: true,
            capitalization: true,
            typo_tolerance: true,
            min_fuzzy_len: default_min_fuzzy_len(),
            transpose_neighbors_only: false,
            current_word_chip: CurrentWordChip::Last,
            app_types: HashMap::new(),
            ui: CompletionUiConfig::default(),
            keyboard: CompletionKeyboardConfig::default(),
            dictionary: CompletionDictionaryConfig::default(),
            ngram: CompletionNgramConfig::default(),
            user_cache: CompletionUserCacheConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct CompletionUiConfig {
    #[serde(default)]
    pub placement: ChipPlacement,

    #[serde(default = "default_columns")]
    pub columns: usize,

    #[serde(default = "default_rows")]
    pub rows: usize,

    #[serde(default = "default_true")]
    pub reserve_slots: bool,

    #[serde(default)]
    pub chip_width: ChipWidth,

    #[serde(default = "default_max_chip_width")]
    pub max_chip_width: f32,

    #[serde(default = "default_min_chip_width")]
    pub min_chip_width: f32,

    #[serde(default = "default_chip_font_size")]
    pub font_size: f32,

    #[serde(default = "default_corner_radius")]
    pub corner_radius: f32,

    #[serde(default = "default_chip_bg")]
    pub background_color: [u8; 4],

    #[serde(default = "default_chip_fg")]
    pub text_color: [u8; 4],

    #[serde(default = "default_chip_sel_bg")]
    pub selected_background_color: [u8; 4],

    #[serde(default = "default_chip_sel_fg")]
    pub selected_text_color: [u8; 4],

    #[serde(default = "default_outline")]
    pub selected_outline_width: f32,

    #[serde(default = "default_pad_x")]
    pub padding_x: f32,

    #[serde(default = "default_pad_y")]
    pub padding_y: f32,

    #[serde(default = "default_gap")]
    pub gap: f32,

    #[serde(default)]
    pub label: ChipLabel,

    #[serde(default = "default_true")]
    pub dim_typed_prefix: bool,

    #[serde(default = "default_empty_slot_bg")]
    pub empty_slot_background: [u8; 4],

    #[serde(default)]
    pub show_debug_scores: bool,

    #[serde(default = "default_true")]
    pub armed_dot: bool,

    #[serde(default = "default_dot_radius")]
    pub armed_dot_radius: f32,

    #[serde(default)]
    pub armed_dot_placement: ArmedDotPlacement,

    #[serde(default = "default_armed_color")]
    pub armed_color: [u8; 4],

    #[serde(default = "default_disarmed_color")]
    pub disarmed_color: [u8; 4],

    #[serde(default = "default_armed_color")]
    pub new_word_mark_color: [u8; 4],
}

impl Default for CompletionUiConfig {
    fn default() -> Self {
        Self {
            placement: ChipPlacement::default(),
            columns: default_columns(),
            rows: default_rows(),
            reserve_slots: true,
            chip_width: ChipWidth::default(),
            max_chip_width: default_max_chip_width(),
            min_chip_width: default_min_chip_width(),
            font_size: default_chip_font_size(),
            corner_radius: default_corner_radius(),
            background_color: default_chip_bg(),
            text_color: default_chip_fg(),
            selected_background_color: default_chip_sel_bg(),
            selected_text_color: default_chip_sel_fg(),
            selected_outline_width: default_outline(),
            padding_x: default_pad_x(),
            padding_y: default_pad_y(),
            gap: default_gap(),
            label: ChipLabel::default(),
            dim_typed_prefix: true,
            empty_slot_background: default_empty_slot_bg(),
            show_debug_scores: false,
            armed_dot: true,
            armed_dot_radius: default_dot_radius(),
            armed_dot_placement: ArmedDotPlacement::default(),
            armed_color: default_armed_color(),
            disarmed_color: default_disarmed_color(),
            new_word_mark_color: default_armed_color(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Default)]
pub struct CompletionAppTypeConfig {
    #[serde(default)]
    pub exes: Vec<String>,

    #[serde(default)]
    pub wordlist: Option<PathBuf>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct CompletionKeyboardConfig {
    #[serde(default)]
    pub accept_via: AcceptVia,

    #[serde(default = "default_true")]
    pub retract_last_accept: bool,

    #[serde(default = "default_true")]
    pub track_backspace: bool,

    #[serde(default = "default_true")]
    pub clear_on_enter: bool,

    #[serde(default = "default_true")]
    pub ignore_ctrl_alt: bool,

    #[serde(default = "default_true")]
    pub start_armed: bool,

    #[serde(default = "default_true")]
    pub latch_off_on_arrow: bool,

    #[serde(default = "default_true")]
    pub latch_off_on_paste: bool,

    #[serde(default = "default_true")]
    pub clear_log_on_arm: bool,

    #[serde(default = "default_max_chars")]
    pub max_chars: usize,

    #[serde(default)]
    pub idle_reset_ms: u64,
}

impl Default for CompletionKeyboardConfig {
    fn default() -> Self {
        Self {
            accept_via: AcceptVia::default(),
            retract_last_accept: true,
            track_backspace: true,
            clear_on_enter: true,
            ignore_ctrl_alt: true,
            start_armed: true,
            latch_off_on_arrow: true,
            latch_off_on_paste: true,
            clear_log_on_arm: true,
            max_chars: default_max_chars(),
            idle_reset_ms: 0,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct CompletionDictionaryConfig {
    #[serde(default = "default_wordlist")]
    pub wordlist: PathBuf,
}

impl Default for CompletionDictionaryConfig {
    fn default() -> Self {
        Self {
            wordlist: default_wordlist(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct CompletionNgramConfig {
    #[serde(default = "default_model_dir")]
    pub model_dir: PathBuf,

    #[serde(default = "default_order")]
    pub order: u8,

    #[serde(default = "default_backoff_alpha")]
    pub backoff_alpha: f32,

    #[serde(default = "default_lambda_tri")]
    pub lambda_trigram: f32,

    #[serde(default = "default_lambda_bi")]
    pub lambda_bigram: f32,

    #[serde(default = "default_lambda_uni")]
    pub lambda_unigram: f32,

    #[serde(default = "default_lambda_user")]
    pub lambda_user: f32,

    #[serde(default = "default_lambda_exact")]
    pub lambda_exact: f32,

    #[serde(default = "default_lambda_typo")]
    pub lambda_typo: f32,

    #[serde(default = "default_prefix_scan_limit")]
    pub prefix_scan_limit: usize,

    #[serde(default = "default_abort_every")]
    pub abort_check_every: usize,
}

impl Default for CompletionNgramConfig {
    fn default() -> Self {
        Self {
            model_dir: default_model_dir(),
            order: default_order(),
            backoff_alpha: default_backoff_alpha(),
            lambda_trigram: default_lambda_tri(),
            lambda_bigram: default_lambda_bi(),
            lambda_unigram: default_lambda_uni(),
            lambda_user: default_lambda_user(),
            lambda_exact: default_lambda_exact(),
            lambda_typo: default_lambda_typo(),
            prefix_scan_limit: default_prefix_scan_limit(),
            abort_check_every: default_abort_every(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct CompletionUserCacheConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,

    #[serde(default = "default_cache_path")]
    pub path: PathBuf,

    #[serde(default = "default_tau")]
    pub decay_tau_hours: f32,

    #[serde(default = "default_max_uni_cache")]
    pub max_unigrams: usize,

    #[serde(default = "default_max_bi_cache")]
    pub max_bigrams: usize,
}

impl Default for CompletionUserCacheConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            path: default_cache_path(),
            decay_tau_hours: default_tau(),
            max_unigrams: default_max_uni_cache(),
            max_bigrams: default_max_bi_cache(),
        }
    }
}

fn default_true() -> bool {
    true
}
fn default_fallback() -> CompletionBackendKind {
    CompletionBackendKind::Dictionary
}
fn default_max_suggestions() -> usize {
    3
}
fn default_min_prefix_len() -> usize {
    1
}
fn default_debounce_ms() -> u64 {
    16
}
fn default_extra_word_chars() -> String {
    "_'".into()
}

fn default_eat_space_before() -> String {
    ",.!?;:)]}'\"/".into()
}

fn default_space_after() -> String {
    ",.!?;:".into()
}

fn default_columns() -> usize {
    3
}
fn default_rows() -> usize {
    1
}
fn default_max_chip_width() -> f32 {
    200.0
}
fn default_min_chip_width() -> f32 {
    48.0
}
fn default_chip_font_size() -> f32 {
    18.0
}
fn default_corner_radius() -> f32 {
    6.0
}
fn default_chip_bg() -> [u8; 4] {
    [40, 40, 40, 255]
}
fn default_chip_fg() -> [u8; 4] {
    [230, 230, 230, 255]
}
fn default_chip_sel_bg() -> [u8; 4] {
    [50, 100, 180, 255]
}
fn default_chip_sel_fg() -> [u8; 4] {
    [255, 255, 255, 255]
}
fn default_outline() -> f32 {
    2.0
}
fn default_pad_x() -> f32 {
    10.0
}
fn default_pad_y() -> f32 {
    4.0
}
fn default_gap() -> f32 {
    6.0
}
fn default_empty_slot_bg() -> [u8; 4] {
    [30, 30, 30, 255]
}
fn default_dot_radius() -> f32 {
    4.0
}
fn default_armed_color() -> [u8; 4] {
    [50, 200, 90, 255]
}
fn default_disarmed_color() -> [u8; 4] {
    [128, 128, 128, 255]
}
fn default_max_chars() -> usize {
    2048
}
fn default_wordlist() -> PathBuf {
    PathBuf::from("data/completion/en/unigrams.tsv")
}
fn default_model_dir() -> PathBuf {
    PathBuf::from("data/completion/en")
}
fn default_order() -> u8 {
    3
}
fn default_backoff_alpha() -> f32 {
    0.4
}
fn default_lambda_tri() -> f32 {
    1.0
}
fn default_lambda_bi() -> f32 {
    0.3
}
fn default_lambda_uni() -> f32 {
    0.2
}
fn default_lambda_user() -> f32 {
    0.8
}
fn default_lambda_exact() -> f32 {
    0.1
}
fn default_lambda_typo() -> f32 {
    -2.0
}
fn default_min_fuzzy_len() -> usize {
    3
}
fn default_prefix_scan_limit() -> usize {
    8192
}
fn default_abort_every() -> usize {
    64
}
fn default_cache_path() -> PathBuf {
    PathBuf::from("completion-cache.bin")
}
fn default_tau() -> f32 {
    2.0
}
fn default_max_uni_cache() -> usize {
    20000
}
fn default_max_bi_cache() -> usize {
    50000
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
        assert_eq!(parsed.max_suggestions, 3);
        assert_eq!(cfg.max_suggestions, 3);
        assert!(parsed.capitalization);
        assert_eq!(parsed.eat_space_before, default_eat_space_before());
        assert_eq!(parsed.space_after, default_space_after());
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
