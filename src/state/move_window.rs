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
use egui::{Align, Color32, Layout, Sense, Stroke, Ui};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use crate::controller::bindings::BindingEngine;
use crate::controller::{ControllerButton, ControllerKind};

/// Stick/pad max-axis past this is treated as engaged (same as SC2 analog idle).
pub(crate) const ANALOG_DEADZONE: f32 = 0.15;

/// Full deflection crosses the monitor in this many seconds.
const MONITOR_CROSS_SECS: f32 = 1.0;

const GLYPH_SIZE: f32 = 28.0;

pub struct MoveWindowState {
    bindings: BindingEngine<MoveWindowAction>,
    last_analog_tick: Option<Instant>,
}

impl MoveWindowState {
    pub fn new() -> Result<Self> {
        Ok(Self {
            bindings: load_bindings(StateId::MoveWindow)?,
            last_analog_tick: None,
        })
    }

    pub fn begin(&mut self) {
        self.last_analog_tick = None;
    }

    pub fn draw_ui(
        &mut self,
        ui: &mut Ui,
        controller_kind: ControllerKind,
        events: &mut EventQueue,
    ) {
        let family = GlyphFamily::from_kind(controller_kind);
        let fill = Color32::from_rgba_unmultiplied(32, 32, 32, 90);
        let border = Color32::from_rgba_unmultiplied(220, 220, 220, 200);
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
        let analog = analog_deflection(
            input.left_stick(),
            input.right_stick(),
            input.left_pad(),
            input.right_pad(),
        );

        let now = Instant::now();
        let dt = self
            .last_analog_tick
            .map(|t| now.saturating_duration_since(t).as_secs_f32().min(0.05))
            .unwrap_or(0.0);
        self.last_analog_tick = Some(now);

        if analog_magnitude(analog) <= ANALOG_DEADZONE {
            return None;
        }
        if dt <= 0.0 {
            return None;
        }

        Some(integrate_pos(coords, analog, dt, window_size, monitor_size))
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
        if ui.button(label).clicked() {
            clicked = true;
        }
    });
    clicked
}

fn analog_magnitude(v: (f32, f32)) -> f32 {
    v.0.abs().max(v.1.abs())
}

fn side_deflection(stick: (f32, f32), pad: Option<(f32, f32)>) -> (f32, f32) {
    match pad {
        Some(p) if analog_magnitude(p) >= analog_magnitude(stick) => p,
        _ => stick,
    }
}

/// Right stick/pad past deadzone wins; otherwise left. Pad vs stick on a side: larger magnitude.
pub(crate) fn analog_deflection(
    left_stick: (f32, f32),
    right_stick: (f32, f32),
    left_pad: Option<(f32, f32)>,
    right_pad: Option<(f32, f32)>,
) -> (f32, f32) {
    let right = side_deflection(right_stick, right_pad);
    if analog_magnitude(right) > ANALOG_DEADZONE {
        return right;
    }
    side_deflection(left_stick, left_pad)
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
    let max_x = (monitor_size.0 - window_size.0).max(0.0);
    let max_y = (monitor_size.1 - window_size.1).max(0.0);
    (x.clamp(0.0, max_x), y.clamp(0.0, max_y))
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
        let v = analog_deflection((0.1, 0.0), (0.0, 0.0), None, None);
        assert!(analog_magnitude(v) <= ANALOG_DEADZONE);
    }

    #[test]
    fn right_stick_ignores_left() {
        let v = analog_deflection((1.0, 0.0), (0.0, 0.8), None, None);
        assert!((v.0 - 0.0).abs() < 1e-6);
        assert!((v.1 - 0.8).abs() < 1e-6);
    }

    #[test]
    fn right_pad_ignores_left_stick() {
        let v = analog_deflection((1.0, 0.0), (0.0, 0.0), None, Some((0.5, 0.0)));
        assert!((v.0 - 0.5).abs() < 1e-6);
        assert!((v.1 - 0.0).abs() < 1e-6);
    }

    #[test]
    fn left_used_when_right_idle() {
        let v = analog_deflection((0.0, -0.9), (0.05, 0.0), None, None);
        assert!((v.0 - 0.0).abs() < 1e-6);
        assert!((v.1 + 0.9).abs() < 1e-6);
    }

    #[test]
    fn pad_beats_stick_on_same_side_when_larger() {
        let v = analog_deflection((0.0, 0.0), (0.2, 0.0), None, Some((0.9, 0.1)));
        assert!((v.0 - 0.9).abs() < 1e-6);
        assert!((v.1 - 0.1).abs() < 1e-6);
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
