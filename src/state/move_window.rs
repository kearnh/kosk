use crate::{
    controller::ControllerInput,
    state::{
        actions::load_bindings,
        event::{Event, EventQueue, EventSource},
        move_window_action::MoveWindowAction,
        window_pos::WindowPos,
        StateId,
    },
    ui::controller_glyph::{self, GlyphFamily},
};
use anyhow::Result;
use egui::{vec2, Align, Color32, FontId, Layout, Sense, Stroke, Ui};
use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::controller::bindings::BindingEngine;
use crate::controller::{ControllerButton, ControllerKind};

/// Stick/pad max-axis past this is treated as engaged (same as SC2 analog idle).
pub(crate) const ANALOG_DEADZONE: f32 = 0.15;

/// Full stick deflection crosses the monitor in this many seconds.
const MONITOR_CROSS_SECS: f32 = 1.0;

/// Pad samples span `-1..=1` on each axis. A full swipe moves one monitor.
const PAD_AXIS_SPAN: f32 = 2.0;

/// EMA time constant for pad samples.
const PAD_SMOOTH_TAU_SECS: f32 = 0.012;

/// Drop pad motion this recent when the thumb lifts.
const PAD_LIFT_SUPPRESS: Duration = Duration::from_millis(40);

const GLYPH_SIZE: f32 = 28.0;
const PROMPT_LABEL_SIZE: f32 = 18.0;
const PROMPT_ROW_GAP: f32 = 12.0;
const PROMPT_ITEM_GAP: f32 = 8.0;
const LABEL_HALO_PX: f32 = 1.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum AnalogSource {
    None,
    Stick((f32, f32)),
    LeftPad((f32, f32)),
    RightPad((f32, f32)),
}

pub struct MoveWindowState {
    bindings: BindingEngine<MoveWindowAction>,
    last_analog_tick: Option<Instant>,
    last_left_pad: Option<(f32, f32)>,
    last_right_pad: Option<(f32, f32)>,
    pad_moves: VecDeque<(Instant, (f32, f32))>,
    left_remainder: (f32, f32),
    right_remainder: (f32, f32),
}

impl MoveWindowState {
    pub fn new() -> Result<Self> {
        Ok(Self {
            bindings: load_bindings(StateId::MoveWindow)?,
            last_analog_tick: None,
            last_left_pad: None,
            last_right_pad: None,
            pad_moves: VecDeque::new(),
            left_remainder: (0.0, 0.0),
            right_remainder: (0.0, 0.0),
        })
    }

    pub fn begin(&mut self) {
        self.last_analog_tick = None;
        self.clear_pad_tracking();
    }

    pub fn draw_ui(
        &mut self,
        ui: &mut Ui,
        controller_kind: ControllerKind,
        events: &mut EventQueue,
    ) {
        let family = GlyphFamily::from_kind(controller_kind);
        let fill = Color32::from_rgba_unmultiplied(16, 16, 16, 48);
        let border = Color32::from_rgba_unmultiplied(255, 255, 255, 210);
        let size = ui.available_size();
        let (resp, painter) = ui.allocate_painter(size, Sense::hover());
        painter.rect_filled(resp.rect, 4.0, fill);
        painter.rect_stroke(
            resp.rect,
            4.0,
            Stroke::new(2.0, border),
            egui::StrokeKind::Inside,
        );

        let save_buttons = self
            .bindings
            .buttons_matching(|a| matches!(a, MoveWindowAction::Save));
        let cancel_buttons = self
            .bindings
            .buttons_matching(|a| matches!(a, MoveWindowAction::SwitchState(StateId::Menu)));

        ui.scope_builder(
            egui::UiBuilder::new()
                .max_rect(resp.rect)
                .layout(Layout::top_down(Align::Center)),
            |ui| {
                let save_gw = glyph_col_width(save_buttons.len());
                let cancel_gw = glyph_col_width(cancel_buttons.len());
                let glyph_col = save_gw.max(cancel_gw);
                let label_col = label_width(ui, "Save").max(label_width(ui, "Cancel"));
                let gap = if glyph_col > 0.0 {
                    PROMPT_ITEM_GAP
                } else {
                    0.0
                };
                let block_w = glyph_col + gap + label_col;
                let left = ((ui.available_width() - block_w) * 0.5).max(0.0);
                let block_h = GLYPH_SIZE * 2.0 + PROMPT_ROW_GAP;
                ui.add_space(((resp.rect.height() - block_h) * 0.5).max(0.0));
                if prompt_row(ui, family, &save_buttons, "Save", left, glyph_col) {
                    let _ = events.push_seq(
                        vec![Event::SaveWindowPos, Event::ChangeState(StateId::Menu)],
                        &EventSource::MouseClick,
                    );
                }
                ui.add_space(PROMPT_ROW_GAP);
                if prompt_row(ui, family, &cancel_buttons, "Cancel", left, glyph_col) {
                    let _ =
                        events.push(Event::ChangeState(StateId::Menu), &EventSource::MouseClick);
                }
            },
        );
    }

    fn do_action(
        &self,
        action: &MoveWindowAction,
        events: &mut EventQueue,
        source: &EventSource,
    ) -> Result<()> {
        match action {
            MoveWindowAction::Save => {
                let _ = events.push_seq(
                    vec![Event::SaveWindowPos, Event::ChangeState(StateId::Menu)],
                    source,
                );
            }
            MoveWindowAction::SwitchState(state) => {
                let _ = events.push(Event::ChangeState(*state), source);
            }
        }
        Ok(())
    }

    pub fn reset_controller_input(&mut self, holdover: Option<&dyn ControllerInput>) {
        self.bindings.reset(holdover);
        self.last_analog_tick = None;
        self.clear_pad_tracking();
    }

    /// Undo recent pad motion when HID goes idle. `Some` if the overlay should move.
    pub fn flush_pad_lift(
        &mut self,
        coords: (f32, f32),
        window_size: (f32, f32),
        monitor_size: (f32, f32),
    ) -> Option<(f32, f32)> {
        self.finish_pad(coords, window_size, monitor_size, Instant::now())
    }

    pub fn handle_controller_input(
        &mut self,
        input: &dyn ControllerInput,
        events: &mut EventQueue,
    ) -> Result<()> {
        for (binding, action) in self
            .bindings
            .evaluate(input, &crate::when::WhenContext::default())
        {
            let src = EventSource::Controller(binding);
            self.do_action(&action, events, &src)?;
        }

        Ok(())
    }

    /// Integrate analog into `coords`. `Some` when the window should move.
    pub fn apply_analog(
        &mut self,
        input: &dyn ControllerInput,
        coords: (f32, f32),
        window_size: (f32, f32),
        monitor_size: (f32, f32),
    ) -> Option<(f32, f32)> {
        let source = pick_analog_source(
            input.left_stick(),
            input.right_stick(),
            input.left_pad_raw(),
            input.right_pad_raw(),
        );

        let now = Instant::now();
        let dt = self
            .last_analog_tick
            .map(|t| now.saturating_duration_since(t).as_secs_f32().min(0.05))
            .unwrap_or(0.0);
        self.last_analog_tick = Some(now);

        match source {
            AnalogSource::None => self.finish_pad(coords, window_size, monitor_size, now),
            AnalogSource::Stick(analog) => {
                let mut pos = coords;
                if let Some(p) = self.finish_pad(pos, window_size, monitor_size, now) {
                    pos = p;
                }
                if dt <= 0.0 {
                    return (pos != coords).then_some(pos);
                }
                Some(integrate_pos(pos, analog, dt, window_size, monitor_size))
            }
            AnalogSource::LeftPad(sample) => {
                let mut pos = coords;
                if self.last_right_pad.is_some() {
                    if let Some(p) = self.finish_pad(pos, window_size, monitor_size, now) {
                        pos = p;
                    }
                }
                Self::apply_pad_mouse(
                    &mut self.last_left_pad,
                    &mut self.pad_moves,
                    &mut self.left_remainder,
                    sample,
                    dt,
                    now,
                    (pos, window_size, monitor_size),
                )
                .or_else(|| (pos != coords).then_some(pos))
            }
            AnalogSource::RightPad(sample) => {
                let mut pos = coords;
                if self.last_left_pad.is_some() {
                    if let Some(p) = self.finish_pad(pos, window_size, monitor_size, now) {
                        pos = p;
                    }
                }
                Self::apply_pad_mouse(
                    &mut self.last_right_pad,
                    &mut self.pad_moves,
                    &mut self.right_remainder,
                    sample,
                    dt,
                    now,
                    (pos, window_size, monitor_size),
                )
                .or_else(|| (pos != coords).then_some(pos))
            }
        }
    }

    fn finish_pad(
        &mut self,
        coords: (f32, f32),
        window_size: (f32, f32),
        monitor_size: (f32, f32),
        now: Instant,
    ) -> Option<(f32, f32)> {
        let was_pad = self.last_left_pad.is_some() || self.last_right_pad.is_some();
        if !was_pad {
            self.clear_pad_tracking();
            return None;
        }
        let next = rewind_pad_lift(
            coords,
            self.pad_moves.iter().copied(),
            now,
            window_size,
            monitor_size,
        );
        self.clear_pad_tracking();
        (next != coords).then_some(next)
    }

    fn clear_pad_tracking(&mut self) {
        self.last_left_pad = None;
        self.last_right_pad = None;
        self.pad_moves.clear();
        self.left_remainder = (0.0, 0.0);
        self.right_remainder = (0.0, 0.0);
    }

    fn apply_pad_mouse(
        last: &mut Option<(f32, f32)>,
        trail: &mut VecDeque<(Instant, (f32, f32))>,
        remainder: &mut (f32, f32),
        sample: (f32, f32),
        dt: f32,
        now: Instant,
        place: ((f32, f32), (f32, f32), (f32, f32)),
    ) -> Option<(f32, f32)> {
        let (coords, window_size, monitor_size) = place;
        let step = filtered_pad_step(*last, sample, dt);
        *last = step.last;
        let d = step.delta?;
        let points = pad_delta_to_points(d, monitor_size);
        let (dx, dy) = take_whole_pixels(remainder, points)?;
        push_pad_move(trail, now, (dx, dy));
        Some(clamp_pos(
            (coords.0 + dx, coords.1 + dy),
            window_size,
            monitor_size,
        ))
    }

    fn reload_from_config(&mut self) -> Result<()> {
        self.bindings = load_bindings(StateId::MoveWindow)?;
        Ok(())
    }
}

fn glyph_col_width(n: usize) -> f32 {
    if n == 0 {
        0.0
    } else {
        n as f32 * GLYPH_SIZE + (n - 1) as f32 * PROMPT_ITEM_GAP
    }
}

fn label_width(ui: &Ui, text: &str) -> f32 {
    let font = FontId::proportional(PROMPT_LABEL_SIZE);
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_string(), font, Color32::PLACEHOLDER);
    galley.mesh_bounds.width() + 2.0 * LABEL_HALO_PX
}

fn prompt_row(
    ui: &mut Ui,
    family: GlyphFamily,
    buttons: &[ControllerButton],
    label: &str,
    left: f32,
    glyph_col: f32,
) -> bool {
    let mut clicked = false;
    ui.allocate_ui_with_layout(
        vec2(ui.available_width(), GLYPH_SIZE),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.spacing_mut().item_spacing.x = PROMPT_ITEM_GAP;
            ui.spacing_mut().item_spacing.y = 0.0;
            let gw = glyph_col_width(buttons.len());
            ui.add_space(left + (glyph_col - gw).max(0.0));
            for button in buttons {
                controller_glyph::show(ui, family, *button, GLYPH_SIZE);
            }
            if outlined_label(ui, label) {
                clicked = true;
            }
        },
    );
    clicked
}

fn outlined_label(ui: &mut Ui, text: &str) -> bool {
    let font = FontId::proportional(PROMPT_LABEL_SIZE);
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_string(), font, Color32::PLACEHOLDER);
    let mesh = galley.mesh_bounds;
    let size = vec2(mesh.width() + 2.0 * LABEL_HALO_PX, GLYPH_SIZE);
    let (resp, painter) = ui.allocate_painter(size, Sense::click());
    let origin = resp.rect.center() - mesh.center().to_vec2();
    for [dx, dy] in [
        [-1.0, 0.0],
        [1.0, 0.0],
        [0.0, -1.0],
        [0.0, 1.0],
        [-1.0, -1.0],
        [-1.0, 1.0],
        [1.0, -1.0],
        [1.0, 1.0],
    ] {
        painter.galley(
            origin + vec2(dx * LABEL_HALO_PX, dy * LABEL_HALO_PX),
            galley.clone(),
            Color32::BLACK,
        );
    }
    painter.galley(origin, galley, Color32::WHITE);
    resp.clicked()
}

fn analog_magnitude(v: (f32, f32)) -> f32 {
    v.0.abs().max(v.1.abs())
}

/// Right analog wins. A touching pad is mouse input, even at the pad center.
pub(crate) fn pick_analog_source(
    left_stick: (f32, f32),
    right_stick: (f32, f32),
    left_pad: Option<(f32, f32)>,
    right_pad: Option<(f32, f32)>,
) -> AnalogSource {
    if let Some(p) = right_pad {
        return AnalogSource::RightPad(p);
    }
    if analog_magnitude(right_stick) > ANALOG_DEADZONE {
        return AnalogSource::Stick(right_stick);
    }
    if let Some(p) = left_pad {
        return AnalogSource::LeftPad(p);
    }
    if analog_magnitude(left_stick) > ANALOG_DEADZONE {
        return AnalogSource::Stick(left_stick);
    }
    AnalogSource::None
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct PadStep {
    delta: Option<(f32, f32)>,
    last: Option<(f32, f32)>,
}

/// First contact and lift produce no delta so the window does not jump.
#[cfg(test)]
fn pad_touch_delta(last: Option<(f32, f32)>, now: Option<(f32, f32)>) -> PadStep {
    match (last, now) {
        (_, None) => PadStep {
            delta: None,
            last: None,
        },
        (None, Some(p)) => PadStep {
            delta: None,
            last: Some(p),
        },
        (Some(prev), Some(p)) => PadStep {
            delta: Some((p.0 - prev.0, p.1 - prev.1)),
            last: Some(p),
        },
    }
}

fn ema_alpha(dt: f32) -> f32 {
    if dt <= 0.0 {
        return 0.0;
    }
    (1.0 - (-dt / PAD_SMOOTH_TAU_SECS).exp()).clamp(0.0, 1.0)
}

fn lerp_pair(a: (f32, f32), b: (f32, f32), t: f32) -> (f32, f32) {
    (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
}

fn filtered_pad_step(last: Option<(f32, f32)>, sample: (f32, f32), dt: f32) -> PadStep {
    let Some(prev) = last else {
        return PadStep {
            delta: None,
            last: Some(sample),
        };
    };
    let smoothed = lerp_pair(prev, sample, ema_alpha(dt));
    let d = (smoothed.0 - prev.0, smoothed.1 - prev.1);
    let delta = (d.0 != 0.0 || d.1 != 0.0).then_some(d);
    PadStep {
        delta,
        last: Some(smoothed),
    }
}

fn rewind_pad_lift(
    coords: (f32, f32),
    trail: impl IntoIterator<Item = (Instant, (f32, f32))>,
    now: Instant,
    window_size: (f32, f32),
    monitor_size: (f32, f32),
) -> (f32, f32) {
    let cutoff = now.checked_sub(PAD_LIFT_SUPPRESS).unwrap_or(now);
    let (dx, dy) = trail
        .into_iter()
        .filter(|(at, _)| *at >= cutoff)
        .fold((0.0, 0.0), |acc, (_, d)| (acc.0 + d.0, acc.1 + d.1));
    clamp_pos((coords.0 - dx, coords.1 - dy), window_size, monitor_size)
}

fn take_whole_pixels(acc: &mut (f32, f32), delta: (f32, f32)) -> Option<(f32, f32)> {
    acc.0 += delta.0;
    acc.1 += delta.1;
    let ix = acc.0.trunc();
    let iy = acc.1.trunc();
    acc.0 -= ix;
    acc.1 -= iy;
    (ix != 0.0 || iy != 0.0).then_some((ix, iy))
}

fn push_pad_move(trail: &mut VecDeque<(Instant, (f32, f32))>, now: Instant, delta: (f32, f32)) {
    trail.push_back((now, delta));
    let cutoff = now.checked_sub(PAD_LIFT_SUPPRESS).unwrap_or(now);
    while trail.front().is_some_and(|(at, _)| *at < cutoff) {
        trail.pop_front();
    }
}

pub(crate) fn pad_delta_to_points(delta: (f32, f32), monitor_size: (f32, f32)) -> (f32, f32) {
    (
        delta.0 * monitor_size.0 / PAD_AXIS_SPAN,
        delta.1 * monitor_size.1 / PAD_AXIS_SPAN,
    )
}

pub(crate) fn clamp_pos(
    coords: (f32, f32),
    window_size: (f32, f32),
    monitor_size: (f32, f32),
) -> (f32, f32) {
    let max_x = (monitor_size.0 - window_size.0).max(0.0);
    let max_y = (monitor_size.1 - window_size.1).max(0.0);
    (coords.0.clamp(0.0, max_x), coords.1.clamp(0.0, max_y))
}

pub(crate) fn integrate_pos(
    coords: (f32, f32),
    analog: (f32, f32),
    dt: f32,
    window_size: (f32, f32),
    monitor_size: (f32, f32),
) -> (f32, f32) {
    let speed_x = monitor_size.0 / MONITOR_CROSS_SECS;
    let speed_y = monitor_size.1 / MONITOR_CROSS_SECS;
    let x = coords.0 + analog.0 * speed_x * dt;
    let y = coords.1 + analog.1 * speed_y * dt;
    clamp_pos((x, y), window_size, monitor_size)
}

pub(crate) fn pos_to_persist(origin: WindowPos, live: WindowPos, moved: bool) -> WindowPos {
    if moved {
        live
    } else {
        origin
    }
}

static MOVE_WINDOW: OnceLock<Mutex<MoveWindowState>> = OnceLock::new();

pub(crate) fn with_mut<R>(f: impl FnOnce(&mut MoveWindowState) -> R) -> R {
    let mut guard = MOVE_WINDOW
        .get()
        .expect("move window state not initialized")
        .lock()
        .unwrap();
    f(&mut guard)
}

pub fn init() -> Result<()> {
    MOVE_WINDOW
        .set(Mutex::new(MoveWindowState::new()?))
        .map_err(|_| anyhow::anyhow!("move window state already initialized"))?;

    crate::config::on_changed(|| {
        let result = with_mut(|mw| mw.reload_from_config());
        if let Err(e) = result {
            eprintln!("Failed to reload move window from config: {}", e);
        }
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn glyph_col_width_counts_gaps() {
        assert_eq!(glyph_col_width(0), 0.0);
        assert_eq!(glyph_col_width(1), GLYPH_SIZE);
        assert_eq!(glyph_col_width(2), GLYPH_SIZE * 2.0 + PROMPT_ITEM_GAP);
    }

    #[test]
    fn deadzone_ignores_noise() {
        assert_eq!(
            pick_analog_source((0.1, 0.0), (0.0, 0.0), None, None),
            AnalogSource::None
        );
    }

    #[test]
    fn right_stick_ignores_left() {
        assert_eq!(
            pick_analog_source((1.0, 0.0), (0.0, 0.8), None, None),
            AnalogSource::Stick((0.0, 0.8))
        );
    }

    #[test]
    fn right_pad_ignores_left_stick() {
        assert_eq!(
            pick_analog_source((1.0, 0.0), (0.0, 0.0), None, Some((0.5, 0.0))),
            AnalogSource::RightPad((0.5, 0.0))
        );
    }

    #[test]
    fn right_pad_touch_at_center_ignores_left() {
        assert_eq!(
            pick_analog_source((1.0, 0.0), (0.0, 0.0), None, Some((0.0, 0.0))),
            AnalogSource::RightPad((0.0, 0.0))
        );
    }

    #[test]
    fn left_used_when_right_idle() {
        assert_eq!(
            pick_analog_source((0.0, -0.9), (0.05, 0.0), None, None),
            AnalogSource::Stick((0.0, -0.9))
        );
    }

    #[test]
    fn pad_touch_beats_same_side_stick() {
        assert_eq!(
            pick_analog_source((0.0, 0.0), (0.9, 0.0), None, Some((0.1, 0.0))),
            AnalogSource::RightPad((0.1, 0.0))
        );
    }

    #[test]
    fn right_stick_ignores_left_pad() {
        assert_eq!(
            pick_analog_source((0.0, 0.0), (0.8, 0.0), Some((0.5, 0.0)), None),
            AnalogSource::Stick((0.8, 0.0))
        );
    }

    #[test]
    fn pad_first_contact_has_no_delta() {
        let step = pad_touch_delta(None, Some((0.4, -0.2)));
        assert!(step.delta.is_none());
        assert_eq!(step.last, Some((0.4, -0.2)));
    }

    #[test]
    fn pad_slide_is_relative() {
        let step = pad_touch_delta(Some((0.0, 0.0)), Some((0.5, -0.25)));
        let d = step.delta.expect("slide");
        assert!((d.0 - 0.5).abs() < 1e-6);
        assert!((d.1 + 0.25).abs() < 1e-6);
        assert_eq!(step.last, Some((0.5, -0.25)));
    }

    #[test]
    fn pad_lift_clears_last() {
        let step = pad_touch_delta(Some((0.3, 0.1)), None);
        assert!(step.delta.is_none());
        assert!(step.last.is_none());
    }

    #[test]
    fn ema_damps_pad_noise() {
        let dt = 0.004;
        let samples = [(0.0, 0.0), (0.02, 0.0), (0.0, 0.0), (0.02, 0.0), (0.0, 0.0)];
        let mut raw_last = None;
        let mut raw_travel = 0.0;
        let mut filtered_last = None;
        let mut filtered_travel = 0.0;
        for sample in samples {
            let raw = pad_touch_delta(raw_last, Some(sample));
            raw_last = raw.last;
            if let Some(d) = raw.delta {
                raw_travel += analog_magnitude(d);
            }
            let step = filtered_pad_step(filtered_last, sample, dt);
            filtered_last = step.last;
            if let Some(d) = step.delta {
                filtered_travel += analog_magnitude(d);
            }
        }
        assert!(
            filtered_travel < raw_travel * 0.7,
            "filtered {filtered_travel} raw {raw_travel}"
        );
    }

    #[test]
    fn sub_pixel_step_does_not_move() {
        let mut acc = (0.0, 0.0);
        assert!(take_whole_pixels(&mut acc, (0.4, -0.2)).is_none());
        assert!((acc.0 - 0.4).abs() < 1e-6);
        assert!((acc.1 + 0.2).abs() < 1e-6);
    }

    #[test]
    fn tiny_steps_eventually_emit_pixel() {
        let mut acc = (0.0, 0.0);
        let mut moved = 0.0;
        for _ in 0..10 {
            if let Some((dx, _)) = take_whole_pixels(&mut acc, (0.3, 0.0)) {
                moved += dx;
            }
        }
        assert!((moved - 3.0).abs() < 1e-6, "{moved}");
    }

    #[test]
    fn filtered_first_contact_has_no_delta() {
        let step = filtered_pad_step(None, (0.4, -0.2), 0.004);
        assert!(step.delta.is_none());
        assert_eq!(step.last, Some((0.4, -0.2)));
    }

    #[test]
    fn lift_undoes_only_trailing_window() {
        let now = Instant::now();
        let trail = vec![
            (now - Duration::from_millis(200), (50.0, 0.0)),
            (now - Duration::from_millis(10), (20.0, 0.0)),
        ];
        let next = rewind_pad_lift((100.0, 40.0), trail, now, (200.0, 100.0), (1000.0, 500.0));
        assert!((next.0 - 80.0).abs() < 1e-3, "{}", next.0);
        assert!((next.1 - 40.0).abs() < 1e-3);
    }

    #[test]
    fn pad_full_swipe_equals_monitor() {
        let (dx, dy) = pad_delta_to_points((2.0, -2.0), (1000.0, 500.0));
        assert!((dx - 1000.0).abs() < 1e-3);
        assert!((dy + 500.0).abs() < 1e-3);
    }

    #[test]
    fn integrate_up_decreases_y() {
        let next = integrate_pos(
            (100.0, 200.0),
            (0.0, -1.0),
            0.2,
            (200.0, 100.0),
            (1000.0, 500.0),
        );
        assert!((next.1 - 100.0).abs() < 1e-3, "{}", next.1);
        assert!((next.0 - 100.0).abs() < 1e-3);
    }

    #[test]
    fn integrate_clamps_to_monitor() {
        let next = integrate_pos(
            (0.0, 0.0),
            (-1.0, -1.0),
            1.0,
            (100.0, 100.0),
            (800.0, 600.0),
        );
        assert_eq!(next, (0.0, 0.0));
        let next = integrate_pos((0.0, 0.0), (1.0, 1.0), 10.0, (100.0, 100.0), (800.0, 600.0));
        assert_eq!(next, (700.0, 500.0));
    }

    #[test]
    fn unmoved_save_keeps_origin_variant() {
        assert_eq!(
            pos_to_persist(WindowPos::TopLeft, WindowPos::Absolute(1.0, 2.0), false),
            WindowPos::TopLeft
        );
        assert_eq!(
            pos_to_persist(WindowPos::MousePointer, WindowPos::Absolute(1.0, 2.0), true),
            WindowPos::Absolute(1.0, 2.0)
        );
        assert_eq!(
            pos_to_persist(WindowPos::BottomRight, WindowPos::BottomRight, false),
            WindowPos::BottomRight
        );
    }
}
