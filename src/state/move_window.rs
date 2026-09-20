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
use egui::{Align, Color32, Label, Layout, RichText, Sense, Stroke, Ui};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use crate::controller::bindings::BindingEngine;
use crate::controller::{ControllerButton, ControllerKind};

/// Stick/pad max-axis past this is treated as engaged (same as SC2 analog idle).
pub(crate) const ANALOG_DEADZONE: f32 = 0.15;

/// Full stick deflection crosses the monitor in this many seconds.
const MONITOR_CROSS_SECS: f32 = 1.0;

/// Pad samples span `-1..=1` on each axis. A full swipe moves one monitor.
const PAD_AXIS_SPAN: f32 = 2.0;

const GLYPH_SIZE: f32 = 28.0;
const PROMPT_LABEL_SIZE: f32 = 18.0;

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
}

impl MoveWindowState {
    pub fn new() -> Result<Self> {
        Ok(Self {
            bindings: load_bindings(StateId::MoveWindow)?,
            last_analog_tick: None,
            last_left_pad: None,
            last_right_pad: None,
        })
    }

    pub fn begin(&mut self) {
        self.last_analog_tick = None;
        self.last_left_pad = None;
        self.last_right_pad = None;
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

        ui.scope_builder(egui::UiBuilder::new().max_rect(resp.rect), |ui| {
            ui.with_layout(Layout::top_down(Align::Center), |ui| {
                ui.add_space((resp.rect.height() * 0.35).max(8.0));
                if prompt_row(ui, family, &save_buttons, "Save") {
                    let _ = events.push_seq(
                        vec![Event::SaveWindowPos, Event::ChangeState(StateId::Menu)],
                        &EventSource::MouseClick,
                    );
                }
                ui.add_space(12.0);
                if prompt_row(ui, family, &cancel_buttons, "Cancel") {
                    let _ =
                        events.push(Event::ChangeState(StateId::Menu), &EventSource::MouseClick);
                }
            });
        });
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
        self.last_left_pad = None;
        self.last_right_pad = None;
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
            AnalogSource::None => {
                self.last_left_pad = None;
                self.last_right_pad = None;
                None
            }
            AnalogSource::Stick(analog) => {
                self.last_left_pad = None;
                self.last_right_pad = None;
                if dt <= 0.0 {
                    return None;
                }
                Some(integrate_pos(coords, analog, dt, window_size, monitor_size))
            }
            AnalogSource::LeftPad(sample) => {
                self.last_right_pad = None;
                Self::apply_pad_mouse(
                    &mut self.last_left_pad,
                    sample,
                    coords,
                    window_size,
                    monitor_size,
                )
            }
            AnalogSource::RightPad(sample) => {
                self.last_left_pad = None;
                Self::apply_pad_mouse(
                    &mut self.last_right_pad,
                    sample,
                    coords,
                    window_size,
                    monitor_size,
                )
            }
        }
    }

    fn apply_pad_mouse(
        last: &mut Option<(f32, f32)>,
        sample: (f32, f32),
        coords: (f32, f32),
        window_size: (f32, f32),
        monitor_size: (f32, f32),
    ) -> Option<(f32, f32)> {
        let step = pad_touch_delta(*last, Some(sample));
        *last = step.last;
        let d = step.delta?;
        if d.0 == 0.0 && d.1 == 0.0 {
            return None;
        }
        let (dx, dy) = pad_delta_to_points(d, monitor_size);
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

fn prompt_row(ui: &mut Ui, family: GlyphFamily, buttons: &[ControllerButton], label: &str) -> bool {
    let mut clicked = false;
    ui.horizontal(|ui| {
        for button in buttons {
            controller_glyph::show(ui, family, *button, GLYPH_SIZE);
        }
        let response = ui.add(
            Label::new(
                RichText::new(label)
                    .size(PROMPT_LABEL_SIZE)
                    .color(Color32::WHITE),
            )
            .sense(Sense::click()),
        );
        if response.clicked() {
            clicked = true;
        }
    });
    clicked
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
