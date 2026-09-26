use crate::completion::settings::{
    AcceptVia, ArmedDotPlacement, ChipLabel, ChipPlacement, ChipWidth, CompletionBackendKind,
    CurrentWordChip, Preselect,
};
use crate::config::Config;
use crate::config::{Debug, HapticIntensity, ReachOverlay};
use crate::controller::ControllerKind;
use crate::state::StateId;

const HUB_MOVE: usize = 0;
const HUB_MAPPINGS: usize = 1;
const HUB_LAYOUTS: usize = 2;
const HUB_OPTIONS: usize = 3;
const HUB_LEN: usize = 5;

const OPACITY_STEP: f32 = 0.05;
const KEYBOARD_OPACITY_MIN: f32 = 0.2;
const MENU_OPACITY_MIN: f32 = 0.4;
const OPACITY_MAX: f32 = 1.0;
const OPACITY_DIGITS: u32 = 2;

const KEY_SCALE_MIN: f32 = 16.0;
const KEY_SCALE_MAX: f32 = 48.0;
const KEY_SCALE_STEP: f32 = 1.0;

const DEBOUNCE_MAX: u64 = 500;
const DEBOUNCE_STEP: u64 = 10;
const REPEAT_MAX: u64 = 200;
const REPEAT_STEP: u64 = 5;

const STICK_RANGE_MIN: f32 = 1.0;
const STICK_RANGE_MAX: f32 = 8.0;
const STICK_RANGE_STEP: f32 = 0.1;
const STICK_RANGE_DIGITS: u32 = 1;

const UNIT_MIN: f32 = 0.0;
const UNIT_MAX: f32 = 1.0;
const UNIT_STEP: f32 = 0.05;
const UNIT_DIGITS: u32 = 2;

const STICKY_MIN: f32 = 1.0;
const STICKY_MAX: f32 = 2.0;

const LOCK_MAX: u64 = 300;
const LOCK_STEP: u64 = 10;

const TRIGGER_MAX: u8 = 255;
const TRIGGER_STEP: u8 = 5;

const COLUMNS_MIN: usize = 1;
const COLUMNS_MAX: usize = 6;
const ROWS_MIN: usize = 1;
const ROWS_MAX: usize = 3;

const MAX_VISIBLE_ROWS: usize = 8;

const COUNT_MIN: usize = 0;
const COUNT_STEP: usize = 1;
const SUGGESTIONS_MIN: usize = 1;
const SUGGESTIONS_MAX: usize = 12;
const PREFIX_MIN: usize = 0;
const PREFIX_MAX: usize = 5;
const FUZZY_MIN: usize = 2;
const FUZZY_MAX: usize = 8;
const COMPLETE_DEBOUNCE_MAX: u64 = 200;
const COMPLETE_DEBOUNCE_STEP: u64 = 5;

const MAX_CHARS_MIN: usize = 256;
const MAX_CHARS_MAX: usize = 8192;
const MAX_CHARS_STEP: usize = 256;
const IDLE_RESET_MAX: u64 = 5000;
const IDLE_RESET_STEP: u64 = 100;

const CHIP_WIDTH_MAX: f32 = 400.0;
const CHIP_WIDTH_MIN: f32 = 80.0;
const CHIP_WIDTH_STEP: f32 = 4.0;
const CHIP_MIN_MIN: f32 = 24.0;
const CHIP_MIN_MAX: f32 = 200.0;
const CHIP_MIN_STEP: f32 = 2.0;
const FONT_MIN: f32 = 10.0;
const FONT_MAX: f32 = 32.0;
const FONT_STEP: f32 = 1.0;
const RADIUS_MAX: f32 = 16.0;
const OUTLINE_MAX: f32 = 6.0;
const OUTLINE_STEP: f32 = 0.5;
const PAD_X_MAX: f32 = 32.0;
const PAD_Y_MAX: f32 = 16.0;
const GAP_MAX: f32 = 24.0;
const DOT_RADIUS_MIN: f32 = 1.0;
const DOT_RADIUS_MAX: f32 = 10.0;
const DOT_RADIUS_STEP: f32 = 0.5;

const CACHE_MAX: usize = 100_000;
const CACHE_STEP: usize = 1000;

const NGRAM_ORDER_MIN: u8 = 1;
const NGRAM_ORDER_MAX: u8 = 5;
const WEIGHT_MAX: f32 = 2.0;
const TYPO_WEIGHT_MIN: f32 = -5.0;
const SCAN_MIN: usize = 512;
const SCAN_MAX: usize = 32_768;
const SCAN_STEP: usize = 512;
const ABORT_MIN: usize = 16;
const ABORT_MAX: usize = 512;
const ABORT_STEP: usize = 16;

const SETTLE_MAX: u64 = 100;
const SETTLE_STEP: u64 = 5;
const GAIN_MAX: f32 = 3.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::state) enum View {
    Hub,
    Index,
    DeviceIndex(ControllerKind),
    Page(Page),
    DevicePage(ControllerKind, DevicePage),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::state) enum OptionEntry {
    Page(Page),
    Device(ControllerKind),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::state) enum DevicePage {
    Pads,
    Stick,
    Triggers,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::state) enum Page {
    Suggestions,
    Overlay,
    Typing,
    Debug,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::state) enum RowId {
    Suggestions,
    OnKeyboard,
    InTextField,
    Typos,
    NextWord,
    HighlightAtRest,
    Columns,
    Rows,
    SeeThrough,
    KeyboardOpacity,
    MenuOpacity,
    KeyWidth,
    KeyHeight,
    DelayBeforeRepeat,
    RepeatInterval,
    PadHorizontalRange,
    PadVerticalRange,
    PadSquare,
    PadStickiness,
    PadHoldAfterKey,
    HorizontalRange,
    VerticalRange,
    SquareStick,
    Stickiness,
    HoldAfterKey,
    ThumbRest,
    StretchShortSide,
    PadClick,
    StretchMaxGain,
    PadSettleMs,
    Sc2TriggerLeft,
    Sc2TriggerRight,
    Ps4TriggerLeft,
    Ps4TriggerRight,
    StickCursors,
    Hitboxes,
    StickBounds,
    ReachOverlay,
    InsertSpace,
    ResetHighlight,
    HighlightWraps,
    LearnOnAccept,
    LearnOnSubmit,
    UnicodeLetters,
    NormalizeNfc,
    Capitalization,
    TransposeNeighbors,
    MaxSuggestions,
    MinPrefixLen,
    CompleteDebounce,
    MinFuzzyLen,
    Backend,
    Fallback,
    CurrentWordChip,
    AcceptVia,
    RetractAccept,
    TrackBackspace,
    ClearOnEnter,
    IgnoreCtrlAlt,
    StartArmed,
    LatchOnArrow,
    LatchOnPaste,
    ClearLogOnArm,
    MaxChars,
    IdleResetMs,
    ReserveSlots,
    ChipWidth,
    ChipPlacement,
    ChipLabel,
    DimTypedPrefix,
    ShowDebugScores,
    ArmedDot,
    ArmedDotPlacement,
    MaxChipWidth,
    MinChipWidth,
    ChipFontSize,
    CornerRadius,
    OutlineWidth,
    PaddingX,
    PaddingY,
    Gap,
    ArmedDotRadius,
    CacheEnabled,
    MaxUnigrams,
    MaxBigrams,
    NgramOrder,
    BackoffAlpha,
    LambdaTri,
    LambdaBi,
    LambdaUni,
    LambdaExact,
    LambdaTypo,
    PrefixScanLimit,
    AbortCheckEvery,
    TextFontSize,
    BatteryButton,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::state) struct Effect {
    pub open: Option<StateId>,
    pub persist: bool,
    pub changed: bool,
}

impl Effect {
    fn none() -> Self {
        Self {
            open: None,
            persist: false,
            changed: false,
        }
    }

    fn open(state: StateId) -> Self {
        Self {
            open: Some(state),
            persist: false,
            changed: false,
        }
    }
}

pub(in crate::state) struct DrawnRow {
    pub label: &'static str,
    pub explain: Option<&'static str>,
    pub value: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::state) enum FooterButtons {
    Activate,
    Back,
    Adjust,
    Page,
    OpenConfig,
    ShowAllControllers,
}

pub(in crate::state) struct FooterHint {
    pub buttons: FooterButtons,
    pub label: &'static str,
}

pub(in crate::state) struct SettingsForm {
    view: View,
    focus: usize,
    dirty: Vec<(RowId, ControllerKind)>,
    show_all_controllers: bool,
}

impl SettingsForm {
    pub(in crate::state) fn new() -> Self {
        Self {
            view: View::Hub,
            focus: 0,
            dirty: Vec::new(),
            show_all_controllers: false,
        }
    }

    pub(in crate::state) fn toggle_show_all(&mut self, kind: ControllerKind) {
        self.show_all_controllers = !self.show_all_controllers;
        self.ensure_focus(kind);
    }

    pub(in crate::state) fn focus(&self) -> usize {
        self.focus
    }

    pub(in crate::state) fn on_page(&self) -> bool {
        matches!(self.view, View::Page(_) | View::DevicePage(..))
    }

    pub(in crate::state) fn set_focus(&mut self, index: usize, kind: ControllerKind) {
        self.focus = index;
        self.ensure_focus(kind);
    }

    pub(in crate::state) fn dirty_rows(&self) -> &[(RowId, ControllerKind)] {
        &self.dirty
    }

    pub(in crate::state) fn clear_dirty(&mut self) {
        self.dirty.clear();
    }

    pub(in crate::state) fn ensure_focus(&mut self, kind: ControllerKind) {
        let n = self.len(kind);
        if n == 0 {
            self.focus = 0;
            return;
        }
        if self.focus >= n {
            self.focus = n - 1;
        }
    }

    pub(in crate::state) fn move_focus(&mut self, delta: i32, kind: ControllerKind) {
        let n = self.len(kind);
        if n == 0 {
            return;
        }
        self.focus = (self.focus as i32 + delta).rem_euclid(n as i32) as usize;
    }

    pub(in crate::state) fn back(&mut self, kind: ControllerKind) -> Effect {
        let persist = !self.dirty.is_empty();
        match self.view {
            View::Hub => Effect {
                open: Some(StateId::Keyboard),
                persist,
                changed: false,
            },
            View::Index => {
                self.view = View::Hub;
                self.focus = HUB_OPTIONS;
                Effect {
                    open: None,
                    persist,
                    changed: false,
                }
            }
            View::DeviceIndex(target) => {
                self.view = View::Index;
                self.focus = device_entry_index(target, kind, self.show_all_controllers);
                Effect {
                    open: None,
                    persist,
                    changed: false,
                }
            }
            View::Page(page) => {
                self.view = View::Index;
                self.focus = page_entry_index(page, kind, self.show_all_controllers);
                Effect {
                    open: None,
                    persist,
                    changed: false,
                }
            }
            View::DevicePage(target, device_page) => {
                self.view = View::DeviceIndex(target);
                self.focus = device_page_index(target, device_page);
                Effect {
                    open: None,
                    persist,
                    changed: false,
                }
            }
        }
    }

    pub(in crate::state) fn activate(&mut self, cfg: &mut Config, kind: ControllerKind) -> Effect {
        self.ensure_focus(kind);
        match self.view {
            View::Hub => match self.focus {
                HUB_MOVE => Effect::open(StateId::MoveWindow),
                HUB_MAPPINGS => Effect::open(StateId::Mappings),
                HUB_LAYOUTS => Effect::open(StateId::SelectLayout),
                HUB_OPTIONS => {
                    self.view = View::Index;
                    self.focus = 0;
                    Effect::none()
                }
                _ => Effect::open(StateId::Keyboard),
            },
            View::Index => {
                let entry = options_entries(kind, self.show_all_controllers)[self.focus];
                match entry {
                    OptionEntry::Page(page) => {
                        self.view = View::Page(page);
                    }
                    OptionEntry::Device(target) => {
                        self.view = View::DeviceIndex(target);
                    }
                }
                self.focus = 0;
                Effect::none()
            }
            View::DeviceIndex(target) => {
                let device_page = device_pages(target)[self.focus];
                self.view = View::DevicePage(target, device_page);
                self.focus = 0;
                Effect::none()
            }
            View::Page(_) | View::DevicePage(..) => {
                let Some((id, _)) = self.focused_edit(kind) else {
                    return Effect::none();
                };
                if !is_toggle(id) {
                    return Effect::none();
                }
                let changed = self.nudge(cfg, 1, kind);
                Effect {
                    open: None,
                    persist: false,
                    changed,
                }
            }
        }
    }

    pub(in crate::state) fn shift_page(&mut self, dir: i32, kind: ControllerKind) -> Effect {
        let current = match self.view {
            View::Page(page) => ValueRef::Page(page),
            View::DevicePage(target, device_page) => ValueRef::Device(target, device_page),
            _ => return Effect::none(),
        };
        let persist = !self.dirty.is_empty();
        let sequence = value_sequence(kind, self.show_all_controllers);
        let n = sequence.len() as i32;
        let position = sequence.iter().position(|v| *v == current).unwrap_or(0);
        let next = sequence[(position as i32 + dir).rem_euclid(n) as usize];
        match next {
            ValueRef::Page(page) => self.view = View::Page(page),
            ValueRef::Device(target, device_page) => {
                self.view = View::DevicePage(target, device_page)
            }
        }
        self.focus = 0;
        Effect {
            open: None,
            persist,
            changed: false,
        }
    }

    pub(in crate::state) fn nudge(
        &mut self,
        cfg: &mut Config,
        dir: i32,
        kind: ControllerKind,
    ) -> bool {
        self.ensure_focus(kind);
        let Some((id, target)) = self.focused_edit(kind) else {
            return false;
        };
        let before = format_value(cfg, id, target);
        apply_dir(cfg, id, dir, target);
        if format_value(cfg, id, target) == before {
            return false;
        }
        if !self.dirty.contains(&(id, target)) {
            self.dirty.push((id, target));
        }
        true
    }

    pub(in crate::state) fn title(&self) -> &'static str {
        match self.view {
            View::Hub => "Settings",
            View::Index => "Options",
            View::DeviceIndex(target) => device_name(target),
            View::Page(page) => page.title(),
            View::DevicePage(target, device_page) => device_page_title(target, device_page),
        }
    }

    pub(in crate::state) fn footer(&self, kind: ControllerKind) -> Vec<FooterHint> {
        match self.view {
            View::Hub => vec![
                FooterHint {
                    buttons: FooterButtons::Activate,
                    label: "open",
                },
                FooterHint {
                    buttons: FooterButtons::Back,
                    label: "keyboard",
                },
            ],
            View::Index => vec![
                FooterHint {
                    buttons: FooterButtons::Activate,
                    label: "open",
                },
                FooterHint {
                    buttons: FooterButtons::Back,
                    label: "back",
                },
                FooterHint {
                    buttons: FooterButtons::ShowAllControllers,
                    label: if self.show_all_controllers {
                        "hide other controllers"
                    } else {
                        "show other controllers"
                    },
                },
                FooterHint {
                    buttons: FooterButtons::OpenConfig,
                    label: "open config",
                },
            ],
            View::DeviceIndex(_) => vec![
                FooterHint {
                    buttons: FooterButtons::Activate,
                    label: "open",
                },
                FooterHint {
                    buttons: FooterButtons::Back,
                    label: "back",
                },
            ],
            View::Page(_) | View::DevicePage(..) => {
                let mut hints = Vec::new();
                if self.focused_edit(kind).is_some_and(|(id, _)| is_toggle(id)) {
                    hints.push(FooterHint {
                        buttons: FooterButtons::Activate,
                        label: "toggle",
                    });
                }
                hints.push(FooterHint {
                    buttons: FooterButtons::Adjust,
                    label: "change",
                });
                hints.push(FooterHint {
                    buttons: FooterButtons::Page,
                    label: "page",
                });
                hints.push(FooterHint {
                    buttons: FooterButtons::Back,
                    label: "back",
                });
                hints
            }
        }
    }

    pub(in crate::state) fn drawn(&self, cfg: &Config, kind: ControllerKind) -> Vec<DrawnRow> {
        match self.view {
            View::Hub => (0..HUB_LEN)
                .map(|i| DrawnRow {
                    label: hub_label(i),
                    explain: None,
                    value: None,
                })
                .collect(),
            View::Index => options_entries(kind, self.show_all_controllers)
                .iter()
                .map(|entry| DrawnRow {
                    label: entry.title(),
                    explain: None,
                    value: None,
                })
                .collect(),
            View::DeviceIndex(target) => device_pages(target)
                .iter()
                .map(|device_page| DrawnRow {
                    label: device_page.title_suffix(),
                    explain: None,
                    value: None,
                })
                .collect(),
            View::Page(page) => page
                .rows()
                .iter()
                .copied()
                .map(|id| {
                    let (label, explain) = row_text(id);
                    DrawnRow {
                        label,
                        explain: Some(explain),
                        value: Some(format_value(cfg, id, kind)),
                    }
                })
                .collect(),
            View::DevicePage(target, device_page) => device_page
                .rows(target)
                .iter()
                .copied()
                .map(|id| {
                    let (label, explain) = row_text(id);
                    DrawnRow {
                        label,
                        explain: Some(explain),
                        value: Some(format_value(cfg, id, target)),
                    }
                })
                .collect(),
        }
    }

    fn len(&self, kind: ControllerKind) -> usize {
        match self.view {
            View::Hub => HUB_LEN,
            View::Index => options_entries(kind, self.show_all_controllers).len(),
            View::DeviceIndex(target) => device_pages(target).len(),
            View::Page(page) => page.rows().len(),
            View::DevicePage(target, device_page) => device_page.rows(target).len(),
        }
    }

    fn focused_edit(&self, kind: ControllerKind) -> Option<(RowId, ControllerKind)> {
        match self.view {
            View::Page(page) => page.rows().get(self.focus).copied().map(|id| (id, kind)),
            View::DevicePage(target, device_page) => device_page
                .rows(target)
                .get(self.focus)
                .copied()
                .map(|id| (id, target)),
            _ => None,
        }
    }

    pub(in crate::state) fn visible_range(&self, kind: ControllerKind) -> (usize, usize) {
        let total = self.len(kind);
        if total <= MAX_VISIBLE_ROWS {
            return (0, total);
        }
        let max_start = total - MAX_VISIBLE_ROWS;
        let start = if self.focus < MAX_VISIBLE_ROWS {
            0
        } else {
            (self.focus + 1 - MAX_VISIBLE_ROWS).min(max_start)
        };
        (start, start + MAX_VISIBLE_ROWS)
    }

    pub(in crate::state) fn scroll_counts(&self, kind: ControllerKind) -> (usize, usize) {
        let (start, end) = self.visible_range(kind);
        (start, self.len(kind).saturating_sub(end))
    }
}

impl Page {
    fn title(self) -> &'static str {
        match self {
            Page::Suggestions => "Suggestions",
            Page::Overlay => "Overlay",
            Page::Typing => "Typing",
            Page::Debug => "Debug",
        }
    }

    fn rows(self) -> &'static [RowId] {
        match self {
            Page::Suggestions => &[
                RowId::Suggestions,
                RowId::OnKeyboard,
                RowId::InTextField,
                RowId::Typos,
                RowId::NextWord,
                RowId::HighlightAtRest,
                RowId::Columns,
                RowId::Rows,
                RowId::InsertSpace,
                RowId::ResetHighlight,
                RowId::HighlightWraps,
                RowId::LearnOnAccept,
                RowId::LearnOnSubmit,
                RowId::UnicodeLetters,
                RowId::NormalizeNfc,
                RowId::Capitalization,
                RowId::TransposeNeighbors,
                RowId::MaxSuggestions,
                RowId::MinPrefixLen,
                RowId::CompleteDebounce,
                RowId::MinFuzzyLen,
                RowId::Backend,
                RowId::Fallback,
                RowId::CurrentWordChip,
                RowId::AcceptVia,
                RowId::RetractAccept,
                RowId::TrackBackspace,
                RowId::ClearOnEnter,
                RowId::IgnoreCtrlAlt,
                RowId::StartArmed,
                RowId::LatchOnArrow,
                RowId::LatchOnPaste,
                RowId::ClearLogOnArm,
                RowId::MaxChars,
                RowId::IdleResetMs,
                RowId::ReserveSlots,
                RowId::ChipWidth,
                RowId::ChipPlacement,
                RowId::ChipLabel,
                RowId::DimTypedPrefix,
                RowId::ShowDebugScores,
                RowId::ArmedDot,
                RowId::ArmedDotPlacement,
                RowId::MaxChipWidth,
                RowId::MinChipWidth,
                RowId::ChipFontSize,
                RowId::CornerRadius,
                RowId::OutlineWidth,
                RowId::PaddingX,
                RowId::PaddingY,
                RowId::Gap,
                RowId::ArmedDotRadius,
                RowId::CacheEnabled,
                RowId::MaxUnigrams,
                RowId::MaxBigrams,
                RowId::NgramOrder,
                RowId::BackoffAlpha,
                RowId::LambdaTri,
                RowId::LambdaBi,
                RowId::LambdaUni,
                RowId::LambdaExact,
                RowId::LambdaTypo,
                RowId::PrefixScanLimit,
                RowId::AbortCheckEvery,
            ],
            Page::Overlay => &[
                RowId::SeeThrough,
                RowId::KeyboardOpacity,
                RowId::MenuOpacity,
                RowId::KeyWidth,
                RowId::KeyHeight,
                RowId::TextFontSize,
                RowId::BatteryButton,
            ],
            Page::Typing => &[RowId::DelayBeforeRepeat, RowId::RepeatInterval],
            Page::Debug => &[
                RowId::StickCursors,
                RowId::Hitboxes,
                RowId::StickBounds,
                RowId::ReachOverlay,
            ],
        }
    }
}

impl OptionEntry {
    fn title(self) -> &'static str {
        match self {
            OptionEntry::Page(page) => page.title(),
            OptionEntry::Device(target) => device_name(target),
        }
    }
}

impl DevicePage {
    fn title_suffix(self) -> &'static str {
        match self {
            DevicePage::Pads => "Pads",
            DevicePage::Stick => "Stick",
            DevicePage::Triggers => "Triggers",
        }
    }

    fn rows(self, target: ControllerKind) -> &'static [RowId] {
        match (self, target) {
            (DevicePage::Pads, _) => &[
                RowId::PadHorizontalRange,
                RowId::PadVerticalRange,
                RowId::PadSquare,
                RowId::PadStickiness,
                RowId::PadHoldAfterKey,
                RowId::ThumbRest,
                RowId::StretchShortSide,
                RowId::PadClick,
                RowId::StretchMaxGain,
                RowId::PadSettleMs,
            ],
            (DevicePage::Stick, _) => &[
                RowId::HorizontalRange,
                RowId::VerticalRange,
                RowId::SquareStick,
                RowId::Stickiness,
                RowId::HoldAfterKey,
            ],
            (DevicePage::Triggers, ControllerKind::Ps4) => {
                &[RowId::Ps4TriggerLeft, RowId::Ps4TriggerRight]
            }
            (DevicePage::Triggers, _) => &[RowId::Sc2TriggerLeft, RowId::Sc2TriggerRight],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ValueRef {
    Page(Page),
    Device(ControllerKind, DevicePage),
}

fn canonical_device(kind: ControllerKind) -> ControllerKind {
    match crate::config::resolved_controller(kind) {
        ControllerKind::Ps4 => ControllerKind::Ps4,
        _ => ControllerKind::Sc2,
    }
}

fn device_name(target: ControllerKind) -> &'static str {
    match target {
        ControllerKind::Ps4 => "DualShock 4",
        _ => "Steam Controller",
    }
}

fn device_page_title(target: ControllerKind, device_page: DevicePage) -> &'static str {
    match (target, device_page) {
        (ControllerKind::Ps4, DevicePage::Stick) => "DualShock 4 Stick",
        (ControllerKind::Ps4, DevicePage::Triggers) => "DualShock 4 Triggers",
        (_, DevicePage::Pads) => "Steam Controller Pads",
        (_, DevicePage::Stick) => "Steam Controller Stick",
        _ => "Steam Controller Triggers",
    }
}

fn visible_devices(kind: ControllerKind, show_all: bool) -> Vec<ControllerKind> {
    if show_all {
        vec![ControllerKind::Sc2, ControllerKind::Ps4]
    } else {
        vec![canonical_device(kind)]
    }
}

fn options_entries(kind: ControllerKind, show_all: bool) -> Vec<OptionEntry> {
    let mut entries = vec![
        OptionEntry::Page(Page::Suggestions),
        OptionEntry::Page(Page::Overlay),
        OptionEntry::Page(Page::Typing),
    ];
    for target in visible_devices(kind, show_all) {
        entries.push(OptionEntry::Device(target));
    }
    entries.push(OptionEntry::Page(Page::Debug));
    entries
}

fn value_sequence(kind: ControllerKind, show_all: bool) -> Vec<ValueRef> {
    let mut sequence = vec![
        ValueRef::Page(Page::Suggestions),
        ValueRef::Page(Page::Overlay),
        ValueRef::Page(Page::Typing),
    ];
    for target in visible_devices(kind, show_all) {
        for device_page in device_pages(target) {
            sequence.push(ValueRef::Device(target, *device_page));
        }
    }
    sequence.push(ValueRef::Page(Page::Debug));
    sequence
}

fn device_pages(target: ControllerKind) -> &'static [DevicePage] {
    match target {
        ControllerKind::Ps4 => &[DevicePage::Stick, DevicePage::Triggers],
        _ => &[DevicePage::Pads, DevicePage::Stick, DevicePage::Triggers],
    }
}

fn page_entry_index(page: Page, kind: ControllerKind, show_all: bool) -> usize {
    options_entries(kind, show_all)
        .iter()
        .position(|entry| *entry == OptionEntry::Page(page))
        .unwrap_or(0)
}

fn device_entry_index(target: ControllerKind, kind: ControllerKind, show_all: bool) -> usize {
    options_entries(kind, show_all)
        .iter()
        .position(|entry| *entry == OptionEntry::Device(target))
        .unwrap_or(0)
}

fn device_page_index(target: ControllerKind, device_page: DevicePage) -> usize {
    device_pages(target)
        .iter()
        .position(|page| *page == device_page)
        .unwrap_or(0)
}

fn hub_label(index: usize) -> &'static str {
    match index {
        HUB_MOVE => "Move window",
        HUB_MAPPINGS => "Mappings",
        HUB_LAYOUTS => "Layouts",
        HUB_OPTIONS => "Options",
        _ => "Back",
    }
}

fn row_text(id: RowId) -> (&'static str, &'static str) {
    match id {
        RowId::Suggestions => (
            "Suggestions",
            "Hides the suggestions. The empty slots stay reserved, so the keys do not jump.",
        ),
        RowId::OnKeyboard => (
            "On the keyboard",
            "Draw suggestions while typing into another app.",
        ),
        RowId::InTextField => (
            "In the text field",
            "Draw suggestions on the text-input screen.",
        ),
        RowId::Typos => (
            "Typo corrections",
            "Offer a word one edit away from what you typed.",
        ),
        RowId::NextWord => (
            "Next word after a space",
            "After a space, suggest a word that often follows the one you just finished.",
        ),
        RowId::HighlightAtRest => (
            "Highlight at rest",
            "None waits until you cycle or click a suggestion. First highlights the top suggestion as soon as the list appears.",
        ),
        RowId::Columns => (
            "Columns",
            "How many suggestions sit on one row. Suggestions past columns times rows are not shown.",
        ),
        RowId::Rows => (
            "Rows",
            "How many suggestion rows to reserve. Use two when you want a correction beside the other suggestions.",
        ),
        RowId::SeeThrough => (
            "See-through window",
            "Draws the overlay as a layered window. Keyboard opacity and menu opacity apply only while this is on.",
        ),
        RowId::KeyboardOpacity => (
            "Keyboard opacity",
            "How solid the keyboard and the text field are. 1 is opaque.",
        ),
        RowId::MenuOpacity => (
            "Menu opacity",
            "How solid settings, mappings, and the layout picker are.",
        ),
        RowId::KeyWidth => (
            "Key width",
            "Horizontal size of a key. This is layout scale, not how far the stick moves.",
        ),
        RowId::KeyHeight => ("Key height", "Vertical size of a key."),
        RowId::DelayBeforeRepeat => (
            "Delay before repeat",
            "How long a key must be held before it starts repeating. 0 sends a repeat on every poll.",
        ),
        RowId::RepeatInterval => (
            "Repeat interval",
            "Time between repeats after the first one. 0 uses the delay before repeat for every step.",
        ),
        RowId::PadHorizontalRange => (
            "Horizontal range",
            "A full slide left or right on the pad covers this much of the keyboard. Raise it when the outer columns stay out of reach.",
        ),
        RowId::PadVerticalRange => (
            "Vertical range",
            "A full slide up or down on the pad covers this much of the keyboard.",
        ),
        RowId::PadSquare => (
            "Square the pad",
            "0 leaves diagonals where the pad reports them. 1 stretches a full diagonal out to the corner keys. A value in between is a partial stretch.",
        ),
        RowId::PadStickiness => (
            "Stickiness",
            "The key you are already on keeps the highlight until another key is this many times closer to the pad. At 1.25 a neighbor has to be noticeably closer before the highlight moves. 1 turns that off, and the nearest key wins immediately.",
        ),
        RowId::PadHoldAfterKey => (
            "Hold after a key",
            "After a letter is sent, the highlight stays on that key for this long. 0 releases it immediately.",
        ),
        RowId::HorizontalRange => (
            "Horizontal range",
            "A full deflection left or right covers this much of the keyboard. Raise it when the outer columns stay out of reach.",
        ),
        RowId::VerticalRange => (
            "Vertical range",
            "A full deflection up or down covers this much of the keyboard.",
        ),
        RowId::SquareStick => (
            "Square the stick",
            "The stick's physical travel is a circle, so a diagonal falls short of the corner keys. 0 keeps that circle. 1 stretches diagonals outward until a full diagonal reaches the corner. A value in between is a partial stretch.",
        ),
        RowId::Stickiness => (
            "Stickiness",
            "The key you are already on keeps the highlight until another key is this many times closer to the stick. At 1.25 a neighbor has to be noticeably closer before the highlight moves. 1 turns that off, and the nearest key wins immediately.",
        ),
        RowId::HoldAfterKey => (
            "Hold after a key",
            "After a letter is sent, the highlight stays on that key for this long. 0 releases it immediately.",
        ),
        RowId::ThumbRest => (
            "Thumb rest",
            "0 treats the place you touch as the key. A thumb on the upper right of the pad highlights an upper-right key. 1 treats the first contact as rest: the highlight starts on that pad's home-row key and only moves as you slide away from where you landed. A value in between starts part-way between those two.",
        ),
        RowId::StretchShortSide => (
            "Stretch the short side",
            "If rest is not the center of the pad, one direction has less pad left. 0 follows your thumb one-to-one, so you can run out of pad before the far keys. 1 speeds up only that short direction, so those keys stay reachable. The long direction is left as it is.",
        ),
        RowId::PadClick => (
            "Pad click",
            "How hard the pads click when you press them. Off is silent. Both pads use this level.",
        ),
        RowId::Sc2TriggerLeft | RowId::Ps4TriggerLeft => (
            "Left trigger",
            "How far the trigger must travel before it counts as pressed. Raise it if a resting finger sends keys.",
        ),
        RowId::Sc2TriggerRight | RowId::Ps4TriggerRight => (
            "Right trigger",
            "The same cutoff, on the right trigger.",
        ),
        RowId::StickCursors => ("Stick cursors", "Draw where the sticks are pointing."),
        RowId::Hitboxes => ("Hitboxes", "Draw the region each key occupies."),
        RowId::StickBounds => ("Stick bounds", "Draw the rectangle the sticks can reach."),
        RowId::ReachOverlay => (
            "Reach overlay",
            "Draw which keys a stick or a pad can reach. None leaves the keyboard as it is.",
        ),
        RowId::StretchMaxGain => (
            "Stretch limit",
            "Caps how much the short direction of the pad can speed up. 1 turns that extra speed-up off.",
        ),
        RowId::PadSettleMs => (
            "Touch settle time",
            "How long to wait after your thumb lands before reading the touch. Raise it if the first key picked is jumpy.",
        ),
        RowId::InsertSpace => (
            "Space after accepting",
            "Adds a space right after the word you pick, so you can keep typing the next one.",
        ),
        RowId::ResetHighlight => (
            "Restart highlight on refresh",
            "When the suggestion list changes, move the highlight back to the start instead of keeping its place.",
        ),
        RowId::HighlightWraps => (
            "Highlight wraps around",
            "Moving past the last suggestion jumps back to the first one, and the other way around.",
        ),
        RowId::LearnOnAccept => (
            "Learn picked words",
            "Remember words you pick so they rank higher next time.",
        ),
        RowId::LearnOnSubmit => (
            "Learn submitted lines",
            "Remember the words in a line you submit, so they rank higher next time.",
        ),
        RowId::UnicodeLetters => (
            "Non-English letters",
            "Treat letters with accents and other alphabets as word characters while typing.",
        ),
        RowId::NormalizeNfc => (
            "Unify accented letters",
            "Treat accented letters typed different ways as the same letter when matching words.",
        ),
        RowId::Capitalization => (
            "Fix capitalization",
            "Offer the word with the right capital letter when what you typed is close but for the case.",
        ),
        RowId::TransposeNeighbors => (
            "Only next-door swaps",
            "Only suggest a word when the two swapped letters sit right next to each other. Off also allows wider swaps.",
        ),
        RowId::MaxSuggestions => (
            "Max suggestions",
            "The most suggestions the list will ever show at once.",
        ),
        RowId::MinPrefixLen => (
            "Letters before suggesting",
            "How many letters you must type before any suggestions appear. 0 shows them right away.",
        ),
        RowId::CompleteDebounce => (
            "Suggestion wait",
            "How long to wait after you stop typing before the list refreshes. Raise it if the list flickers.",
        ),
        RowId::MinFuzzyLen => (
            "Shortest typo fix",
            "Words shorter than this never get typo corrections, only exact matches.",
        ),
        RowId::Backend => (
            "Word source",
            "Where suggestions come from. Smart uses your recent writing; Dictionary uses the plain word list.",
        ),
        RowId::Fallback => (
            "Backup word source",
            "Where suggestions come from when the main source has nothing to offer.",
        ),
        RowId::CurrentWordChip => (
            "Current word position",
            "Which end of the suggestion row shows the word you are typing right now.",
        ),
        RowId::AcceptVia => (
            "How a pick is typed",
            "How the picked word replaces what you typed. One way retypes the ending, the other deletes it first.",
        ),
        RowId::RetractAccept => (
            "Undo a pick on backspace",
            "Pressing backspace right after picking a word brings your original letters back.",
        ),
        RowId::TrackBackspace => (
            "Follow backspace",
            "Keep the suggestion list in step when you delete letters.",
        ),
        RowId::ClearOnEnter => (
            "Clear on enter",
            "Forget what you typed and start fresh after you submit a line.",
        ),
        RowId::IgnoreCtrlAlt => (
            "Ignore ctrl and alt",
            "Key presses held with ctrl or alt do not disturb the suggestion list.",
        ),
        RowId::StartArmed => (
            "Listen from the start",
            "Start watching your typing as soon as the keyboard opens, instead of waiting for the first letter.",
        ),
        RowId::LatchOnArrow => (
            "Arrow keys pause listening",
            "Moving the caret with an arrow key stops suggestions until you type again.",
        ),
        RowId::LatchOnPaste => (
            "Pasting pauses listening",
            "Pasted text stops suggestions until you type again.",
        ),
        RowId::ClearLogOnArm => (
            "Fresh start on wake",
            "Throw away the remembered keystrokes each time listening starts again.",
        ),
        RowId::MaxChars => (
            "Longest tracked word",
            "Letters past this many are forgotten while matching. Lower uses less memory.",
        ),
        RowId::IdleResetMs => (
            "Forget when idle",
            "Stop listening after this long with no typing. 0 never stops on its own.",
        ),
        RowId::ReserveSlots => (
            "Keep empty slots",
            "Leave blank space where suggestions will appear, so the keys do not jump when the list pops in.",
        ),
        RowId::ChipWidth => (
            "Suggestion width",
            "Fill stretches each suggestion across its slot. Hug shrinks each one to fit its word.",
        ),
        RowId::ChipPlacement => (
            "Suggestion position",
            "Where the suggestion row sits: between the keys, above the text field, or above the keyboard.",
        ),
        RowId::ChipLabel => (
            "Suggestion text",
            "Show the whole word, or only the ending you have not typed yet.",
        ),
        RowId::DimTypedPrefix => (
            "Fade typed part",
            "Draw the part of the word you already typed in a dimmer color.",
        ),
        RowId::ShowDebugScores => (
            "Show scores",
            "Print each suggestion's match score beside it. Useful when tuning, noisy otherwise.",
        ),
        RowId::ArmedDot => (
            "Listening dot",
            "Show a small dot while the keyboard is watching your typing.",
        ),
        RowId::ArmedDotPlacement => (
            "Dot position",
            "Which end of the suggestion row the listening dot sits on.",
        ),
        RowId::MaxChipWidth => (
            "Widest suggestion",
            "A suggestion never grows wider than this, no matter how long the word is.",
        ),
        RowId::MinChipWidth => (
            "Narrowest suggestion",
            "A suggestion never shrinks narrower than this, no matter how short the word is.",
        ),
        RowId::ChipFontSize => (
            "Suggestion text size",
            "How big the suggestion words are drawn.",
        ),
        RowId::CornerRadius => (
            "Suggestion roundness",
            "How rounded the corners of each suggestion look. 0 is a sharp rectangle.",
        ),
        RowId::OutlineWidth => (
            "Highlight outline",
            "How thick the outline around the highlighted suggestion is. 0 hides it.",
        ),
        RowId::PaddingX => (
            "Side padding",
            "Empty space left and right inside each suggestion.",
        ),
        RowId::PaddingY => (
            "Top padding",
            "Empty space above and below inside each suggestion.",
        ),
        RowId::Gap => (
            "Gap between suggestions",
            "Empty space between one suggestion and the next.",
        ),
        RowId::ArmedDotRadius => (
            "Dot size",
            "How big the listening dot is drawn.",
        ),
        RowId::CacheEnabled => (
            "Remember my words",
            "Keep a file of words you use so they keep ranking higher between sessions.",
        ),
        RowId::MaxUnigrams => (
            "Single words kept",
            "How many single words your personal file holds. 0 keeps none.",
        ),
        RowId::MaxBigrams => (
            "Word pairs kept",
            "How many two-word pairs your personal file holds. 0 keeps none.",
        ),
        RowId::NgramOrder => (
            "Word memory length",
            "How many previous words the smart source looks at. 3 means the last two words shape the next suggestion.",
        ),
        RowId::BackoffAlpha => (
            "Trust in longer memory",
            "How much to trust longer word histories over shorter ones. Higher leans on longer histories.",
        ),
        RowId::LambdaTri => (
            "Two-word history weight",
            "How much the last two words count when ranking. Higher trusts recent context more.",
        ),
        RowId::LambdaBi => (
            "One-word history weight",
            "How much the last word counts when ranking. Higher trusts the previous word more.",
        ),
        RowId::LambdaUni => (
            "Common words weight",
            "How much plain word popularity counts when ranking. Higher favors common words.",
        ),
        RowId::LambdaExact => (
            "Exact match bonus",
            "Extra score for words that match what you typed letter for letter.",
        ),
        RowId::LambdaTypo => (
            "Typo penalty",
            "How much to lower the score of a word that differs from what you typed. More negative punishes typos harder.",
        ),
        RowId::PrefixScanLimit => (
            "Words scanned",
            "How many dictionary words to scan for each keystroke. Higher finds more, but can feel slower.",
        ),
        RowId::AbortCheckEvery => (
            "Slow-search cutoff",
            "How often to check whether a slow search should give up early. Lower gives up sooner.",
        ),
        RowId::TextFontSize => (
            "Text field size",
            "How big the text is on the text-input screen.",
        ),
        RowId::BatteryButton => (
            "Battery as key",
            "Draw the battery readout with the same background as the keys around it.",
        ),
    }
}

fn is_toggle(id: RowId) -> bool {
    matches!(
        id,
        RowId::Suggestions
            | RowId::OnKeyboard
            | RowId::InTextField
            | RowId::Typos
            | RowId::NextWord
            | RowId::SeeThrough
            | RowId::StickCursors
            | RowId::Hitboxes
            | RowId::StickBounds
            | RowId::InsertSpace
            | RowId::ResetHighlight
            | RowId::HighlightWraps
            | RowId::LearnOnAccept
            | RowId::LearnOnSubmit
            | RowId::UnicodeLetters
            | RowId::NormalizeNfc
            | RowId::Capitalization
            | RowId::TransposeNeighbors
            | RowId::RetractAccept
            | RowId::TrackBackspace
            | RowId::ClearOnEnter
            | RowId::IgnoreCtrlAlt
            | RowId::StartArmed
            | RowId::LatchOnArrow
            | RowId::LatchOnPaste
            | RowId::ClearLogOnArm
            | RowId::ReserveSlots
            | RowId::DimTypedPrefix
            | RowId::ShowDebugScores
            | RowId::ArmedDot
            | RowId::CacheEnabled
            | RowId::BatteryButton
    )
}

fn stick_aim(cfg: &Config, kind: ControllerKind) -> &crate::config::AimProfile {
    match crate::config::resolved_controller(kind) {
        ControllerKind::Ps4 => &cfg.ps4.stick,
        _ => &cfg.sc2.stick,
    }
}

fn stick_aim_mut(cfg: &mut Config, kind: ControllerKind) -> &mut crate::config::AimProfile {
    match crate::config::resolved_controller(kind) {
        ControllerKind::Ps4 => &mut cfg.ps4.stick,
        _ => &mut cfg.sc2.stick,
    }
}

pub(in crate::state) fn format_value(cfg: &Config, id: RowId, kind: ControllerKind) -> String {
    match id {
        RowId::Suggestions => on_off(cfg.completion.enabled),
        RowId::OnKeyboard => on_off(cfg.completion.show_in_keyboard),
        RowId::InTextField => on_off(cfg.completion.show_in_text_input),
        RowId::Typos => on_off(cfg.completion.typo_tolerance),
        RowId::NextWord => on_off(cfg.completion.suggest_next_word),
        RowId::HighlightAtRest => match cfg.completion.preselect {
            Preselect::None => "None".to_owned(),
            Preselect::First => "First".to_owned(),
        },
        RowId::Columns => cfg.completion.ui.columns.to_string(),
        RowId::Rows => cfg.completion.ui.rows.to_string(),
        RowId::SeeThrough => on_off(cfg.transparent),
        RowId::KeyboardOpacity => format_f32(cfg.keyboard_opacity, OPACITY_DIGITS),
        RowId::MenuOpacity => format_f32(cfg.ui_opacity, OPACITY_DIGITS),
        RowId::KeyWidth => format_f32(cfg.scale_x, 0),
        RowId::KeyHeight => format_f32(cfg.scale_y, 0),
        RowId::DelayBeforeRepeat => format!("{} ms", cfg.event_debounce_ms),
        RowId::RepeatInterval => format!("{} ms", cfg.event_debounce_repeat_ms),
        RowId::PadHorizontalRange => format_f32(cfg.sc2.pad.scale_x, STICK_RANGE_DIGITS),
        RowId::PadVerticalRange => format_f32(cfg.sc2.pad.scale_y, STICK_RANGE_DIGITS),
        RowId::PadSquare => format_f32(cfg.sc2.pad.warp, UNIT_DIGITS),
        RowId::PadStickiness => format_f32(cfg.sc2.pad.select_sticky, UNIT_DIGITS),
        RowId::PadHoldAfterKey => format!("{} ms", cfg.sc2.pad.select_lock_ms),
        RowId::HorizontalRange => format_f32(stick_aim(cfg, kind).scale_x, STICK_RANGE_DIGITS),
        RowId::VerticalRange => format_f32(stick_aim(cfg, kind).scale_y, STICK_RANGE_DIGITS),
        RowId::SquareStick => format_f32(stick_aim(cfg, kind).warp, UNIT_DIGITS),
        RowId::Stickiness => format_f32(stick_aim(cfg, kind).select_sticky, UNIT_DIGITS),
        RowId::HoldAfterKey => format!("{} ms", stick_aim(cfg, kind).select_lock_ms),
        RowId::ThumbRest => format_f32(cfg.sc2.pad_origin_relative, UNIT_DIGITS),
        RowId::StretchShortSide => format_f32(cfg.sc2.pad_origin_stretch, UNIT_DIGITS),
        RowId::PadClick => haptic_label(cfg.sc2.touchpad_left_haptic),
        RowId::Sc2TriggerLeft => cfg.sc2.trigger_left_threshold.to_string(),
        RowId::Sc2TriggerRight => cfg.sc2.trigger_right_threshold.to_string(),
        RowId::Ps4TriggerLeft => cfg.ps4.trigger_left_threshold.to_string(),
        RowId::Ps4TriggerRight => cfg.ps4.trigger_right_threshold.to_string(),
        RowId::StickCursors => on_off(debug_flag(cfg, |d| d.show_stick_cursors)),
        RowId::Hitboxes => on_off(debug_flag(cfg, |d| d.show_hitboxes)),
        RowId::StickBounds => on_off(debug_flag(cfg, |d| d.show_stick_bounds)),
        RowId::ReachOverlay => reach_label(cfg.debug.as_ref().map(|d| d.reach_overlay)),
        RowId::StretchMaxGain => format_f32(cfg.sc2.pad_origin_stretch_max_gain, UNIT_DIGITS),
        RowId::PadSettleMs => format!("{} ms", cfg.sc2.pad_origin_settle_ms),
        RowId::InsertSpace => on_off(cfg.completion.insert_space_on_accept),
        RowId::ResetHighlight => on_off(cfg.completion.reset_highlight_on_refresh),
        RowId::HighlightWraps => on_off(cfg.completion.highlight_wraps),
        RowId::LearnOnAccept => on_off(cfg.completion.learn_on_accept),
        RowId::LearnOnSubmit => on_off(cfg.completion.learn_on_submit),
        RowId::UnicodeLetters => on_off(cfg.completion.unicode_letters),
        RowId::NormalizeNfc => on_off(cfg.completion.normalize_nfc),
        RowId::Capitalization => on_off(cfg.completion.capitalization),
        RowId::TransposeNeighbors => on_off(cfg.completion.transpose_neighbors_only),
        RowId::MaxSuggestions => cfg.completion.max_suggestions.to_string(),
        RowId::MinPrefixLen => cfg.completion.min_prefix_len.to_string(),
        RowId::CompleteDebounce => format!("{} ms", cfg.completion.debounce_ms),
        RowId::MinFuzzyLen => cfg.completion.min_fuzzy_len.to_string(),
        RowId::Backend => backend_label(cfg.completion.backend).to_owned(),
        RowId::Fallback => backend_label(cfg.completion.fallback).to_owned(),
        RowId::CurrentWordChip => word_chip_label(cfg.completion.current_word_chip).to_owned(),
        RowId::AcceptVia => accept_via_label(cfg.completion.keyboard.accept_via).to_owned(),
        RowId::RetractAccept => on_off(cfg.completion.keyboard.retract_last_accept),
        RowId::TrackBackspace => on_off(cfg.completion.keyboard.track_backspace),
        RowId::ClearOnEnter => on_off(cfg.completion.keyboard.clear_on_enter),
        RowId::IgnoreCtrlAlt => on_off(cfg.completion.keyboard.ignore_ctrl_alt),
        RowId::StartArmed => on_off(cfg.completion.keyboard.start_armed),
        RowId::LatchOnArrow => on_off(cfg.completion.keyboard.latch_off_on_arrow),
        RowId::LatchOnPaste => on_off(cfg.completion.keyboard.latch_off_on_paste),
        RowId::ClearLogOnArm => on_off(cfg.completion.keyboard.clear_log_on_arm),
        RowId::MaxChars => cfg.completion.keyboard.max_chars.to_string(),
        RowId::IdleResetMs => format!("{} ms", cfg.completion.keyboard.idle_reset_ms),
        RowId::ReserveSlots => on_off(cfg.completion.ui.reserve_slots),
        RowId::ChipWidth => chip_width_label(cfg.completion.ui.chip_width).to_owned(),
        RowId::ChipPlacement => chip_placement_label(cfg.completion.ui.placement).to_owned(),
        RowId::ChipLabel => chip_label_label(cfg.completion.ui.label).to_owned(),
        RowId::DimTypedPrefix => on_off(cfg.completion.ui.dim_typed_prefix),
        RowId::ShowDebugScores => on_off(cfg.completion.ui.show_debug_scores),
        RowId::ArmedDot => on_off(cfg.completion.ui.armed_dot),
        RowId::ArmedDotPlacement => {
            dot_placement_label(cfg.completion.ui.armed_dot_placement).to_owned()
        }
        RowId::MaxChipWidth => format_f32(cfg.completion.ui.max_chip_width, 0),
        RowId::MinChipWidth => format_f32(cfg.completion.ui.min_chip_width, 0),
        RowId::ChipFontSize => format_f32(cfg.completion.ui.font_size, 0),
        RowId::CornerRadius => format_f32(cfg.completion.ui.corner_radius, 0),
        RowId::OutlineWidth => format_f32(cfg.completion.ui.selected_outline_width, 1),
        RowId::PaddingX => format_f32(cfg.completion.ui.padding_x, 0),
        RowId::PaddingY => format_f32(cfg.completion.ui.padding_y, 0),
        RowId::Gap => format_f32(cfg.completion.ui.gap, 0),
        RowId::ArmedDotRadius => format_f32(cfg.completion.ui.armed_dot_radius, 1),
        RowId::CacheEnabled => on_off(cfg.completion.user_cache.enabled),
        RowId::MaxUnigrams => cfg.completion.user_cache.max_unigrams.to_string(),
        RowId::MaxBigrams => cfg.completion.user_cache.max_bigrams.to_string(),
        RowId::NgramOrder => cfg.completion.ngram.order.to_string(),
        RowId::BackoffAlpha => format_f32(cfg.completion.ngram.backoff_alpha, UNIT_DIGITS),
        RowId::LambdaTri => format_f32(cfg.completion.ngram.lambda_trigram, UNIT_DIGITS),
        RowId::LambdaBi => format_f32(cfg.completion.ngram.lambda_bigram, UNIT_DIGITS),
        RowId::LambdaUni => format_f32(cfg.completion.ngram.lambda_unigram, UNIT_DIGITS),
        RowId::LambdaExact => format_f32(cfg.completion.ngram.lambda_exact, UNIT_DIGITS),
        RowId::LambdaTypo => format_f32(cfg.completion.ngram.lambda_typo, 1),
        RowId::PrefixScanLimit => cfg.completion.ngram.prefix_scan_limit.to_string(),
        RowId::AbortCheckEvery => cfg.completion.ngram.abort_check_every.to_string(),
        RowId::TextFontSize => format_f32(cfg.text_input.font_size, 0),
        RowId::BatteryButton => on_off(cfg.battery.draw_button),
    }
}

pub(in crate::state) fn copy_row(dst: &mut Config, src: &Config, id: RowId, kind: ControllerKind) {
    match id {
        RowId::Suggestions => dst.completion.enabled = src.completion.enabled,
        RowId::OnKeyboard => dst.completion.show_in_keyboard = src.completion.show_in_keyboard,
        RowId::InTextField => dst.completion.show_in_text_input = src.completion.show_in_text_input,
        RowId::Typos => dst.completion.typo_tolerance = src.completion.typo_tolerance,
        RowId::NextWord => dst.completion.suggest_next_word = src.completion.suggest_next_word,
        RowId::HighlightAtRest => dst.completion.preselect = src.completion.preselect,
        RowId::Columns => dst.completion.ui.columns = src.completion.ui.columns,
        RowId::Rows => dst.completion.ui.rows = src.completion.ui.rows,
        RowId::SeeThrough => dst.transparent = src.transparent,
        RowId::KeyboardOpacity => dst.keyboard_opacity = src.keyboard_opacity,
        RowId::MenuOpacity => dst.ui_opacity = src.ui_opacity,
        RowId::KeyWidth => dst.scale_x = src.scale_x,
        RowId::KeyHeight => dst.scale_y = src.scale_y,
        RowId::DelayBeforeRepeat => dst.event_debounce_ms = src.event_debounce_ms,
        RowId::RepeatInterval => dst.event_debounce_repeat_ms = src.event_debounce_repeat_ms,
        RowId::PadHorizontalRange => dst.sc2.pad.scale_x = src.sc2.pad.scale_x,
        RowId::PadVerticalRange => dst.sc2.pad.scale_y = src.sc2.pad.scale_y,
        RowId::PadSquare => dst.sc2.pad.warp = src.sc2.pad.warp,
        RowId::PadStickiness => dst.sc2.pad.select_sticky = src.sc2.pad.select_sticky,
        RowId::PadHoldAfterKey => dst.sc2.pad.select_lock_ms = src.sc2.pad.select_lock_ms,
        RowId::HorizontalRange => stick_aim_mut(dst, kind).scale_x = stick_aim(src, kind).scale_x,
        RowId::VerticalRange => stick_aim_mut(dst, kind).scale_y = stick_aim(src, kind).scale_y,
        RowId::SquareStick => stick_aim_mut(dst, kind).warp = stick_aim(src, kind).warp,
        RowId::Stickiness => {
            stick_aim_mut(dst, kind).select_sticky = stick_aim(src, kind).select_sticky
        }
        RowId::HoldAfterKey => {
            stick_aim_mut(dst, kind).select_lock_ms = stick_aim(src, kind).select_lock_ms
        }
        RowId::ThumbRest => dst.sc2.pad_origin_relative = src.sc2.pad_origin_relative,
        RowId::StretchShortSide => dst.sc2.pad_origin_stretch = src.sc2.pad_origin_stretch,
        RowId::PadClick => {
            dst.sc2.touchpad_left_haptic = src.sc2.touchpad_left_haptic;
            dst.sc2.touchpad_right_haptic = src.sc2.touchpad_right_haptic;
        }
        RowId::Sc2TriggerLeft => dst.sc2.trigger_left_threshold = src.sc2.trigger_left_threshold,
        RowId::Sc2TriggerRight => dst.sc2.trigger_right_threshold = src.sc2.trigger_right_threshold,
        RowId::Ps4TriggerLeft => dst.ps4.trigger_left_threshold = src.ps4.trigger_left_threshold,
        RowId::Ps4TriggerRight => dst.ps4.trigger_right_threshold = src.ps4.trigger_right_threshold,
        RowId::StickCursors | RowId::Hitboxes | RowId::StickBounds | RowId::ReachOverlay => {
            dst.debug = src.debug.clone();
        }
        RowId::StretchMaxGain => {
            dst.sc2.pad_origin_stretch_max_gain = src.sc2.pad_origin_stretch_max_gain
        }
        RowId::PadSettleMs => dst.sc2.pad_origin_settle_ms = src.sc2.pad_origin_settle_ms,
        RowId::InsertSpace => {
            dst.completion.insert_space_on_accept = src.completion.insert_space_on_accept
        }
        RowId::ResetHighlight => {
            dst.completion.reset_highlight_on_refresh = src.completion.reset_highlight_on_refresh
        }
        RowId::HighlightWraps => dst.completion.highlight_wraps = src.completion.highlight_wraps,
        RowId::LearnOnAccept => dst.completion.learn_on_accept = src.completion.learn_on_accept,
        RowId::LearnOnSubmit => dst.completion.learn_on_submit = src.completion.learn_on_submit,
        RowId::UnicodeLetters => dst.completion.unicode_letters = src.completion.unicode_letters,
        RowId::NormalizeNfc => dst.completion.normalize_nfc = src.completion.normalize_nfc,
        RowId::Capitalization => dst.completion.capitalization = src.completion.capitalization,
        RowId::TransposeNeighbors => {
            dst.completion.transpose_neighbors_only = src.completion.transpose_neighbors_only
        }
        RowId::MaxSuggestions => dst.completion.max_suggestions = src.completion.max_suggestions,
        RowId::MinPrefixLen => dst.completion.min_prefix_len = src.completion.min_prefix_len,
        RowId::CompleteDebounce => dst.completion.debounce_ms = src.completion.debounce_ms,
        RowId::MinFuzzyLen => dst.completion.min_fuzzy_len = src.completion.min_fuzzy_len,
        RowId::Backend => dst.completion.backend = src.completion.backend,
        RowId::Fallback => dst.completion.fallback = src.completion.fallback,
        RowId::CurrentWordChip => {
            dst.completion.current_word_chip = src.completion.current_word_chip
        }
        RowId::AcceptVia => dst.completion.keyboard.accept_via = src.completion.keyboard.accept_via,
        RowId::RetractAccept => {
            dst.completion.keyboard.retract_last_accept =
                src.completion.keyboard.retract_last_accept
        }
        RowId::TrackBackspace => {
            dst.completion.keyboard.track_backspace = src.completion.keyboard.track_backspace
        }
        RowId::ClearOnEnter => {
            dst.completion.keyboard.clear_on_enter = src.completion.keyboard.clear_on_enter
        }
        RowId::IgnoreCtrlAlt => {
            dst.completion.keyboard.ignore_ctrl_alt = src.completion.keyboard.ignore_ctrl_alt
        }
        RowId::StartArmed => {
            dst.completion.keyboard.start_armed = src.completion.keyboard.start_armed
        }
        RowId::LatchOnArrow => {
            dst.completion.keyboard.latch_off_on_arrow = src.completion.keyboard.latch_off_on_arrow
        }
        RowId::LatchOnPaste => {
            dst.completion.keyboard.latch_off_on_paste = src.completion.keyboard.latch_off_on_paste
        }
        RowId::ClearLogOnArm => {
            dst.completion.keyboard.clear_log_on_arm = src.completion.keyboard.clear_log_on_arm
        }
        RowId::MaxChars => dst.completion.keyboard.max_chars = src.completion.keyboard.max_chars,
        RowId::IdleResetMs => {
            dst.completion.keyboard.idle_reset_ms = src.completion.keyboard.idle_reset_ms
        }
        RowId::ReserveSlots => dst.completion.ui.reserve_slots = src.completion.ui.reserve_slots,
        RowId::ChipWidth => dst.completion.ui.chip_width = src.completion.ui.chip_width,
        RowId::ChipPlacement => dst.completion.ui.placement = src.completion.ui.placement,
        RowId::ChipLabel => dst.completion.ui.label = src.completion.ui.label,
        RowId::DimTypedPrefix => {
            dst.completion.ui.dim_typed_prefix = src.completion.ui.dim_typed_prefix
        }
        RowId::ShowDebugScores => {
            dst.completion.ui.show_debug_scores = src.completion.ui.show_debug_scores
        }
        RowId::ArmedDot => dst.completion.ui.armed_dot = src.completion.ui.armed_dot,
        RowId::ArmedDotPlacement => {
            dst.completion.ui.armed_dot_placement = src.completion.ui.armed_dot_placement
        }
        RowId::MaxChipWidth => dst.completion.ui.max_chip_width = src.completion.ui.max_chip_width,
        RowId::MinChipWidth => dst.completion.ui.min_chip_width = src.completion.ui.min_chip_width,
        RowId::ChipFontSize => dst.completion.ui.font_size = src.completion.ui.font_size,
        RowId::CornerRadius => dst.completion.ui.corner_radius = src.completion.ui.corner_radius,
        RowId::OutlineWidth => {
            dst.completion.ui.selected_outline_width = src.completion.ui.selected_outline_width
        }
        RowId::PaddingX => dst.completion.ui.padding_x = src.completion.ui.padding_x,
        RowId::PaddingY => dst.completion.ui.padding_y = src.completion.ui.padding_y,
        RowId::Gap => dst.completion.ui.gap = src.completion.ui.gap,
        RowId::ArmedDotRadius => {
            dst.completion.ui.armed_dot_radius = src.completion.ui.armed_dot_radius
        }
        RowId::CacheEnabled => {
            dst.completion.user_cache.enabled = src.completion.user_cache.enabled
        }
        RowId::MaxUnigrams => {
            dst.completion.user_cache.max_unigrams = src.completion.user_cache.max_unigrams
        }
        RowId::MaxBigrams => {
            dst.completion.user_cache.max_bigrams = src.completion.user_cache.max_bigrams
        }
        RowId::NgramOrder => dst.completion.ngram.order = src.completion.ngram.order,
        RowId::BackoffAlpha => {
            dst.completion.ngram.backoff_alpha = src.completion.ngram.backoff_alpha
        }
        RowId::LambdaTri => {
            dst.completion.ngram.lambda_trigram = src.completion.ngram.lambda_trigram
        }
        RowId::LambdaBi => dst.completion.ngram.lambda_bigram = src.completion.ngram.lambda_bigram,
        RowId::LambdaUni => {
            dst.completion.ngram.lambda_unigram = src.completion.ngram.lambda_unigram
        }
        RowId::LambdaExact => dst.completion.ngram.lambda_exact = src.completion.ngram.lambda_exact,
        RowId::LambdaTypo => dst.completion.ngram.lambda_typo = src.completion.ngram.lambda_typo,
        RowId::PrefixScanLimit => {
            dst.completion.ngram.prefix_scan_limit = src.completion.ngram.prefix_scan_limit
        }
        RowId::AbortCheckEvery => {
            dst.completion.ngram.abort_check_every = src.completion.ngram.abort_check_every
        }
        RowId::TextFontSize => dst.text_input.font_size = src.text_input.font_size,
        RowId::BatteryButton => dst.battery.draw_button = src.battery.draw_button,
    }
}

fn apply_dir(cfg: &mut Config, id: RowId, dir: i32, kind: ControllerKind) {
    match id {
        RowId::Suggestions => cfg.completion.enabled = !cfg.completion.enabled,
        RowId::OnKeyboard => cfg.completion.show_in_keyboard = !cfg.completion.show_in_keyboard,
        RowId::InTextField => {
            cfg.completion.show_in_text_input = !cfg.completion.show_in_text_input
        }
        RowId::Typos => cfg.completion.typo_tolerance = !cfg.completion.typo_tolerance,
        RowId::NextWord => cfg.completion.suggest_next_word = !cfg.completion.suggest_next_word,
        RowId::HighlightAtRest => {
            cfg.completion.preselect = cycle_preselect(cfg.completion.preselect, dir)
        }
        RowId::Columns => {
            cfg.completion.ui.columns =
                step_usize(cfg.completion.ui.columns, COLUMNS_MIN, COLUMNS_MAX, dir)
        }
        RowId::Rows => {
            cfg.completion.ui.rows = step_usize(cfg.completion.ui.rows, ROWS_MIN, ROWS_MAX, dir)
        }
        RowId::SeeThrough => cfg.transparent = !cfg.transparent,
        RowId::KeyboardOpacity => {
            cfg.keyboard_opacity = step_f32(
                cfg.keyboard_opacity,
                KEYBOARD_OPACITY_MIN,
                OPACITY_MAX,
                OPACITY_STEP,
                dir,
                OPACITY_DIGITS,
            )
        }
        RowId::MenuOpacity => {
            cfg.ui_opacity = step_f32(
                cfg.ui_opacity,
                MENU_OPACITY_MIN,
                OPACITY_MAX,
                OPACITY_STEP,
                dir,
                OPACITY_DIGITS,
            )
        }
        RowId::KeyWidth => {
            cfg.scale_x = step_f32(
                cfg.scale_x,
                KEY_SCALE_MIN,
                KEY_SCALE_MAX,
                KEY_SCALE_STEP,
                dir,
                0,
            )
        }
        RowId::KeyHeight => {
            cfg.scale_y = step_f32(
                cfg.scale_y,
                KEY_SCALE_MIN,
                KEY_SCALE_MAX,
                KEY_SCALE_STEP,
                dir,
                0,
            )
        }
        RowId::DelayBeforeRepeat => {
            cfg.event_debounce_ms =
                step_u64(cfg.event_debounce_ms, 0, DEBOUNCE_MAX, DEBOUNCE_STEP, dir)
        }
        RowId::RepeatInterval => {
            cfg.event_debounce_repeat_ms = step_u64(
                cfg.event_debounce_repeat_ms,
                0,
                REPEAT_MAX,
                REPEAT_STEP,
                dir,
            )
        }
        RowId::PadHorizontalRange => {
            cfg.sc2.pad.scale_x = step_f32(
                cfg.sc2.pad.scale_x,
                STICK_RANGE_MIN,
                STICK_RANGE_MAX,
                STICK_RANGE_STEP,
                dir,
                STICK_RANGE_DIGITS,
            )
        }
        RowId::PadVerticalRange => {
            cfg.sc2.pad.scale_y = step_f32(
                cfg.sc2.pad.scale_y,
                STICK_RANGE_MIN,
                STICK_RANGE_MAX,
                STICK_RANGE_STEP,
                dir,
                STICK_RANGE_DIGITS,
            )
        }
        RowId::PadSquare => {
            cfg.sc2.pad.warp = step_f32(
                cfg.sc2.pad.warp,
                UNIT_MIN,
                UNIT_MAX,
                UNIT_STEP,
                dir,
                UNIT_DIGITS,
            )
        }
        RowId::PadStickiness => {
            cfg.sc2.pad.select_sticky = step_f32(
                cfg.sc2.pad.select_sticky,
                STICKY_MIN,
                STICKY_MAX,
                UNIT_STEP,
                dir,
                UNIT_DIGITS,
            )
        }
        RowId::PadHoldAfterKey => {
            cfg.sc2.pad.select_lock_ms =
                step_u64(cfg.sc2.pad.select_lock_ms, 0, LOCK_MAX, LOCK_STEP, dir)
        }
        RowId::HorizontalRange => {
            let aim = stick_aim_mut(cfg, kind);
            aim.scale_x = step_f32(
                aim.scale_x,
                STICK_RANGE_MIN,
                STICK_RANGE_MAX,
                STICK_RANGE_STEP,
                dir,
                STICK_RANGE_DIGITS,
            )
        }
        RowId::VerticalRange => {
            let aim = stick_aim_mut(cfg, kind);
            aim.scale_y = step_f32(
                aim.scale_y,
                STICK_RANGE_MIN,
                STICK_RANGE_MAX,
                STICK_RANGE_STEP,
                dir,
                STICK_RANGE_DIGITS,
            )
        }
        RowId::SquareStick => {
            let aim = stick_aim_mut(cfg, kind);
            aim.warp = step_f32(aim.warp, UNIT_MIN, UNIT_MAX, UNIT_STEP, dir, UNIT_DIGITS)
        }
        RowId::Stickiness => {
            let aim = stick_aim_mut(cfg, kind);
            aim.select_sticky = step_f32(
                aim.select_sticky,
                STICKY_MIN,
                STICKY_MAX,
                UNIT_STEP,
                dir,
                UNIT_DIGITS,
            )
        }
        RowId::HoldAfterKey => {
            let aim = stick_aim_mut(cfg, kind);
            aim.select_lock_ms = step_u64(aim.select_lock_ms, 0, LOCK_MAX, LOCK_STEP, dir)
        }
        RowId::ThumbRest => {
            cfg.sc2.pad_origin_relative = step_f32(
                cfg.sc2.pad_origin_relative,
                UNIT_MIN,
                UNIT_MAX,
                UNIT_STEP,
                dir,
                UNIT_DIGITS,
            )
        }
        RowId::StretchShortSide => {
            cfg.sc2.pad_origin_stretch = step_f32(
                cfg.sc2.pad_origin_stretch,
                UNIT_MIN,
                UNIT_MAX,
                UNIT_STEP,
                dir,
                UNIT_DIGITS,
            )
        }
        RowId::PadClick => {
            let next = cycle_haptic(cfg.sc2.touchpad_left_haptic, dir);
            cfg.sc2.touchpad_left_haptic = next;
            cfg.sc2.touchpad_right_haptic = next;
        }
        RowId::Sc2TriggerLeft => {
            cfg.sc2.trigger_left_threshold = step_u8(
                cfg.sc2.trigger_left_threshold,
                0,
                TRIGGER_MAX,
                TRIGGER_STEP,
                dir,
            )
        }
        RowId::Sc2TriggerRight => {
            cfg.sc2.trigger_right_threshold = step_u8(
                cfg.sc2.trigger_right_threshold,
                0,
                TRIGGER_MAX,
                TRIGGER_STEP,
                dir,
            )
        }
        RowId::Ps4TriggerLeft => {
            cfg.ps4.trigger_left_threshold = step_u8(
                cfg.ps4.trigger_left_threshold,
                0,
                TRIGGER_MAX,
                TRIGGER_STEP,
                dir,
            )
        }
        RowId::Ps4TriggerRight => {
            cfg.ps4.trigger_right_threshold = step_u8(
                cfg.ps4.trigger_right_threshold,
                0,
                TRIGGER_MAX,
                TRIGGER_STEP,
                dir,
            )
        }
        RowId::StickCursors => {
            let next = !debug_flag(cfg, |d| d.show_stick_cursors);
            debug_mut(cfg).show_stick_cursors = next;
        }
        RowId::Hitboxes => {
            let next = !debug_flag(cfg, |d| d.show_hitboxes);
            debug_mut(cfg).show_hitboxes = next;
        }
        RowId::StickBounds => {
            let next = !debug_flag(cfg, |d| d.show_stick_bounds);
            debug_mut(cfg).show_stick_bounds = next;
        }
        RowId::ReachOverlay => {
            let current = cfg.debug.as_ref().map(|d| d.reach_overlay);
            debug_mut(cfg).reach_overlay = cycle_reach(current, dir);
        }
        RowId::StretchMaxGain => {
            cfg.sc2.pad_origin_stretch_max_gain = step_f32(
                cfg.sc2.pad_origin_stretch_max_gain,
                1.0,
                GAIN_MAX,
                UNIT_STEP,
                dir,
                UNIT_DIGITS,
            )
        }
        RowId::PadSettleMs => {
            cfg.sc2.pad_origin_settle_ms = step_u64(
                cfg.sc2.pad_origin_settle_ms,
                0,
                SETTLE_MAX,
                SETTLE_STEP,
                dir,
            )
        }
        RowId::InsertSpace => {
            cfg.completion.insert_space_on_accept = !cfg.completion.insert_space_on_accept
        }
        RowId::ResetHighlight => {
            cfg.completion.reset_highlight_on_refresh = !cfg.completion.reset_highlight_on_refresh
        }
        RowId::HighlightWraps => cfg.completion.highlight_wraps = !cfg.completion.highlight_wraps,
        RowId::LearnOnAccept => cfg.completion.learn_on_accept = !cfg.completion.learn_on_accept,
        RowId::LearnOnSubmit => cfg.completion.learn_on_submit = !cfg.completion.learn_on_submit,
        RowId::UnicodeLetters => cfg.completion.unicode_letters = !cfg.completion.unicode_letters,
        RowId::NormalizeNfc => cfg.completion.normalize_nfc = !cfg.completion.normalize_nfc,
        RowId::Capitalization => cfg.completion.capitalization = !cfg.completion.capitalization,
        RowId::TransposeNeighbors => {
            cfg.completion.transpose_neighbors_only = !cfg.completion.transpose_neighbors_only
        }
        RowId::MaxSuggestions => {
            cfg.completion.max_suggestions = step_usize_by(
                cfg.completion.max_suggestions,
                SUGGESTIONS_MIN,
                SUGGESTIONS_MAX,
                COUNT_STEP,
                dir,
            )
        }
        RowId::MinPrefixLen => {
            cfg.completion.min_prefix_len = step_usize_by(
                cfg.completion.min_prefix_len,
                PREFIX_MIN,
                PREFIX_MAX,
                COUNT_STEP,
                dir,
            )
        }
        RowId::CompleteDebounce => {
            cfg.completion.debounce_ms = step_u64(
                cfg.completion.debounce_ms,
                0,
                COMPLETE_DEBOUNCE_MAX,
                COMPLETE_DEBOUNCE_STEP,
                dir,
            )
        }
        RowId::MinFuzzyLen => {
            cfg.completion.min_fuzzy_len = step_usize_by(
                cfg.completion.min_fuzzy_len,
                FUZZY_MIN,
                FUZZY_MAX,
                COUNT_STEP,
                dir,
            )
        }
        RowId::Backend => cfg.completion.backend = cycle_backend(cfg.completion.backend, dir),
        RowId::Fallback => cfg.completion.fallback = cycle_backend(cfg.completion.fallback, dir),
        RowId::CurrentWordChip => {
            cfg.completion.current_word_chip =
                cycle_word_chip(cfg.completion.current_word_chip, dir)
        }
        RowId::AcceptVia => {
            cfg.completion.keyboard.accept_via =
                cycle_accept_via(cfg.completion.keyboard.accept_via, dir)
        }
        RowId::RetractAccept => {
            cfg.completion.keyboard.retract_last_accept =
                !cfg.completion.keyboard.retract_last_accept
        }
        RowId::TrackBackspace => {
            cfg.completion.keyboard.track_backspace = !cfg.completion.keyboard.track_backspace
        }
        RowId::ClearOnEnter => {
            cfg.completion.keyboard.clear_on_enter = !cfg.completion.keyboard.clear_on_enter
        }
        RowId::IgnoreCtrlAlt => {
            cfg.completion.keyboard.ignore_ctrl_alt = !cfg.completion.keyboard.ignore_ctrl_alt
        }
        RowId::StartArmed => {
            cfg.completion.keyboard.start_armed = !cfg.completion.keyboard.start_armed
        }
        RowId::LatchOnArrow => {
            cfg.completion.keyboard.latch_off_on_arrow = !cfg.completion.keyboard.latch_off_on_arrow
        }
        RowId::LatchOnPaste => {
            cfg.completion.keyboard.latch_off_on_paste = !cfg.completion.keyboard.latch_off_on_paste
        }
        RowId::ClearLogOnArm => {
            cfg.completion.keyboard.clear_log_on_arm = !cfg.completion.keyboard.clear_log_on_arm
        }
        RowId::MaxChars => {
            cfg.completion.keyboard.max_chars = step_usize_by(
                cfg.completion.keyboard.max_chars,
                MAX_CHARS_MIN,
                MAX_CHARS_MAX,
                MAX_CHARS_STEP,
                dir,
            )
        }
        RowId::IdleResetMs => {
            cfg.completion.keyboard.idle_reset_ms = step_u64(
                cfg.completion.keyboard.idle_reset_ms,
                0,
                IDLE_RESET_MAX,
                IDLE_RESET_STEP,
                dir,
            )
        }
        RowId::ReserveSlots => cfg.completion.ui.reserve_slots = !cfg.completion.ui.reserve_slots,
        RowId::ChipWidth => {
            cfg.completion.ui.chip_width = cycle_chip_width(cfg.completion.ui.chip_width, dir)
        }
        RowId::ChipPlacement => {
            cfg.completion.ui.placement = cycle_chip_placement(cfg.completion.ui.placement, dir)
        }
        RowId::ChipLabel => {
            cfg.completion.ui.label = cycle_chip_label(cfg.completion.ui.label, dir)
        }
        RowId::DimTypedPrefix => {
            cfg.completion.ui.dim_typed_prefix = !cfg.completion.ui.dim_typed_prefix
        }
        RowId::ShowDebugScores => {
            cfg.completion.ui.show_debug_scores = !cfg.completion.ui.show_debug_scores
        }
        RowId::ArmedDot => cfg.completion.ui.armed_dot = !cfg.completion.ui.armed_dot,
        RowId::ArmedDotPlacement => {
            cfg.completion.ui.armed_dot_placement =
                cycle_dot_placement(cfg.completion.ui.armed_dot_placement, dir)
        }
        RowId::MaxChipWidth => {
            cfg.completion.ui.max_chip_width = step_f32(
                cfg.completion.ui.max_chip_width,
                CHIP_WIDTH_MIN,
                CHIP_WIDTH_MAX,
                CHIP_WIDTH_STEP,
                dir,
                0,
            )
        }
        RowId::MinChipWidth => {
            cfg.completion.ui.min_chip_width = step_f32(
                cfg.completion.ui.min_chip_width,
                CHIP_MIN_MIN,
                CHIP_MIN_MAX,
                CHIP_MIN_STEP,
                dir,
                0,
            )
        }
        RowId::ChipFontSize => {
            cfg.completion.ui.font_size = step_f32(
                cfg.completion.ui.font_size,
                FONT_MIN,
                FONT_MAX,
                FONT_STEP,
                dir,
                0,
            )
        }
        RowId::CornerRadius => {
            cfg.completion.ui.corner_radius = step_f32(
                cfg.completion.ui.corner_radius,
                0.0,
                RADIUS_MAX,
                FONT_STEP,
                dir,
                0,
            )
        }
        RowId::OutlineWidth => {
            cfg.completion.ui.selected_outline_width = step_f32(
                cfg.completion.ui.selected_outline_width,
                0.0,
                OUTLINE_MAX,
                OUTLINE_STEP,
                dir,
                1,
            )
        }
        RowId::PaddingX => {
            cfg.completion.ui.padding_x = step_f32(
                cfg.completion.ui.padding_x,
                0.0,
                PAD_X_MAX,
                FONT_STEP,
                dir,
                0,
            )
        }
        RowId::PaddingY => {
            cfg.completion.ui.padding_y = step_f32(
                cfg.completion.ui.padding_y,
                0.0,
                PAD_Y_MAX,
                FONT_STEP,
                dir,
                0,
            )
        }
        RowId::Gap => {
            cfg.completion.ui.gap = step_f32(cfg.completion.ui.gap, 0.0, GAP_MAX, FONT_STEP, dir, 0)
        }
        RowId::ArmedDotRadius => {
            cfg.completion.ui.armed_dot_radius = step_f32(
                cfg.completion.ui.armed_dot_radius,
                DOT_RADIUS_MIN,
                DOT_RADIUS_MAX,
                DOT_RADIUS_STEP,
                dir,
                1,
            )
        }
        RowId::CacheEnabled => {
            cfg.completion.user_cache.enabled = !cfg.completion.user_cache.enabled
        }
        RowId::MaxUnigrams => {
            cfg.completion.user_cache.max_unigrams = step_usize_by(
                cfg.completion.user_cache.max_unigrams,
                COUNT_MIN,
                CACHE_MAX,
                CACHE_STEP,
                dir,
            )
        }
        RowId::MaxBigrams => {
            cfg.completion.user_cache.max_bigrams = step_usize_by(
                cfg.completion.user_cache.max_bigrams,
                COUNT_MIN,
                CACHE_MAX,
                CACHE_STEP,
                dir,
            )
        }
        RowId::NgramOrder => {
            cfg.completion.ngram.order = step_u8(
                cfg.completion.ngram.order,
                NGRAM_ORDER_MIN,
                NGRAM_ORDER_MAX,
                1,
                dir,
            )
        }
        RowId::BackoffAlpha => {
            cfg.completion.ngram.backoff_alpha = step_f32(
                cfg.completion.ngram.backoff_alpha,
                UNIT_MIN,
                UNIT_MAX,
                UNIT_STEP,
                dir,
                UNIT_DIGITS,
            )
        }
        RowId::LambdaTri => {
            cfg.completion.ngram.lambda_trigram = step_f32(
                cfg.completion.ngram.lambda_trigram,
                UNIT_MIN,
                WEIGHT_MAX,
                UNIT_STEP,
                dir,
                UNIT_DIGITS,
            )
        }
        RowId::LambdaBi => {
            cfg.completion.ngram.lambda_bigram = step_f32(
                cfg.completion.ngram.lambda_bigram,
                UNIT_MIN,
                WEIGHT_MAX,
                UNIT_STEP,
                dir,
                UNIT_DIGITS,
            )
        }
        RowId::LambdaUni => {
            cfg.completion.ngram.lambda_unigram = step_f32(
                cfg.completion.ngram.lambda_unigram,
                UNIT_MIN,
                WEIGHT_MAX,
                UNIT_STEP,
                dir,
                UNIT_DIGITS,
            )
        }
        RowId::LambdaExact => {
            cfg.completion.ngram.lambda_exact = step_f32(
                cfg.completion.ngram.lambda_exact,
                UNIT_MIN,
                WEIGHT_MAX,
                UNIT_STEP,
                dir,
                UNIT_DIGITS,
            )
        }
        RowId::LambdaTypo => {
            cfg.completion.ngram.lambda_typo = step_f32(
                cfg.completion.ngram.lambda_typo,
                TYPO_WEIGHT_MIN,
                UNIT_MIN,
                0.1,
                dir,
                1,
            )
        }
        RowId::PrefixScanLimit => {
            cfg.completion.ngram.prefix_scan_limit = step_usize_by(
                cfg.completion.ngram.prefix_scan_limit,
                SCAN_MIN,
                SCAN_MAX,
                SCAN_STEP,
                dir,
            )
        }
        RowId::AbortCheckEvery => {
            cfg.completion.ngram.abort_check_every = step_usize_by(
                cfg.completion.ngram.abort_check_every,
                ABORT_MIN,
                ABORT_MAX,
                ABORT_STEP,
                dir,
            )
        }
        RowId::TextFontSize => {
            cfg.text_input.font_size = step_f32(
                cfg.text_input.font_size,
                FONT_MIN,
                FONT_MAX,
                FONT_STEP,
                dir,
                0,
            )
        }
        RowId::BatteryButton => cfg.battery.draw_button = !cfg.battery.draw_button,
    }
}

fn on_off(value: bool) -> String {
    if value {
        "On".to_owned()
    } else {
        "Off".to_owned()
    }
}

fn format_f32(value: f32, digits: u32) -> String {
    if digits == 0 {
        return format!("{}", value.round() as i32);
    }
    format!("{:.*}", digits as usize, value)
}

fn step_f32(value: f32, min: f32, max: f32, step: f32, dir: i32, digits: u32) -> f32 {
    let next = (value + step * dir as f32).clamp(min, max);
    let factor = 10f32.powi(digits as i32);
    (next * factor).round() / factor
}

fn step_u64(value: u64, min: u64, max: u64, step: u64, dir: i32) -> u64 {
    let next = value as i64 + dir as i64 * step as i64;
    next.clamp(min as i64, max as i64) as u64
}

fn step_u8(value: u8, min: u8, max: u8, step: u8, dir: i32) -> u8 {
    let next = value as i32 + dir * step as i32;
    next.clamp(min as i32, max as i32) as u8
}

fn step_usize(value: usize, min: usize, max: usize, dir: i32) -> usize {
    step_usize_by(value, min, max, 1, dir)
}

fn step_usize_by(value: usize, min: usize, max: usize, step: usize, dir: i32) -> usize {
    let next = value as i64 + dir as i64 * step as i64;
    next.clamp(min as i64, max as i64) as usize
}

fn step_index(index: usize, len: usize, dir: i32) -> usize {
    if len == 0 {
        return 0;
    }
    (index as i32 + dir).rem_euclid(len as i32) as usize
}

fn cycle_preselect(value: Preselect, dir: i32) -> Preselect {
    const OPTIONS: [Preselect; 2] = [Preselect::None, Preselect::First];
    let index = OPTIONS.iter().position(|v| *v == value).unwrap_or(0);
    OPTIONS[step_index(index, OPTIONS.len(), dir)]
}

fn cycle_backend(value: CompletionBackendKind, dir: i32) -> CompletionBackendKind {
    const OPTIONS: [CompletionBackendKind; 2] = [
        CompletionBackendKind::Ngram,
        CompletionBackendKind::Dictionary,
    ];
    let index = OPTIONS.iter().position(|v| *v == value).unwrap_or(0);
    OPTIONS[step_index(index, OPTIONS.len(), dir)]
}

fn cycle_word_chip(value: CurrentWordChip, dir: i32) -> CurrentWordChip {
    const OPTIONS: [CurrentWordChip; 2] = [CurrentWordChip::First, CurrentWordChip::Last];
    let index = OPTIONS.iter().position(|v| *v == value).unwrap_or(0);
    OPTIONS[step_index(index, OPTIONS.len(), dir)]
}

fn cycle_accept_via(value: AcceptVia, dir: i32) -> AcceptVia {
    const OPTIONS: [AcceptVia; 2] = [AcceptVia::Suffix, AcceptVia::BackspaceReplace];
    let index = OPTIONS.iter().position(|v| *v == value).unwrap_or(0);
    OPTIONS[step_index(index, OPTIONS.len(), dir)]
}

fn cycle_chip_width(value: ChipWidth, dir: i32) -> ChipWidth {
    const OPTIONS: [ChipWidth; 2] = [ChipWidth::Fill, ChipWidth::Hug];
    let index = OPTIONS.iter().position(|v| *v == value).unwrap_or(0);
    OPTIONS[step_index(index, OPTIONS.len(), dir)]
}

fn cycle_chip_placement(value: ChipPlacement, dir: i32) -> ChipPlacement {
    const OPTIONS: [ChipPlacement; 3] = [
        ChipPlacement::Between,
        ChipPlacement::AboveField,
        ChipPlacement::AboveKeyboard,
    ];
    let index = OPTIONS.iter().position(|v| *v == value).unwrap_or(0);
    OPTIONS[step_index(index, OPTIONS.len(), dir)]
}

fn cycle_chip_label(value: ChipLabel, dir: i32) -> ChipLabel {
    const OPTIONS: [ChipLabel; 2] = [ChipLabel::Full, ChipLabel::Remainder];
    let index = OPTIONS.iter().position(|v| *v == value).unwrap_or(0);
    OPTIONS[step_index(index, OPTIONS.len(), dir)]
}

fn cycle_dot_placement(value: ArmedDotPlacement, dir: i32) -> ArmedDotPlacement {
    const OPTIONS: [ArmedDotPlacement; 2] = [
        ArmedDotPlacement::ChipsLeading,
        ArmedDotPlacement::ChipsTrailing,
    ];
    let index = OPTIONS.iter().position(|v| *v == value).unwrap_or(0);
    OPTIONS[step_index(index, OPTIONS.len(), dir)]
}

fn backend_label(value: CompletionBackendKind) -> &'static str {
    match value {
        CompletionBackendKind::Ngram => "Smart",
        CompletionBackendKind::Dictionary => "Dictionary",
    }
}

fn word_chip_label(value: CurrentWordChip) -> &'static str {
    match value {
        CurrentWordChip::First => "First",
        CurrentWordChip::Last => "Last",
    }
}

fn accept_via_label(value: AcceptVia) -> &'static str {
    match value {
        AcceptVia::Suffix => "Suffix",
        AcceptVia::BackspaceReplace => "Backspace",
    }
}

fn chip_width_label(value: ChipWidth) -> &'static str {
    match value {
        ChipWidth::Fill => "Fill",
        ChipWidth::Hug => "Hug",
    }
}

fn chip_placement_label(value: ChipPlacement) -> &'static str {
    match value {
        ChipPlacement::Between => "Between",
        ChipPlacement::AboveField => "Above field",
        ChipPlacement::AboveKeyboard => "Above keyboard",
    }
}

fn chip_label_label(value: ChipLabel) -> &'static str {
    match value {
        ChipLabel::Full => "Full",
        ChipLabel::Remainder => "Remainder",
    }
}

fn dot_placement_label(value: ArmedDotPlacement) -> &'static str {
    match value {
        ArmedDotPlacement::ChipsLeading => "Leading",
        ArmedDotPlacement::ChipsTrailing => "Trailing",
    }
}

fn cycle_haptic(value: HapticIntensity, dir: i32) -> HapticIntensity {
    const OPTIONS: [HapticIntensity; 4] = [
        HapticIntensity::None,
        HapticIntensity::Low,
        HapticIntensity::Medium,
        HapticIntensity::High,
    ];
    let index = OPTIONS.iter().position(|v| *v == value).unwrap_or(0);
    OPTIONS[step_index(index, OPTIONS.len(), dir)]
}

fn cycle_reach(value: Option<ReachOverlay>, dir: i32) -> ReachOverlay {
    const OPTIONS: [ReachOverlay; 3] = [ReachOverlay::None, ReachOverlay::Stick, ReachOverlay::Pad];
    let index = value
        .and_then(|v| OPTIONS.iter().position(|o| *o == v))
        .unwrap_or(0);
    OPTIONS[step_index(index, OPTIONS.len(), dir)]
}

fn haptic_label(value: HapticIntensity) -> String {
    match value {
        HapticIntensity::None => "Off",
        HapticIntensity::Low => "Low",
        HapticIntensity::Medium => "Medium",
        HapticIntensity::High => "High",
    }
    .to_owned()
}

fn reach_label(value: Option<ReachOverlay>) -> String {
    match value.unwrap_or(ReachOverlay::None) {
        ReachOverlay::None => "None",
        ReachOverlay::Stick => "Stick",
        ReachOverlay::Pad => "Pad",
    }
    .to_owned()
}

fn debug_flag(cfg: &Config, read: impl Fn(&Debug) -> bool) -> bool {
    cfg.debug.as_ref().is_some_and(read)
}

fn debug_mut(cfg: &mut Config) -> &mut Debug {
    cfg.debug.get_or_insert_with(Debug::default)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Config {
        toml::from_str(
            r#"
            layouts = { main = "kb.toml" }
            keyboard_opacity = 1.0
            ui_opacity = 0.4
            "#,
        )
        .unwrap()
    }

    #[test]
    fn opacity_clamps_at_both_ends() {
        let mut form = SettingsForm::new();
        form.view = View::Page(Page::Overlay);
        form.focus = 1;
        let mut cfg = sample();

        assert!(!form.nudge(&mut cfg, 1, ControllerKind::Sc2));
        assert!((cfg.keyboard_opacity - 1.0).abs() < f32::EPSILON);

        cfg.keyboard_opacity = KEYBOARD_OPACITY_MIN;
        assert!(!form.nudge(&mut cfg, -1, ControllerKind::Sc2));
        assert!((cfg.keyboard_opacity - KEYBOARD_OPACITY_MIN).abs() < f32::EPSILON);

        cfg.keyboard_opacity = 0.3;
        assert!(form.nudge(&mut cfg, 1, ControllerKind::Sc2));
        assert!((cfg.keyboard_opacity - 0.35).abs() < 0.001);
    }

    #[test]
    fn highlight_at_rest_wraps() {
        let mut form = SettingsForm::new();
        form.view = View::Page(Page::Suggestions);
        form.focus = 5;
        let mut cfg = sample();
        cfg.completion.preselect = Preselect::None;

        assert!(form.nudge(&mut cfg, 1, ControllerKind::Sc2));
        assert_eq!(cfg.completion.preselect, Preselect::First);
        assert!(form.nudge(&mut cfg, 1, ControllerKind::Sc2));
        assert_eq!(cfg.completion.preselect, Preselect::None);
        assert!(form.nudge(&mut cfg, -1, ControllerKind::Sc2));
        assert_eq!(cfg.completion.preselect, Preselect::First);
    }

    #[test]
    fn toggle_flips_either_direction() {
        let mut form = SettingsForm::new();
        form.view = View::Page(Page::Suggestions);
        form.focus = 0;
        let mut cfg = sample();
        cfg.completion.enabled = true;

        assert!(form.nudge(&mut cfg, 1, ControllerKind::Sc2));
        assert!(!cfg.completion.enabled);
        assert!(form.nudge(&mut cfg, -1, ControllerKind::Sc2));
        assert!(cfg.completion.enabled);
    }

    #[test]
    fn back_walks_hub_index_page() {
        let mut form = SettingsForm::new();
        let leave = form.back(ControllerKind::Sc2);
        assert_eq!(leave.open, Some(StateId::Keyboard));
        assert!(!leave.persist);
        assert_eq!(form.view, View::Hub);

        form.focus = HUB_OPTIONS;
        let opened = form.activate(&mut sample(), ControllerKind::Sc2);
        assert!(opened.open.is_none());
        assert_eq!(form.view, View::Index);
        assert_eq!(form.focus, 0);
        assert!(form
            .footer(ControllerKind::Sc2)
            .iter()
            .any(|hint| hint.buttons == FooterButtons::OpenConfig && hint.label == "open config"));
        assert!(form
            .footer(ControllerKind::Sc2)
            .iter()
            .any(|hint| hint.buttons == FooterButtons::ShowAllControllers
                && hint.label == "show other controllers"));
        form.toggle_show_all(ControllerKind::Sc2);
        assert!(form
            .footer(ControllerKind::Sc2)
            .iter()
            .any(|hint| hint.buttons == FooterButtons::ShowAllControllers
                && hint.label == "hide other controllers"));
        form.toggle_show_all(ControllerKind::Sc2);

        let page = form.activate(&mut sample(), ControllerKind::Sc2);
        assert!(!page.persist);
        assert_eq!(form.view, View::Page(Page::Suggestions));

        form.dirty.push((RowId::Suggestions, ControllerKind::Sc2));
        let back_page = form.back(ControllerKind::Sc2);
        assert!(back_page.persist);
        assert!(back_page.open.is_none());
        assert_eq!(form.view, View::Index);
        assert_eq!(form.focus, 0);

        let back_index = form.back(ControllerKind::Sc2);
        assert!(back_index.persist);
        assert_eq!(form.view, View::Hub);
        assert_eq!(form.focus, HUB_OPTIONS);
    }

    #[test]
    fn shoulders_change_page_only_while_one_is_open() {
        let mut form = SettingsForm::new();
        let idle = form.shift_page(1, ControllerKind::Sc2);
        assert!(!idle.persist);
        assert_eq!(form.view, View::Hub);

        form.view = View::Page(Page::Debug);
        form.dirty.push((RowId::Hitboxes, ControllerKind::Sc2));
        let wrapped = form.shift_page(1, ControllerKind::Sc2);
        assert!(wrapped.persist);
        assert_eq!(form.view, View::Page(Page::Suggestions));
        assert_eq!(form.focus, 0);

        form.clear_dirty();
        let back = form.shift_page(-1, ControllerKind::Sc2);
        assert!(!back.persist);
        assert_eq!(form.view, View::Page(Page::Debug));
    }

    #[test]
    fn options_lists_only_connected_device_until_toggled() {
        let mut form = SettingsForm::new();
        form.view = View::Index;

        let connected = form.drawn(&sample(), ControllerKind::Sc2);
        let labels: Vec<&str> = connected.iter().map(|row| row.label).collect();
        assert!(labels.contains(&"Steam Controller"));
        assert!(!labels.contains(&"DualShock 4"));

        form.toggle_show_all(ControllerKind::Sc2);
        let both = form.drawn(&sample(), ControllerKind::Sc2);
        let labels: Vec<&str> = both.iter().map(|row| row.label).collect();
        assert!(labels.contains(&"Steam Controller"));
        assert!(labels.contains(&"DualShock 4"));

        let ps4_only = SettingsForm::new();
        let rows = ps4_only.drawn(&sample(), ControllerKind::Ps4);
        let _ = (rows, ps4_only);
    }

    #[test]
    fn device_index_sizes_match_family() {
        let mut form = SettingsForm::new();
        form.view = View::DeviceIndex(ControllerKind::Sc2);
        assert_eq!(form.len(ControllerKind::Sc2), 3);
        form.view = View::DeviceIndex(ControllerKind::Ps4);
        assert_eq!(form.len(ControllerKind::Ps4), 2);
        assert_eq!(
            device_pages(ControllerKind::Ps4),
            &[DevicePage::Stick, DevicePage::Triggers]
        );
        assert_eq!(
            device_pages(ControllerKind::Sc2),
            &[DevicePage::Pads, DevicePage::Stick, DevicePage::Triggers]
        );
    }

    #[test]
    fn stick_edits_follow_target_device() {
        let mut form = SettingsForm::new();
        form.view = View::DevicePage(ControllerKind::Ps4, DevicePage::Stick);
        form.focus = 0;
        let mut cfg = sample();
        cfg.ps4.stick.scale_x = 1.0;
        cfg.sc2.stick.scale_x = 1.0;

        assert!(form.nudge(&mut cfg, 1, ControllerKind::Sc2));
        assert!((cfg.ps4.stick.scale_x - 1.1).abs() < 0.001);
        assert!((cfg.sc2.stick.scale_x - 1.0).abs() < f32::EPSILON);
        assert_eq!(
            form.dirty_rows(),
            &[(RowId::HorizontalRange, ControllerKind::Ps4)]
        );
    }

    #[test]
    fn back_walks_device_value_to_index_to_options() {
        let mut form = SettingsForm::new();
        form.view = View::DevicePage(ControllerKind::Sc2, DevicePage::Stick);
        form.focus = 0;

        let to_index = form.back(ControllerKind::Sc2);
        assert!(!to_index.persist);
        assert_eq!(form.view, View::DeviceIndex(ControllerKind::Sc2));
        assert_eq!(form.focus, 1);

        let _to_options = form.back(ControllerKind::Sc2);
        assert_eq!(form.view, View::Index);
        assert_eq!(
            form.focus,
            device_entry_index(
                ControllerKind::Sc2,
                ControllerKind::Sc2,
                form.show_all_controllers
            )
        );
    }

    #[test]
    fn ps4_device_page_is_only_the_triggers() {
        let mut form = SettingsForm::new();
        form.view = View::DevicePage(ControllerKind::Ps4, DevicePage::Triggers);
        assert_eq!(form.len(ControllerKind::Ps4), 2);
        let rows = form.drawn(&sample(), ControllerKind::Ps4);
        assert_eq!(rows[0].label, "Left trigger");
        assert_eq!(rows[1].label, "Right trigger");
    }

    #[test]
    fn pads_page_holds_pad_feel() {
        let mut form = SettingsForm::new();
        form.view = View::DevicePage(ControllerKind::Sc2, DevicePage::Pads);
        assert_eq!(form.len(ControllerKind::Sc2), 10);
        let rows = form.drawn(&sample(), ControllerKind::Sc2);
        let labels: Vec<&str> = rows.iter().map(|row| row.label).collect();
        assert!(labels.contains(&"Thumb rest"));
        assert!(labels.contains(&"Pad click"));
        assert!(labels.contains(&"Stretch limit"));
        assert!(labels.contains(&"Touch settle time"));
    }

    #[test]
    fn long_lists_scroll_eight_at_a_time() {
        let mut form = SettingsForm::new();
        form.view = View::Page(Page::Suggestions);
        let total = form.len(ControllerKind::Sc2);
        assert!(total > 8);

        form.focus = 0;
        assert_eq!(form.visible_range(ControllerKind::Sc2), (0, 8));
        assert_eq!(form.scroll_counts(ControllerKind::Sc2).0, 0);
        assert_eq!(form.scroll_counts(ControllerKind::Sc2).1, total - 8);

        form.focus = total - 1;
        assert_eq!(form.visible_range(ControllerKind::Sc2), (total - 8, total));
        assert_eq!(form.scroll_counts(ControllerKind::Sc2).1, 0);
        assert_eq!(form.scroll_counts(ControllerKind::Sc2).0, total - 8);

        form.focus = 9;
        let (start, end) = form.visible_range(ControllerKind::Sc2);
        assert!(start <= 9 && 9 < end);
        assert_eq!(end - start, 8);
    }

    #[test]
    fn short_lists_show_everything() {
        let mut form = SettingsForm::new();
        form.view = View::Page(Page::Typing);
        assert_eq!(form.visible_range(ControllerKind::Sc2), (0, 2));
        assert_eq!(form.scroll_counts(ControllerKind::Sc2), (0, 0));
    }

    #[test]
    fn new_rows_round_trip() {
        let mut form = SettingsForm::new();
        let mut cfg = sample();

        form.view = View::Page(Page::Suggestions);
        form.focus = row_position(Page::Suggestions, RowId::MaxSuggestions);
        let before = cfg.completion.max_suggestions;
        assert!(form.nudge(&mut cfg, 1, ControllerKind::Sc2));
        assert_eq!(cfg.completion.max_suggestions, before + 1);

        form.focus = row_position(Page::Suggestions, RowId::Backend);
        assert!(form.nudge(&mut cfg, 1, ControllerKind::Sc2));
        assert_ne!(cfg.completion.backend, sample().completion.backend);

        form.focus = row_position(Page::Suggestions, RowId::LearnOnAccept);
        let enabled = cfg.completion.learn_on_accept;
        assert!(form.nudge(&mut cfg, 1, ControllerKind::Sc2));
        assert_eq!(cfg.completion.learn_on_accept, !enabled);

        form.view = View::DevicePage(ControllerKind::Sc2, DevicePage::Pads);
        form.focus = row_position_device(DevicePage::Pads, RowId::PadSettleMs);
        assert!(form.nudge(&mut cfg, 1, ControllerKind::Sc2));
        assert_eq!(cfg.sc2.pad_origin_settle_ms, 25);

        form.view = View::Page(Page::Overlay);
        form.focus = row_position(Page::Overlay, RowId::BatteryButton);
        assert!(form.nudge(&mut cfg, 1, ControllerKind::Sc2));
        assert!(cfg.battery.draw_button);
    }

    fn row_position(page: Page, id: RowId) -> usize {
        page.rows().iter().position(|row| *row == id).unwrap()
    }

    fn row_position_device(page: DevicePage, id: RowId) -> usize {
        page.rows(ControllerKind::Sc2)
            .iter()
            .position(|row| *row == id)
            .unwrap()
    }

    #[test]
    fn device_page_titles_carry_device() {
        assert_eq!(
            device_page_title(ControllerKind::Sc2, DevicePage::Pads),
            "Steam Controller Pads"
        );
        assert_eq!(
            device_page_title(ControllerKind::Sc2, DevicePage::Stick),
            "Steam Controller Stick"
        );
        assert_eq!(
            device_page_title(ControllerKind::Sc2, DevicePage::Triggers),
            "Steam Controller Triggers"
        );
        assert_eq!(
            device_page_title(ControllerKind::Ps4, DevicePage::Stick),
            "DualShock 4 Stick"
        );
        assert_eq!(
            device_page_title(ControllerKind::Ps4, DevicePage::Triggers),
            "DualShock 4 Triggers"
        );
    }
}
