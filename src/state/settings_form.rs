use crate::completion::settings::Preselect;
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

const PAGES: [Page; 6] = [
    Page::Suggestions,
    Page::Overlay,
    Page::Typing,
    Page::Sticks,
    Page::Controller,
    Page::Debug,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::state) enum View {
    Hub,
    Index,
    Page(Page),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::state) enum Page {
    Suggestions,
    Overlay,
    Typing,
    Sticks,
    Controller,
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
    HorizontalRange,
    VerticalRange,
    SquareStick,
    Stickiness,
    HoldAfterKey,
    ThumbRest,
    StretchShortSide,
    PadClick,
    Sc2TriggerLeft,
    Sc2TriggerRight,
    Ps4TriggerLeft,
    Ps4TriggerRight,
    StickCursors,
    Hitboxes,
    StickBounds,
    ReachOverlay,
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
}

pub(in crate::state) struct FooterHint {
    pub buttons: FooterButtons,
    pub label: &'static str,
}

pub(in crate::state) struct SettingsForm {
    view: View,
    focus: usize,
    dirty: Vec<RowId>,
}

impl SettingsForm {
    pub(in crate::state) fn new() -> Self {
        Self {
            view: View::Hub,
            focus: 0,
            dirty: Vec::new(),
        }
    }

    pub(in crate::state) fn focus(&self) -> usize {
        self.focus
    }

    pub(in crate::state) fn on_page(&self) -> bool {
        matches!(self.view, View::Page(_))
    }

    pub(in crate::state) fn set_focus(&mut self, index: usize, kind: ControllerKind) {
        self.focus = index;
        self.ensure_focus(kind);
    }

    pub(in crate::state) fn dirty_rows(&self) -> &[RowId] {
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

    pub(in crate::state) fn back(&mut self) -> Effect {
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
            View::Page(page) => {
                self.view = View::Index;
                self.focus = page_index(page);
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
                let page = PAGES[self.focus];
                self.view = View::Page(page);
                self.focus = 0;
                Effect::none()
            }
            View::Page(_) => {
                let Some(id) = self.focused_row(kind) else {
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

    pub(in crate::state) fn shift_page(&mut self, dir: i32) -> Effect {
        let View::Page(page) = self.view else {
            return Effect::none();
        };
        let persist = !self.dirty.is_empty();
        let n = PAGES.len() as i32;
        let next = PAGES[(page_index(page) as i32 + dir).rem_euclid(n) as usize];
        self.view = View::Page(next);
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
        let Some(id) = self.focused_row(kind) else {
            return false;
        };
        let before = format_value(cfg, id);
        apply_dir(cfg, id, dir);
        if format_value(cfg, id) == before {
            return false;
        }
        if !self.dirty.contains(&id) {
            self.dirty.push(id);
        }
        true
    }

    pub(in crate::state) fn title(&self) -> &'static str {
        match self.view {
            View::Hub => "Settings",
            View::Index => "Options",
            View::Page(page) => page.title(),
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
            ],
            View::Page(_) => {
                let mut hints = Vec::new();
                if self.focused_row(kind).is_some_and(is_toggle) {
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
            View::Index => PAGES
                .iter()
                .map(|page| DrawnRow {
                    label: page.title(),
                    explain: None,
                    value: None,
                })
                .collect(),
            View::Page(page) => page
                .rows(kind)
                .iter()
                .copied()
                .map(|id| {
                    let (label, explain) = row_text(id);
                    DrawnRow {
                        label,
                        explain: Some(explain),
                        value: Some(format_value(cfg, id)),
                    }
                })
                .collect(),
        }
    }

    fn len(&self, kind: ControllerKind) -> usize {
        match self.view {
            View::Hub => HUB_LEN,
            View::Index => PAGES.len(),
            View::Page(page) => page.rows(kind).len(),
        }
    }

    fn focused_row(&self, kind: ControllerKind) -> Option<RowId> {
        let View::Page(page) = self.view else {
            return None;
        };
        page.rows(kind).get(self.focus).copied()
    }
}

impl Page {
    fn title(self) -> &'static str {
        match self {
            Page::Suggestions => "Suggestions",
            Page::Overlay => "Overlay",
            Page::Typing => "Typing",
            Page::Sticks => "Sticks",
            Page::Controller => "Controller",
            Page::Debug => "Debug",
        }
    }

    fn rows(self, kind: ControllerKind) -> &'static [RowId] {
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
            ],
            Page::Overlay => &[
                RowId::SeeThrough,
                RowId::KeyboardOpacity,
                RowId::MenuOpacity,
                RowId::KeyWidth,
                RowId::KeyHeight,
            ],
            Page::Typing => &[RowId::DelayBeforeRepeat, RowId::RepeatInterval],
            Page::Sticks => &[
                RowId::HorizontalRange,
                RowId::VerticalRange,
                RowId::SquareStick,
                RowId::Stickiness,
                RowId::HoldAfterKey,
            ],
            Page::Controller => match kind {
                ControllerKind::Ps4 => &[RowId::Ps4TriggerLeft, RowId::Ps4TriggerRight],
                ControllerKind::Sc2 | ControllerKind::Replay => &[
                    RowId::ThumbRest,
                    RowId::StretchShortSide,
                    RowId::PadClick,
                    RowId::Sc2TriggerLeft,
                    RowId::Sc2TriggerRight,
                ],
            },
            Page::Debug => &[
                RowId::StickCursors,
                RowId::Hitboxes,
                RowId::StickBounds,
                RowId::ReachOverlay,
            ],
        }
    }
}

fn page_index(page: Page) -> usize {
    PAGES.iter().position(|p| *p == page).unwrap_or(0)
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
    )
}

pub(in crate::state) fn format_value(cfg: &Config, id: RowId) -> String {
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
        RowId::HorizontalRange => format_f32(cfg.stick_scale_x, STICK_RANGE_DIGITS),
        RowId::VerticalRange => format_f32(cfg.stick_scale_y, STICK_RANGE_DIGITS),
        RowId::SquareStick => format_f32(cfg.stick_warp, UNIT_DIGITS),
        RowId::Stickiness => format_f32(cfg.stick_select_sticky, UNIT_DIGITS),
        RowId::HoldAfterKey => format!("{} ms", cfg.stick_select_lock_ms),
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
    }
}

pub(in crate::state) fn copy_row(dst: &mut Config, src: &Config, id: RowId) {
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
        RowId::HorizontalRange => dst.stick_scale_x = src.stick_scale_x,
        RowId::VerticalRange => dst.stick_scale_y = src.stick_scale_y,
        RowId::SquareStick => dst.stick_warp = src.stick_warp,
        RowId::Stickiness => dst.stick_select_sticky = src.stick_select_sticky,
        RowId::HoldAfterKey => dst.stick_select_lock_ms = src.stick_select_lock_ms,
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
    }
}

fn apply_dir(cfg: &mut Config, id: RowId, dir: i32) {
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
        RowId::HorizontalRange => {
            cfg.stick_scale_x = step_f32(
                cfg.stick_scale_x,
                STICK_RANGE_MIN,
                STICK_RANGE_MAX,
                STICK_RANGE_STEP,
                dir,
                STICK_RANGE_DIGITS,
            )
        }
        RowId::VerticalRange => {
            cfg.stick_scale_y = step_f32(
                cfg.stick_scale_y,
                STICK_RANGE_MIN,
                STICK_RANGE_MAX,
                STICK_RANGE_STEP,
                dir,
                STICK_RANGE_DIGITS,
            )
        }
        RowId::SquareStick => {
            cfg.stick_warp = step_f32(
                cfg.stick_warp,
                UNIT_MIN,
                UNIT_MAX,
                UNIT_STEP,
                dir,
                UNIT_DIGITS,
            )
        }
        RowId::Stickiness => {
            cfg.stick_select_sticky = step_f32(
                cfg.stick_select_sticky,
                STICKY_MIN,
                STICKY_MAX,
                UNIT_STEP,
                dir,
                UNIT_DIGITS,
            )
        }
        RowId::HoldAfterKey => {
            cfg.stick_select_lock_ms =
                step_u64(cfg.stick_select_lock_ms, 0, LOCK_MAX, LOCK_STEP, dir)
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
    let next = value as i32 + dir;
    next.clamp(min as i32, max as i32) as usize
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
        let leave = form.back();
        assert_eq!(leave.open, Some(StateId::Keyboard));
        assert!(!leave.persist);
        assert_eq!(form.view, View::Hub);

        form.focus = HUB_OPTIONS;
        let opened = form.activate(&mut sample(), ControllerKind::Sc2);
        assert!(opened.open.is_none());
        assert_eq!(form.view, View::Index);
        assert_eq!(form.focus, 0);

        let page = form.activate(&mut sample(), ControllerKind::Sc2);
        assert!(!page.persist);
        assert_eq!(form.view, View::Page(Page::Suggestions));

        form.dirty.push(RowId::Suggestions);
        let back_page = form.back();
        assert!(back_page.persist);
        assert!(back_page.open.is_none());
        assert_eq!(form.view, View::Index);
        assert_eq!(form.focus, 0);

        let back_index = form.back();
        assert!(back_index.persist);
        assert_eq!(form.view, View::Hub);
        assert_eq!(form.focus, HUB_OPTIONS);
    }

    #[test]
    fn shoulders_change_page_only_while_one_is_open() {
        let mut form = SettingsForm::new();
        let idle = form.shift_page(1);
        assert!(!idle.persist);
        assert_eq!(form.view, View::Hub);

        form.view = View::Page(Page::Debug);
        form.dirty.push(RowId::Hitboxes);
        let wrapped = form.shift_page(1);
        assert!(wrapped.persist);
        assert_eq!(form.view, View::Page(Page::Suggestions));
        assert_eq!(form.focus, 0);

        form.clear_dirty();
        let back = form.shift_page(-1);
        assert!(!back.persist);
        assert_eq!(form.view, View::Page(Page::Debug));
    }

    #[test]
    fn ps4_page_is_only_the_triggers() {
        let mut form = SettingsForm::new();
        form.view = View::Page(Page::Controller);
        assert_eq!(form.len(ControllerKind::Ps4), 2);
        assert_eq!(form.len(ControllerKind::Sc2), 5);
        let rows = form.drawn(&sample(), ControllerKind::Ps4);
        assert_eq!(rows[0].label, "Left trigger");
        assert_eq!(rows[1].label, "Right trigger");
    }
}
