//! OSK pad-origin stretch, applied above raw SC2 HID.
//!
//! First-touch origin is tracked across frames, then leftover pad travel on the
//! short edge is stretched so the keyboard can still reach ±1. Circle-to-square
//! `stick_warp` runs after this, via [`ControllerInput::left_pad`]. Lift is
//! `None`; analog sticks are not stretched.

use std::time::{Duration, Instant};

use crate::config;
use crate::controller::sc2::Sc2State;
use crate::controller::{warp, ControllerInput};

/// Stretch leftover pad travel toward the unused stick edge so a short-side
/// landing can still reach ±1. `k=0` is identity. Gain extra applies only on
/// the short remaining side and is capped by `max_gain`.
pub fn stretch_axis(pos: f32, origin: f32, k: f32, max_gain: f32) -> f32 {
    let k = k.clamp(0.0, 1.0);
    let pos = pos.clamp(-1.0, 1.0);
    if k == 0.0 {
        return pos;
    }
    let origin = origin.clamp(-1.0, 1.0);
    let delta = pos - origin;
    if delta.abs() < f32::EPSILON {
        return pos;
    }
    let room = if delta > 0.0 {
        (1.0 - origin).max(1e-3)
    } else {
        (origin + 1.0).max(1e-3)
    };
    let short = (1.0 - room.min(1.0)).clamp(0.0, 1.0);
    let gain = 1.0 + (max_gain.max(1.0) - 1.0) * short;
    let stretched = (origin + delta * gain).clamp(-1.0, 1.0);
    pos * (1.0 - k) + stretched * k
}

fn stretch_stick(pos: (f32, f32), origin: (f32, f32), k: f32, max_gain: f32) -> (f32, f32) {
    (
        stretch_axis(pos.0, origin.0, k, max_gain),
        stretch_axis(pos.1, origin.1, k, max_gain),
    )
}

/// Per-pad first-touch origin. Lift clears; a short settle skips the contact spike.
#[derive(Debug, Default)]
pub struct PadOrigin {
    touching: bool,
    origin: Option<(f32, f32)>,
    touch_at: Option<Instant>,
}

impl PadOrigin {
    pub fn update(
        &mut self,
        pos: Option<(f32, f32)>,
        now: Instant,
        settle: Duration,
        k: f32,
        max_gain: f32,
    ) -> Option<(f32, f32)> {
        let Some(pos) = pos else {
            *self = Self::default();
            return None;
        };
        if !self.touching {
            self.touching = true;
            self.touch_at = Some(now);
            self.origin = None;
        }
        let Some(origin) = self.origin else {
            if now.saturating_duration_since(self.touch_at.unwrap_or(now)) >= settle {
                self.origin = Some(pos);
            }
            // Absolute until origin is captured, and on the capture frame itself.
            return Some(pos);
        };
        Some(stretch_stick(pos, origin, k, max_gain))
    }
}

/// Frame-to-frame origin state for both SC2 pads.
#[derive(Debug, Default)]
pub struct PadOriginMapper {
    left: PadOrigin,
    right: PadOrigin,
}

impl PadOriginMapper {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn map(&mut self, state: &Sc2State) -> MappedSc2Input {
        let cfg = config::sc2();
        let now = Instant::now();
        let settle = Duration::from_millis(cfg.pad_origin_settle_ms);
        let k = cfg.pad_origin_stretch;
        let max_gain = cfg.pad_origin_stretch_max_gain;
        let left = self
            .left
            .update(state.pad_as_stick_left(), now, settle, k, max_gain);
        let right = self
            .right
            .update(state.pad_as_stick_right(), now, settle, k, max_gain);
        MappedSc2Input {
            inner: state.clone(),
            left,
            right,
        }
    }
}

/// Raw SC2 snapshot with OSK pad stretch applied in [`left_pad`](ControllerInput::left_pad).
#[derive(Debug, Clone)]
pub struct MappedSc2Input {
    inner: Sc2State,
    left: Option<(f32, f32)>,
    right: Option<(f32, f32)>,
}

impl MappedSc2Input {
    pub fn stretched_left(&self) -> Option<(f32, f32)> {
        self.left
    }

    pub fn stretched_right(&self) -> Option<(f32, f32)> {
        self.right
    }
}

impl ControllerInput for MappedSc2Input {
    fn left_stick_raw(&self) -> (f32, f32) {
        self.inner.left_stick_raw()
    }
    fn right_stick_raw(&self) -> (f32, f32) {
        self.inner.right_stick_raw()
    }
    fn left_pad_raw(&self) -> Option<(f32, f32)> {
        self.inner.left_pad_raw()
    }
    fn right_pad_raw(&self) -> Option<(f32, f32)> {
        self.inner.right_pad_raw()
    }
    fn left_pad(&self) -> Option<(f32, f32)> {
        self.left.map(|p| warp(p, config::get().stick_warp))
    }
    fn right_pad(&self) -> Option<(f32, f32)> {
        self.right.map(|p| warp(p, config::get().stick_warp))
    }
    fn trigger_left(&self) -> Option<u8> {
        self.inner.trigger_left()
    }
    fn trigger_right(&self) -> Option<u8> {
        self.inner.trigger_right()
    }
    fn query(&self, button: crate::controller::ControllerButton) -> bool {
        self.inner.query(button)
    }
    fn is_engaged(&self) -> bool {
        self.inner.is_engaged()
    }
    fn box_clone(&self) -> Box<dyn ControllerInput + Send + Sync> {
        Box::new(self.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stretch_k_zero_is_identity() {
        for pos in [-1.0, -0.4, 0.0, 0.7, 1.0] {
            assert_eq!(stretch_axis(pos, -0.5, 0.0, 1.5), pos);
        }
    }

    #[test]
    fn stretch_centered_origin_is_identity() {
        let pos = -0.4;
        assert!((stretch_axis(pos, 0.0, 1.0, 1.5) - pos).abs() < 1e-6);
    }

    #[test]
    fn stretch_short_edge_moves_closer_to_limit() {
        let origin = -0.5;
        let pos = -0.75;
        let out = stretch_axis(pos, origin, 0.3, 1.5);
        assert!(out < pos, "expected {out} < {pos}");
        assert!(out > -1.0);
    }

    #[test]
    fn stretch_long_edge_stays_near_absolute() {
        let origin = -0.5;
        let pos = 0.4;
        let out = stretch_axis(pos, origin, 0.3, 1.5);
        assert!((out - pos).abs() < 1e-6);
    }

    #[test]
    fn stretch_gain_cap_keeps_output_in_range() {
        let origin = -0.9;
        let pos = -0.95;
        let out = stretch_axis(pos, origin, 1.0, 1.5);
        assert!((-1.0..=1.0).contains(&out));
        assert!(out < pos);
        let delta = pos - origin;
        let max_step = delta.abs() * 1.5;
        assert!((origin - out).abs() <= max_step + 1e-5);
    }

    #[test]
    fn origin_tracker_settle_then_stretch_and_reset_on_lift() {
        let mut pad = PadOrigin::default();
        let t0 = Instant::now();
        let settle = Duration::from_millis(20);
        let k = 0.3;
        let max_gain = 1.5;

        let first = pad.update(Some((-0.5, 0.2)), t0, settle, k, max_gain);
        assert_eq!(first, Some((-0.5, 0.2)));

        let during = pad.update(
            Some((-0.6, 0.2)),
            t0 + Duration::from_millis(10),
            settle,
            k,
            max_gain,
        );
        assert_eq!(during, Some((-0.6, 0.2)), "still absolute before settle");

        let capture = pad.update(
            Some((-0.4, 0.1)),
            t0 + Duration::from_millis(20),
            settle,
            k,
            max_gain,
        );
        assert_eq!(
            capture,
            Some((-0.4, 0.1)),
            "capture frame stays absolute"
        );

        let moved = pad
            .update(
                Some((-0.7, 0.1)),
                t0 + Duration::from_millis(25),
                settle,
                k,
                max_gain,
            )
            .expect("still touching");
        let expected = stretch_axis(-0.7, -0.4, k, max_gain);
        assert!((moved.0 - expected).abs() < 1e-6);
        assert!(moved.0 < -0.7);

        assert_eq!(
            pad.update(None, t0 + Duration::from_millis(30), settle, k, max_gain),
            None
        );

        let again = pad.update(
            Some((0.2, -0.3)),
            t0 + Duration::from_millis(40),
            settle,
            k,
            max_gain,
        );
        assert_eq!(
            again,
            Some((0.2, -0.3)),
            "lift+retouch starts a new origin"
        );
    }
}
