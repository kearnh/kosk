use std::collections::HashMap;
use std::fs;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::config;
use crate::controller::bindings::BindingEngine;
use crate::controller::record as input_record;
use crate::controller::record::{MappingScales, TapeHeader};
use crate::controller::BatteryStatus;
use crate::controller::StickSide;
use crate::state::actions::load_bindings;
use crate::state::keyboard::layout::KeyboardLayout;
use crate::when::WhenContext;
use crate::{
    controller::ControllerInput,
    controller::{ControllerBinding, ControllerButton},
    state::{
        event::{Event, EventQueue, EventSource},
        keyboard::key::RawKey,
        StateId,
    },
};
use anyhow::Result;
use egui::{Context, Ui};

pub(crate) mod display_icon;
mod geom;
pub(crate) mod geometry_snap;
mod key;
mod keyboard_action;
mod layout;
mod reach_extent;
mod when;

pub use crate::state::keyboard::keyboard_action::KeyboardAction;

const BATTERY_PCT_EMPTY: u8 = 10;
const BATTERY_PCT_LOW: u8 = 35;
const BATTERY_PCT_MEDIUM: u8 = 65;
const BATTERY_PCT_HIGH: u8 = 90;

fn battery_icon_name(status: Option<BatteryStatus>) -> &'static str {
    let Some(s) = status else {
        return "battery-empty";
    };
    if s.charging {
        return "battery-charging";
    }
    match s.percent {
        0..=BATTERY_PCT_EMPTY => "battery-empty",
        p if p <= BATTERY_PCT_LOW => "battery-low",
        p if p <= BATTERY_PCT_MEDIUM => "battery-medium",
        p if p <= BATTERY_PCT_HIGH => "battery-high",
        _ => "battery-full",
    }
}

fn battery_percent_text(status: Option<BatteryStatus>) -> String {
    match status {
        None => "--".to_string(),
        Some(s) => format!("{}%", s.percent),
    }
}

fn rgba(c: [u8; 4]) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3])
}

fn battery_color(status: Option<BatteryStatus>, cfg: &config::BatteryConfig) -> egui::Color32 {
    let c = match status {
        None => cfg.unknown,
        Some(s) if s.charging => cfg.charging,
        Some(s) => match s.percent {
            0..=BATTERY_PCT_EMPTY => cfg.empty,
            p if p <= BATTERY_PCT_LOW => cfg.low,
            p if p <= BATTERY_PCT_MEDIUM => cfg.medium,
            p if p <= BATTERY_PCT_HIGH => cfg.high,
            _ => cfg.full,
        },
    };
    rgba(c)
}

fn draw_battery(
    ui: &mut Ui,
    layout: &KeyboardLayout,
    batt: &layout::BatteryItem,
    row_height: f32,
    status: Option<BatteryStatus>,
    cfg: &config::BatteryConfig,
    label_cache: &mut display_icon::LabelCache,
) {
    let font_size = batt.font_size.unwrap_or(layout.font_size);
    let color = battery_color(status, cfg);
    let icon_name = battery_icon_name(status);
    let icon = format!("{{icon:{icon_name}:fill}}");
    let icon_label = label_cache.get(&icon, row_height * 0.9, color);
    let size = egui::Vec2::new(layout.scale_x(batt.width), row_height);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());

    if cfg.draw_button {
        ui.painter().rect_filled(
            rect,
            ui.visuals().widgets.inactive.corner_radius,
            ui.visuals().widgets.inactive.bg_fill,
        );
    }

    let icon_size = row_height * 0.9;
    let icon_center = egui::Pos2::new(rect.left() + icon_size * 0.5, rect.center().y);
    let icon_rect = egui::Rect::from_center_size(icon_center, egui::Vec2::splat(icon_size));
    ui.put(icon_rect, egui::Label::new(icon_label));
    let text_color = ui.style().visuals.widgets.inactive.fg_stroke.color;
    ui.painter().text(
        egui::Pos2::new(icon_rect.right() + 4.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        battery_percent_text(status),
        egui::FontId::proportional(font_size),
        text_color,
    );
}

fn stick_side_sources(left: bool) -> [EventSource; 2] {
    if left {
        [
            EventSource::Controller(ControllerBinding::Single(ControllerButton::TriggerLeft)),
            EventSource::Controller(ControllerBinding::Single(ControllerButton::PadLeft)),
        ]
    } else {
        [
            EventSource::Controller(ControllerBinding::Single(ControllerButton::TriggerRight)),
            EventSource::Controller(ControllerBinding::Single(ControllerButton::PadRight)),
        ]
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct StickCells {
    left: Option<(usize, usize)>,
    right: Option<(usize, usize)>,
}

#[derive(Default)]
pub struct KeyboardState {
    layouts: HashMap<String, KeyboardLayout>,
    current_layout: String,
    selected: StickCells,
    shift_state: bool,
    shift_mod: bool,
    ctrl_mod: bool,
    alt_mod: bool,
    bindings: BindingEngine<KeyboardAction>,
    last_left_stick_action: Option<Instant>,
    last_right_stick_action: Option<Instant>,
    stick_select_lock_ms: Duration,
    stick_select_sticky: f32,
    label_cache: display_icon::LabelCache,
    config: Option<config::Config>,
    last_battery: Option<BatteryStatus>,
    feed_completion_log: bool,
}

impl KeyboardState {
    fn cfg(&self) -> config::Config {
        match &self.config {
            Some(c) => c.clone(),
            None => config::get(),
        }
    }

    pub fn new() -> Result<Self> {
        Self::construct(None)
    }

    pub fn with_config(config: config::Config) -> Result<Self> {
        Self::construct(Some(config))
    }

    fn construct(config: Option<config::Config>) -> Result<Self> {
        let mut state = Self {
            config,
            ..Default::default()
        };
        state.reload_from_config()?;
        state.current_layout = state.cfg().start_layout;
        state.feed_completion_log = true;
        Ok(state)
    }

    pub fn send_key(
        &mut self,
        key: &RawKey,
        events: &mut EventQueue,
        source: &EventSource,
    ) -> Result<()> {
        match key {
            RawKey::Key(c) => {
                let mut steps = Vec::new();
                if self.shift_mod {
                    steps.push(Event::SendKey(enigo::Key::Shift, enigo::Direction::Press));
                }
                if self.ctrl_mod {
                    steps.push(Event::SendKey(enigo::Key::Control, enigo::Direction::Press));
                }
                if self.alt_mod {
                    steps.push(Event::SendKey(enigo::Key::Alt, enigo::Direction::Press));
                }
                // `enigo::text` (SendText) injects Unicode and ignores held modifiers, so
                // Ctrl/Alt/Shift chords never reach the app. Virtual-key click does combine.
                // Without mods, SendText is required: SendKey does not emit uppercase letters.
                if self.ctrl_mod || self.alt_mod || self.shift_mod {
                    steps.push(Event::SendKey(
                        enigo::Key::Unicode(*c),
                        enigo::Direction::Click,
                    ));
                } else {
                    steps.push(Event::SendText(c.to_string()));
                }
                if self.alt_mod {
                    steps.push(Event::SendKey(enigo::Key::Alt, enigo::Direction::Release));
                }
                if self.ctrl_mod {
                    steps.push(Event::SendKey(
                        enigo::Key::Control,
                        enigo::Direction::Release,
                    ));
                }
                if self.shift_mod {
                    steps.push(Event::SendKey(enigo::Key::Shift, enigo::Direction::Release));
                }
                if events.push_seq(steps, source) {
                    self.shift_state = false;
                    self.shift_mod = false;
                    self.ctrl_mod = false;
                    self.alt_mod = false;
                    self.note_outgoing(key);
                }
            }
            RawKey::Enigo(k) => {
                let mut steps = Vec::new();
                if self.shift_mod {
                    steps.push(Event::SendKey(enigo::Key::Shift, enigo::Direction::Press));
                }
                if self.ctrl_mod {
                    steps.push(Event::SendKey(enigo::Key::Control, enigo::Direction::Press));
                }
                if self.alt_mod {
                    steps.push(Event::SendKey(enigo::Key::Alt, enigo::Direction::Press));
                }
                steps.push(Event::SendKey(*k, enigo::Direction::Click));
                if self.alt_mod {
                    steps.push(Event::SendKey(enigo::Key::Alt, enigo::Direction::Release));
                }
                if self.ctrl_mod {
                    steps.push(Event::SendKey(
                        enigo::Key::Control,
                        enigo::Direction::Release,
                    ));
                }
                if self.shift_mod {
                    steps.push(Event::SendKey(enigo::Key::Shift, enigo::Direction::Release));
                }
                if events.push_seq(steps, source) {
                    self.shift_state = false;
                    self.shift_mod = false;
                    self.ctrl_mod = false;
                    self.alt_mod = false;
                    self.note_outgoing(key);
                }
            }
            RawKey::Action(action) => {
                self.do_action(action, events, source)?;
            }
            RawKey::Text(text) => {
                if events.push(Event::SendText(text.to_owned()), source) {
                    self.note_outgoing(key);
                }
            }
            _ => return Ok(()),
        }

        Ok(())
    }

    pub fn set_feed_completion_log(&mut self, feed: bool) {
        self.feed_completion_log = feed;
    }

    fn note_outgoing(&mut self, key: &RawKey) {
        if !self.feed_completion_log {
            return;
        }
        crate::completion::with_mut(|sess| {
            let Some(sess) = sess else {
                return;
            };
            if self.ctrl_mod || self.alt_mod {
                sess.note_log(crate::completion::LogEvent::CtrlAlt, "");
                return;
            }
            match key {
                RawKey::Key('\n') | RawKey::Enigo(enigo::Key::Return) => {
                    if sess.cfg().learn_on_submit && sess.armed() {
                        let words = crate::completion::tokens_in(sess.typed_text(), sess.cfg());
                        sess.learn(&words);
                    }
                    sess.note_log(crate::completion::LogEvent::Enter, "");
                }
                RawKey::Key(c) => sess.note_log(crate::completion::LogEvent::Char(*c), ""),
                RawKey::Text(t) => sess.note_log(crate::completion::LogEvent::Text, t),
                RawKey::Enigo(enigo::Key::Backspace) => {
                    sess.note_log(crate::completion::LogEvent::Backspace, "")
                }
                RawKey::Enigo(
                    enigo::Key::LeftArrow
                    | enigo::Key::RightArrow
                    | enigo::Key::UpArrow
                    | enigo::Key::DownArrow
                    | enigo::Key::Home
                    | enigo::Key::End
                    | enigo::Key::Delete,
                ) => sess.note_log(crate::completion::LogEvent::Arrow, ""),
                _ => {}
            }
            if sess.armed() {
                let text = sess.typed_text().to_string();
                let n = text.len();
                sess.request_from_buffer(&text, n);
            }
        });
    }

    fn completion_cycle(&mut self, forward: bool) {
        crate::completion::with_mut(|s| {
            if let Some(s) = s {
                s.cycle(forward);
            }
        });
    }

    fn completion_toggle(&mut self) {
        crate::completion::with_mut(|s| {
            if let Some(s) = s {
                s.toggle_armed();
            }
        });
    }

    fn enqueue_accept(
        events: &mut EventQueue,
        source: &EventSource,
        out: &crate::completion::AcceptOutcome,
    ) -> bool {
        use crate::completion::settings::AcceptVia;
        match out.via {
            AcceptVia::Suffix => {
                if out.inject.is_empty() {
                    return true;
                }
                events.push(Event::SendText(out.inject.clone()), source)
            }
            AcceptVia::BackspaceReplace => {
                let mut steps = Vec::with_capacity(out.token_char_len.saturating_add(1));
                for _ in 0..out.token_char_len {
                    steps.push(Event::SendKey(
                        enigo::Key::Backspace,
                        enigo::Direction::Click,
                    ));
                }
                if !out.inject.is_empty() {
                    steps.push(Event::SendText(out.inject.clone()));
                }
                events.push_seq(steps, source)
            }
        }
    }

    fn completion_accept(
        events: &mut EventQueue,
        source: &EventSource,
        index: Option<usize>,
    ) -> bool {
        use crate::completion::settings::AcceptVia;
        let Some(out) = crate::completion::with_mut(|s| s.and_then(|s| s.take_accept(index)))
        else {
            return false;
        };
        if !Self::enqueue_accept(events, source, &out) {
            return true;
        }
        match out.via {
            AcceptVia::Suffix => {
                if !out.inject.is_empty() {
                    crate::completion::with_mut(|s| {
                        if let Some(s) = s {
                            s.note_log(crate::completion::LogEvent::Text, &out.inject);
                            let t = s.typed_text().to_string();
                            let n = t.len();
                            s.request_from_buffer(&t, n);
                        }
                    });
                }
            }
            AcceptVia::BackspaceReplace => {
                if !out.inject.is_empty() {
                    crate::completion::with_mut(|s| {
                        if let Some(s) = s {
                            for _ in 0..out.token_char_len {
                                s.note_log(crate::completion::LogEvent::Backspace, "");
                            }
                            s.note_log(crate::completion::LogEvent::Text, &out.inject);
                            let t = s.typed_text().to_string();
                            let n = t.len();
                            s.request_from_buffer(&t, n);
                        }
                    });
                }
            }
        }
        true
    }

    fn completion_cancel(&mut self, events: &mut EventQueue, source: &EventSource) {
        crate::completion::with_mut(|s| {
            let Some(s) = s else {
                return;
            };
            if !s.cfg().keyboard.retract_last_accept {
                s.clear_highlight();
                return;
            }
            if let Some(inj) = s.last_injected().map(str::to_string) {
                let steps: Vec<Event> = inj
                    .chars()
                    .map(|_| Event::SendKey(enigo::Key::Backspace, enigo::Direction::Click))
                    .collect();
                if events.push_seq(steps, source) {
                    for _ in inj.chars() {
                        s.note_log(crate::completion::LogEvent::Backspace, "");
                    }
                }
                s.set_last_injected(None);
            }
            s.clear_highlight();
        });
    }

    fn send_under_stick(
        &mut self,
        left: bool,
        events: &mut EventQueue,
        source: &EventSource,
    ) -> Result<()> {
        let cell = if left {
            self.selected.left
        } else {
            self.selected.right
        };
        let Some(key) = cell.and_then(|cell| self.raw_key_at_selected(cell)) else {
            return Ok(());
        };
        if left {
            self.last_left_stick_action = Some(Instant::now());
        } else {
            self.last_right_stick_action = Some(Instant::now());
        }
        self.send_key(&key, events, source)
    }

    pub(crate) fn when_context(&self) -> WhenContext {
        let session = input_record::session();
        let (suggestion_selected, completion_active) = crate::completion::with_mut(|s| {
            s.map(|s| (s.highlight().is_some(), s.armed()))
                .unwrap_or((false, false))
        });
        WhenContext {
            shift: self.shift_state,
            recording: session.is_recording(),
            replay: session.is_replay(),
            ctrl: self.ctrl_mod,
            alt: self.alt_mod,
            suggestion_selected,
            completion_active,
        }
    }

    fn do_action(
        &mut self,
        action: &KeyboardAction,
        events: &mut EventQueue,
        source: &EventSource,
    ) -> Result<()> {
        use KeyboardAction::*;
        match action {
            SendKeyUnderLeftStick => self.send_under_stick(true, events, source)?,
            SendKeyUnderRightStick => self.send_under_stick(false, events, source)?,
            SendKey(key) => {
                self.send_key(&RawKey::Key(*key), events, source)?;
            }
            SendEnigoKey(key) => {
                self.send_key(&RawKey::Enigo(*key), events, source)?;
            }
            ToggleShift => {
                let _ = events.push(Event::ToggleShift, source);
            }
            ToggleCtrl => {
                let _ = events.push(Event::ToggleCtrl, source);
            }
            ToggleAlt => {
                let _ = events.push(Event::ToggleAlt, source);
            }
            Paste => {
                if events.push_seq(
                    vec![
                        Event::SendKey(enigo::Key::Control, enigo::Direction::Press),
                        Event::SendKey(enigo::Key::Unicode('v'), enigo::Direction::Click),
                        Event::SendKey(enigo::Key::Control, enigo::Direction::Release),
                    ],
                    source,
                ) {
                    crate::completion::with_mut(|s| {
                        if let Some(s) = s {
                            s.note_log(crate::completion::LogEvent::Paste, "");
                        }
                    });
                }
            }
            CycleSuggestion => self.completion_cycle(true),
            CycleSuggestionPrev => self.completion_cycle(false),
            ToggleCompletion => self.completion_toggle(),
            CancelSuggestion => self.completion_cancel(events, source),
            AcceptSuggestion(i) => {
                let _ = Self::completion_accept(events, source, *i);
            }
            SwitchState(state) => {
                let _ = events.push(Event::ChangeState(*state), source);
            }
            SwitchLayout(layout_name) => {
                self.set_current_layout(layout_name)?;
                input_record::session().tap_layout(layout_name);
            }
            FlipWindowLeftRight => {
                let _ = events.push(Event::FlipWindowLeftRight, source);
            }
            FlipWindowAboveBelow => {
                let _ = events.push(Event::FlipWindowAboveBelow, source);
            }
            RotateWindow => {
                let _ = events.push(Event::RotateWindow, source);
            }
            Exit => {
                let _ = events.push(Event::Exit, source);
            }
            ToggleRecord => {
                let _ = events.push(Event::ToggleRecord, source);
            }
        }
        Ok(())
    }

    pub(crate) fn tape_header(&self) -> Result<TapeHeader> {
        let cfg = self.cfg();
        let mut layouts: Vec<(String, String)> = self
            .layouts
            .iter()
            .map(|(name, layout)| (name.clone(), layout.source().to_owned()))
            .collect();
        layouts.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(TapeHeader {
            version: crate::controller::record::CURRENT_TAPE_VERSION,
            current_layout: self.current_layout.clone(),
            scales: MappingScales {
                scale_x: cfg.scale_x,
                scale_y: cfg.scale_y,
                stick_scale_x: cfg.stick_scale_x,
                stick_scale_y: cfg.stick_scale_y,
            },
            config_toml: Some(config::tape_config_toml(&cfg)?),
            layouts,
        })
    }

    pub(crate) fn install_recorded_layouts(&mut self, header: &TapeHeader) -> Result<()> {
        let mut map = HashMap::new();
        for (name, toml) in &header.layouts {
            map.insert(
                name.clone(),
                KeyboardLayout::load_with_scales(
                    toml,
                    header.scales.scale_x,
                    header.scales.scale_y,
                    header.scales.stick_scale_x,
                    header.scales.stick_scale_y,
                )?,
            );
        }
        if !map.contains_key(&header.current_layout) {
            return Err(anyhow::anyhow!(
                "recorded current_layout '{}' is not in the tape header",
                header.current_layout
            ));
        }
        self.layouts = map;
        self.current_layout = header.current_layout.clone();
        self.selected = StickCells::default();
        self.on_layouts_changed();
        Ok(())
    }

    pub(crate) fn set_current_layout(&mut self, name: &str) -> Result<()> {
        if !self.layouts.contains_key(name) {
            return Err(anyhow::anyhow!("Layout '{}' not found", name));
        }
        if self.current_layout != name {
            self.current_layout = name.to_string();
            self.on_layouts_changed();
        }
        self.selected = StickCells::default();
        Ok(())
    }

    /// Bookkeeping after layouts are replaced or the current layout name changes.
    fn on_layouts_changed(&mut self) {
        for layout in self.layouts.values_mut() {
            layout.clear_captured_geometry();
        }
        if self.config.is_none() {
            geometry_snap::clear();
        }
        self.label_cache.clear();
    }

    pub(crate) fn toggle_shift(&mut self) {
        if self.shift_state || self.shift_mod {
            self.shift_state = false;
            self.shift_mod = false;
        } else {
            if self.ctrl_mod || self.alt_mod {
                self.shift_mod = !self.shift_mod
            } else {
                self.shift_state = !self.shift_state;
            }
        }
    }

    pub(crate) fn toggle_ctrl(&mut self) {
        self.ctrl_mod = !self.ctrl_mod;
    }

    pub(crate) fn toggle_alt(&mut self) {
        self.alt_mod = !self.alt_mod;
    }

    pub fn draw_ui(&mut self, ctx: &Context, ui: &mut Ui, events: &mut EventQueue) {
        self.feed_completion_log = true;
        if let Some(key) = self.draw_keyboard_ui(ctx, ui, events) {
            self.send_key(&key, events, &EventSource::MouseClick)
                .expect("send key");
        }
    }

    pub fn content_width(&self) -> f32 {
        let Some(layout) = self.layouts.get(&self.current_layout) else {
            return 400.0;
        };
        let pad_x = layout.scale_x(layout.pad_x);
        layout.left_content_width(pad_x)
    }

    pub fn reset_controller_input(&mut self, holdover: Option<&dyn ControllerInput>) {
        if holdover.is_none() {
            self.selected = StickCells::default();
        }
        self.bindings.reset(holdover);
    }

    fn raw_key_at_selected(&self, cell: (usize, usize)) -> Option<RawKey> {
        self.layouts
            .get(&self.current_layout)?
            .key_at_cell(cell, self.shift_state)
    }

    pub fn handle_controller_input(
        &mut self,
        input: &dyn ControllerInput,
        events: &mut EventQueue,
    ) -> Result<()> {
        self.note_battery(input.battery());

        let current_layout = self
            .layouts
            .get(&self.current_layout)
            .ok_or_else(|| anyhow::anyhow!("Current layout '{}' not found", self.current_layout))?;

        let prev_selected = self.selected;

        let selected_left = current_layout.nearest_cell(
            StickSide::Left,
            input.left_pad().unwrap_or_else(|| input.left_stick()),
            prev_selected.left,
            self.stick_select_sticky,
        );
        let selected_right = current_layout.nearest_cell(
            StickSide::Right,
            input.right_pad().unwrap_or_else(|| input.right_stick()),
            prev_selected.right,
            self.stick_select_sticky,
        );

        let lock_left = self
            .last_left_stick_action
            .is_some_and(|t| t.elapsed() <= self.stick_select_lock_ms);
        let lock_right = self
            .last_right_stick_action
            .is_some_and(|t| t.elapsed() <= self.stick_select_lock_ms);

        let new_left = if lock_left {
            prev_selected.left
        } else {
            selected_left
        };
        let new_right = if lock_right {
            prev_selected.right
        } else {
            selected_right
        };

        if prev_selected.left != new_left {
            events.clear_toggle_suppress(stick_side_sources(true));
        }
        if prev_selected.right != new_right {
            events.clear_toggle_suppress(stick_side_sources(false));
        }

        self.selected = StickCells {
            left: new_left,
            right: new_right,
        };
        if !lock_left {
            self.last_left_stick_action = None;
        }
        if !lock_right {
            self.last_right_stick_action = None;
        }

        for (binding, action) in self.bindings.evaluate(input, &self.when_context()) {
            let src = EventSource::Controller(binding);
            self.do_action(&action, events, &src)?;
        }

        Ok(())
    }

    pub fn note_battery(&mut self, battery: Option<BatteryStatus>) {
        if let Some(b) = battery {
            self.last_battery = Some(b);
        }
    }

    pub(crate) fn draw_keyboard_ui(
        &mut self,
        ctx: &Context,
        ui: &mut Ui,
        events: &mut EventQueue,
    ) -> Option<RawKey> {
        let display_ctx = self.when_context();

        let show_chips = self.feed_completion_log
            && self.cfg().completion.enabled
            && self.cfg().completion.show_in_keyboard;

        let publish_geometry = self.config.is_none();
        let batt_cfg = self.cfg().battery;

        // Split field borrows so label_cache and layouts can be used together.
        let KeyboardState {
            layouts,
            current_layout: current_layout_name,
            label_cache,
            selected,
            shift_state,
            shift_mod,
            ctrl_mod,
            alt_mod,
            last_battery,
            ..
        } = self;

        let current_layout = layouts.get_mut(current_layout_name)?;

        let mut pressed_key: Option<RawKey> = None;

        // Set semi-transparent button styling
        let style = ui.style_mut();
        style.visuals.widgets.inactive.weak_bg_fill =
            egui::Color32::from_rgba_premultiplied(60, 60, 60, 128);
        style.visuals.widgets.inactive.bg_fill =
            egui::Color32::from_rgba_premultiplied(60, 60, 60, 128);
        style.visuals.widgets.inactive.fg_stroke.color = egui::Color32::WHITE;
        style.visuals.widgets.hovered.weak_bg_fill =
            egui::Color32::from_rgba_premultiplied(80, 80, 80, 180);
        style.visuals.widgets.hovered.bg_fill =
            egui::Color32::from_rgba_premultiplied(80, 80, 80, 180);
        style.visuals.widgets.hovered.fg_stroke.color = egui::Color32::WHITE;
        style.visuals.widgets.active.weak_bg_fill =
            egui::Color32::from_rgba_premultiplied(100, 100, 100, 200);
        style.visuals.widgets.active.bg_fill =
            egui::Color32::from_rgba_premultiplied(100, 100, 100, 200);
        style.visuals.widgets.active.fg_stroke.color = egui::Color32::WHITE;
        style.visuals.selection.bg_fill = egui::Color32::from_rgba_premultiplied(50, 100, 180, 220);
        style.visuals.selection.stroke.color = egui::Color32::WHITE;
        // Keep key slots at TOML widths: padding + icon glyphs must not expand the row.
        style.spacing.button_padding = egui::Vec2::ZERO;

        let pad_x = current_layout.scale_x(current_layout.pad_x);
        let pad_y = current_layout.scale_y(current_layout.pad_y);

        let mut captured_data = Vec::new();

        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing = egui::Vec2::new(pad_x, pad_y);

            let left_rest = current_layout.nearest_cell(StickSide::Left, (0.0, 0.0), None, 1.0);
            let right_rest = current_layout.nearest_cell(StickSide::Right, (0.0, 0.0), None, 1.0);
            let content_width = current_layout.left_content_width(pad_x);

            if show_chips {
                let token = crate::completion::with_mut(|s| {
                    s.and_then(|s| {
                        crate::completion::CompletionContext::from_buffer(
                            s.typed_text(),
                            s.typed_text().len(),
                            s.cfg(),
                        )
                        .map(|c| c.token)
                    })
                    .unwrap_or_default()
                });
                if let Some(i) =
                    crate::state::completion_ui::draw_session_strip(ui, content_width, &token)
                {
                    Self::completion_accept(events, &EventSource::MouseClick, Some(i));
                }
            }

            for (row_idx, (items, indent, height)) in (&*current_layout).into_iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = egui::Vec2::new(pad_x, pad_y);

                    ui.add_space(current_layout.scale_x(indent));

                    let row_height = current_layout.scale_y(height);
                    let mut row_centres = vec![None; items.len()];

                    let mut draw_item = |ui: &mut Ui, col_idx: usize, item: &layout::RowItem| {
                        match item {
                            layout::RowItem::Key(key) => {
                                // Skip rendering for SKIP keys - just add space
                                if key.is_skip() {
                                    ui.add_space(current_layout.scale_x(key.width));
                                    return;
                                }

                                let appearance = key.appearance(&display_ctx);
                                let font_size = key.font_size.unwrap_or(current_layout.font_size);
                                let color = appearance
                                    .text_color
                                    .unwrap_or(ui.style().visuals.widgets.inactive.fg_stroke.color);
                                let label = label_cache.get(&appearance.text, font_size, color);
                                let mut button = egui::Button::new(label);

                                if let Some(fill) = appearance.button_color {
                                    button = button.fill(fill);
                                } else if key.is_key(
                                    *shift_state,
                                    &RawKey::Action(KeyboardAction::ToggleShift),
                                ) && *shift_state
                                {
                                    button = button.selected(true);
                                } else {
                                    let cell = (row_idx, col_idx);
                                    let sel0 = selected.left == Some(cell);
                                    let sel1 = selected.right == Some(cell);

                                    if sel0 && sel1 {
                                        // Purple for both
                                        button = button
                                            .fill(egui::Color32::from_rgb(120, 60, 180))
                                            .selected(true);
                                    } else if sel0
                                        || (selected.left.is_none() && left_rest == Some(cell))
                                    {
                                        // Blue for left stick
                                        button = button
                                            .fill(egui::Color32::from_rgb(50, 100, 180))
                                            .selected(true);
                                    } else if sel1
                                        || (selected.right.is_none() && right_rest == Some(cell))
                                    {
                                        // Green for right stick
                                        button = button
                                            .fill(egui::Color32::from_rgb(50, 150, 80))
                                            .selected(true);
                                    }
                                }

                                let size =
                                    egui::Vec2::new(current_layout.scale_x(key.width), row_height);
                                let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
                                let response = ui.place(rect, button.truncate());

                                row_centres[col_idx] = Some(rect.center());

                                // Overlay small indicator for Ctrl/Alt on the Space key in the bottom left
                                if key.display_modifiers && (*ctrl_mod || *alt_mod) {
                                    let mut mods = Vec::new();
                                    if *ctrl_mod {
                                        mods.push("ctrl");
                                    }
                                    if *shift_mod {
                                        mods.push("shift");
                                    }
                                    if *alt_mod {
                                        mods.push("alt");
                                    }
                                    let mod_string = mods.join("+");
                                    let rect = response.rect;
                                    let font_size = current_layout.font_size * 0.6;
                                    ui.painter().text(
                                        rect.left_bottom() + egui::Vec2::new(4.0, -4.0),
                                        egui::Align2::LEFT_BOTTOM,
                                        mod_string,
                                        egui::FontId::proportional(font_size),
                                        egui::Color32::WHITE,
                                    );
                                }

                                if response.clicked() {
                                    pressed_key = Some(key.key(*shift_state));
                                }
                            }
                            layout::RowItem::Battery(batt) => {
                                draw_battery(
                                    ui,
                                    current_layout,
                                    batt,
                                    row_height,
                                    *last_battery,
                                    &batt_cfg,
                                    label_cache,
                                );
                            }
                        }
                    };

                    for (col_idx, item) in items.iter().enumerate() {
                        if item.align() == layout::ItemAlign::Left {
                            draw_item(ui, col_idx, item);
                        }
                    }

                    let right_w = current_layout.cluster_width(
                        items
                            .iter()
                            .filter(|item| item.align() == layout::ItemAlign::Right),
                        pad_x,
                    );
                    if right_w > 0.0 {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        let used = ui.min_rect().width();
                        let gap = KeyboardLayout::rtl_leading_gap(content_width, used, right_w);
                        ui.add_space(gap);
                        ui.allocate_ui_with_layout(
                            egui::Vec2::new(right_w, row_height),
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                ui.spacing_mut().item_spacing = egui::Vec2::new(pad_x, pad_y);
                                for (col_idx, item) in items.iter().enumerate() {
                                    if item.align() == layout::ItemAlign::Right {
                                        draw_item(ui, col_idx, item);
                                    }
                                }
                            },
                        );
                    }

                    captured_data.push(row_centres);
                });
            }
        });

        if current_layout.captured_centres.as_ref() != Some(&captured_data) {
            current_layout.update_geometry(captured_data);
            let neighbors = current_layout.typo_neighbors();
            const MIN_LETTERS_FOR_TYPO_NEIGHBORS: usize = 10;
            let letter_n = neighbors.keys().filter(|c| c.is_alphabetic()).count();
            if letter_n >= MIN_LETTERS_FOR_TYPO_NEIGHBORS {
                crate::completion::with_mut(|s| {
                    if let Some(s) = s {
                        s.set_neighbors(neighbors);
                    }
                });
            }
            if publish_geometry {
                geometry_snap::publish(current_layout_name, current_layout);
            }
        }

        if publish_geometry {
            current_layout.draw_debug(ctx, ui);
        }

        pressed_key
    }

    fn reload_from_config(&mut self) -> Result<()> {
        let cfg = self.cfg();

        // Reload all layouts
        self.layouts = HashMap::new();
        for (name, path) in &cfg.layouts {
            let toml = fs::read_to_string(path)?;
            let layout = KeyboardLayout::load_with_scales(
                &toml,
                cfg.scale_x,
                cfg.scale_y,
                cfg.stick_scale_x,
                cfg.stick_scale_y,
            )?;
            self.layouts.insert(name.clone(), layout);
        }

        self.bindings = load_bindings(StateId::Keyboard)?;

        self.stick_select_lock_ms = Duration::from_millis(cfg.stick_select_lock_ms);
        self.stick_select_sticky = cfg.stick_select_sticky;

        self.on_layouts_changed();

        Ok(())
    }
}

static KEYBOARD: OnceLock<Mutex<KeyboardState>> = OnceLock::new();

pub(crate) fn with_mut<R>(f: impl FnOnce(&mut KeyboardState) -> R) -> R {
    let mut guard = KEYBOARD
        .get()
        .expect("keyboard state not initialized")
        .lock()
        .unwrap();
    f(&mut guard)
}

/// Names of currently loaded keyboard layouts (sorted), for the mappings catalog.
pub(crate) fn layout_names() -> Vec<String> {
    let Some(cell) = KEYBOARD.get() else {
        return Vec::new();
    };
    let guard = cell.lock().unwrap();
    let mut names: Vec<String> = guard.layouts.keys().cloned().collect();
    names.sort();
    names
}

pub(crate) fn current_layout_name() -> String {
    let Some(cell) = KEYBOARD.get() else {
        return String::new();
    };
    cell.lock().unwrap().current_layout.clone()
}

pub fn init() -> Result<()> {
    let kb = KeyboardState::new()?;
    KEYBOARD
        .set(Mutex::new(kb))
        .map_err(|_| anyhow::anyhow!("keyboard state already initialized"))?;

    crate::config::on_changed(|| {
        let result = with_mut(|k| k.reload_from_config());
        if let Err(e) = result {
            eprintln!("Failed to reload keyboard from config: {}", e);
        }
    })?;

    Ok(())
}

#[cfg(test)]
mod send_key_tests {
    use super::*;
    use crate::state::event::{Event, EventQueue, EventSource};

    fn drain_char(kb: &mut KeyboardState, c: char) -> Vec<Event> {
        let mut events = EventQueue::passthrough();
        let src = EventSource::MouseClick;
        kb.send_key(&RawKey::Key(c), &mut events, &src).unwrap();
        events.drain_pending().into_iter().map(|(e, _)| e).collect()
    }

    #[test]
    fn letter_without_mods_uses_send_text() {
        let mut kb = KeyboardState::default();
        assert_eq!(drain_char(&mut kb, 'c'), vec![Event::SendText("c".into())]);
    }

    #[test]
    fn typo_replace_survives_debounce() {
        use crate::completion::settings::AcceptVia;
        use crate::completion::AcceptOutcome;
        use enigo::Direction;

        let mut events = EventQueue::passthrough();
        events.set_debounce_ms(240, 55);
        let src = EventSource::MouseClick;
        let out = AcceptOutcome {
            inject: "people ".into(),
            via: AcceptVia::BackspaceReplace,
            token_char_len: 4,
        };
        assert!(KeyboardState::enqueue_accept(&mut events, &src, &out));
        let got: Vec<Event> = events.drain_pending().into_iter().map(|(e, _)| e).collect();
        let backspace = Event::SendKey(enigo::Key::Backspace, Direction::Click);
        assert_eq!(
            got,
            vec![
                backspace.clone(),
                backspace.clone(),
                backspace.clone(),
                backspace,
                Event::SendText("people ".into()),
            ]
        );
    }

    #[test]
    fn ctrl_letter_sends_control_and_virtual_key() {
        let mut kb = KeyboardState {
            ctrl_mod: true,
            ..Default::default()
        };
        assert_eq!(
            drain_char(&mut kb, 'c'),
            vec![
                Event::SendKey(enigo::Key::Control, enigo::Direction::Press),
                Event::SendKey(enigo::Key::Unicode('c'), enigo::Direction::Click),
                Event::SendKey(enigo::Key::Control, enigo::Direction::Release),
            ]
        );
        assert!(!kb.ctrl_mod);
    }

    #[test]
    fn ctrl_shift_letter_sends_modifiers_and_virtual_key() {
        let mut kb = KeyboardState {
            ctrl_mod: true,
            shift_mod: true,
            ..Default::default()
        };
        assert_eq!(
            drain_char(&mut kb, 'c'),
            vec![
                Event::SendKey(enigo::Key::Shift, enigo::Direction::Press),
                Event::SendKey(enigo::Key::Control, enigo::Direction::Press),
                Event::SendKey(enigo::Key::Unicode('c'), enigo::Direction::Click),
                Event::SendKey(enigo::Key::Control, enigo::Direction::Release),
                Event::SendKey(enigo::Key::Shift, enigo::Direction::Release),
            ]
        );
    }

    #[test]
    fn last_battery_survives_without_new_input() {
        let mut kb = KeyboardState::default();
        kb.note_battery(Some(BatteryStatus {
            percent: 42,
            charging: false,
        }));
        let first = battery_percent_text(kb.last_battery);
        let icon = battery_icon_name(kb.last_battery);
        let second = battery_percent_text(kb.last_battery);
        assert_eq!(first, "42%");
        assert_eq!(first, second);
        assert_eq!(icon, "battery-medium");
        kb.note_battery(None);
        assert_eq!(battery_percent_text(kb.last_battery), "42%");
    }

    #[test]
    fn battery_color_uses_config_levels() {
        let cfg = config::BatteryConfig::default();
        assert_eq!(battery_color(None, &cfg), rgba(cfg.unknown));
        assert_eq!(
            battery_color(
                Some(BatteryStatus {
                    percent: 5,
                    charging: false
                }),
                &cfg
            ),
            rgba(cfg.empty)
        );
        assert_eq!(
            battery_color(
                Some(BatteryStatus {
                    percent: 12,
                    charging: true
                }),
                &cfg
            ),
            rgba(cfg.charging)
        );
        assert_eq!(
            battery_color(
                Some(BatteryStatus {
                    percent: 99,
                    charging: false
                }),
                &cfg
            ),
            rgba(cfg.full)
        );
    }

    #[test]
    fn battery_icon_thresholds_and_charging() {
        assert_eq!(battery_icon_name(None), "battery-empty");
        assert_eq!(battery_percent_text(None), "--");
        assert_eq!(
            battery_icon_name(Some(BatteryStatus {
                percent: 5,
                charging: false
            })),
            "battery-empty"
        );
        assert_eq!(
            battery_icon_name(Some(BatteryStatus {
                percent: 20,
                charging: false
            })),
            "battery-low"
        );
        assert_eq!(
            battery_icon_name(Some(BatteryStatus {
                percent: 80,
                charging: false
            })),
            "battery-high"
        );
        assert_eq!(
            battery_icon_name(Some(BatteryStatus {
                percent: 99,
                charging: false
            })),
            "battery-full"
        );
        assert_eq!(
            battery_icon_name(Some(BatteryStatus {
                percent: 12,
                charging: true
            })),
            "battery-charging"
        );
    }
}
