//! OSK pad-origin mapping, applied above raw SC2 HID.
//!
//! First-touch `T` is stored after a short settle. Each frame the stick origin
//! is `T * relative` (`0` = pad center, `1` = the touch). Output is `pos` minus
//! that origin, with leftover travel on the short edge stretched toward ±1.
//! Circle-to-square `stick_warp` runs after this, via [`ControllerInput::left_pad`].
//! Lift is `None`; analog sticks are not stretched.

use std::time::{Duration, Instant};

use crate::config;
use crate::controller::sc2::Sc2State;
use crate::controller::{warp, ControllerInput};

/// Map pad `pos` into stick space relative to `origin`.
///
/// `k=0` is 1:1 `pos - origin`. Extra gain applies only on the short remaining
/// side and is capped by `max_gain`.
pub fn stretch_axis(pos: f32, origin: f32, k: f32, max_gain: f32) -> f32 {
    let k = k.clamp(0.0, 1.0);
    let pos = pos.clamp(-1.0, 1.0);
    let origin = origin.clamp(-1.0, 1.0);
    let delta = pos - origin;
    if delta.abs() < f32::EPSILON {
        return 0.0;
    }
    let room = if delta > 0.0 {
        (1.0 - origin).max(1e-3)
    } else {
        (origin + 1.0).max(1e-3)
    };
    let short = (1.0 - room.min(1.0)).clamp(0.0, 1.0);
    let extra = 1.0 + (max_gain.max(1.0) - 1.0) * short;
    let gain = 1.0 + (extra - 1.0) * k;
    (delta * gain).clamp(-1.0, 1.0)
}

fn scale_pair(pos: (f32, f32), s: f32) -> (f32, f32) {
    (pos.0 * s, pos.1 * s)
}

pub(crate) fn stretch_stick(
    pos: (f32, f32),
    origin: (f32, f32),
    k: f32,
    max_gain: f32,
) -> (f32, f32) {
    (
        stretch_axis(pos.0, origin.0, k, max_gain),
        stretch_axis(pos.1, origin.1, k, max_gain),
    )
}

/// Per-pad first-touch sample. Lift clears; a short settle skips the contact spike.
#[derive(Debug, Default)]
pub struct PadOrigin {
    touching: bool,
    touch: Option<(f32, f32)>,
    touch_at: Option<Instant>,
}

impl PadOrigin {
    pub fn update(
        &mut self,
        pos: Option<(f32, f32)>,
        now: Instant,
        settle: Duration,
        relative: f32,
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
            self.touch = None;
        }
        let relative = relative.clamp(0.0, 1.0);
        let Some(touch) = self.touch else {
            if now.saturating_duration_since(self.touch_at.unwrap_or(now)) >= settle {
                self.touch = Some(pos);
                let origin = scale_pair(pos, relative);
                return Some(stretch_stick(pos, origin, k, max_gain));
            }
            return Some(scale_pair(pos, 1.0 - relative));
        };
        let origin = scale_pair(touch, relative);
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
        let relative = cfg.pad_origin_relative;
        let k = cfg.pad_origin_stretch;
        let max_gain = cfg.pad_origin_stretch_max_gain;
        let left = self.left.update(
            state.pad_as_stick_left(),
            now,
            settle,
            relative,
            k,
            max_gain,
        );
        let right = self.right.update(
            state.pad_as_stick_right(),
            now,
            settle,
            relative,
            k,
            max_gain,
        );
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
    fn stretch_k_zero_is_one_to_one_delta() {
        let origin = -0.5;
        for pos in [-1.0, -0.4, 0.0, 0.7, 1.0] {
            let out = stretch_axis(pos, origin, 0.0, 1.5);
            assert!((out - (pos - origin).clamp(-1.0, 1.0)).abs() < 1e-6);
        }
    }

    #[test]
    fn stretch_centered_origin_is_identity() {
        let pos = -0.4;
        assert!((stretch_axis(pos, 0.0, 1.0, 1.5) - pos).abs() < 1e-6);
    }

    #[test]
    fn stretch_contact_at_origin_is_zero() {
        assert_eq!(stretch_axis(-0.5, -0.5, 1.0, 1.5), 0.0);
    }

    #[test]
    fn stretch_short_edge_amplifies_delta() {
        let origin = -0.5;
        let pos = -0.75;
        let delta = pos - origin;
        let out = stretch_axis(pos, origin, 0.3, 1.5);
        assert!(out.abs() > delta.abs(), "expected |{out}| > |{delta}|");
        assert!(out < 0.0);
        assert!(out > -1.0);
    }

    #[test]
    fn stretch_long_edge_stays_one_to_one_delta() {
        let origin = -0.5;
        let pos = 0.4;
        let out = stretch_axis(pos, origin, 0.3, 1.5);
        assert!((out - (pos - origin)).abs() < 1e-6);
    }

    #[test]
    fn stretch_gain_cap_keeps_output_in_range() {
        let origin = -0.9;
        let pos = -0.95;
        let out = stretch_axis(pos, origin, 1.0, 1.5);
        assert!((-1.0..=1.0).contains(&out));
        let delta = pos - origin;
        assert!(out.abs() > delta.abs());
        assert!(out.abs() <= delta.abs() * 1.5 + 1e-5);
    }

    #[test]
    fn relative_zero_is_absolute_passthrough() {
        let mut pad = PadOrigin::default();
        let t0 = Instant::now();
        let settle = Duration::from_millis(20);
        let pos = (-0.5, 0.2);
        assert_eq!(pad.update(Some(pos), t0, settle, 0.0, 0.3, 1.5), Some(pos));
        assert_eq!(
            pad.update(
                Some((-0.7, 0.1)),
                t0 + Duration::from_millis(25),
                settle,
                0.0,
                0.3,
                1.5
            ),
            Some((-0.7, 0.1))
        );
    }

    #[test]
    fn relative_one_contact_is_zero() {
        let mut pad = PadOrigin::default();
        let t0 = Instant::now();
        let settle = Duration::from_millis(20);
        let touch = (0.8, 0.0);
        let first = pad.update(Some(touch), t0, settle, 1.0, 0.0, 1.5);
        assert_eq!(first, Some((0.0, 0.0)));
        let capture = pad.update(
            Some(touch),
            t0 + Duration::from_millis(20),
            settle,
            1.0,
            0.0,
            1.5,
        );
        assert_eq!(capture, Some((0.0, 0.0)));
    }

    #[test]
    fn relative_half_contact_is_half_touch() {
        let mut pad = PadOrigin::default();
        let t0 = Instant::now();
        let settle = Duration::from_millis(20);
        let touch = (0.8, 0.0);
        let during = pad.update(Some(touch), t0, settle, 0.5, 0.0, 1.5);
        assert_eq!(during, Some((0.4, 0.0)));
        let capture = pad
            .update(
                Some(touch),
                t0 + Duration::from_millis(20),
                settle,
                0.5,
                0.0,
                1.5,
            )
            .expect("capture");
        assert!((capture.0 - 0.4).abs() < 1e-6);
        assert!((capture.1).abs() < 1e-6);
    }

    #[test]
    fn settle_emits_pos_times_one_minus_relative() {
        let mut pad = PadOrigin::default();
        let t0 = Instant::now();
        let settle = Duration::from_millis(20);
        let pos = (-0.6, 0.2);
        let relative = 0.25;
        let out = pad
            .update(Some(pos), t0, settle, relative, 1.0, 1.5)
            .unwrap();
        assert!((out.0 - pos.0 * (1.0 - relative)).abs() < 1e-6);
        assert!((out.1 - pos.1 * (1.0 - relative)).abs() < 1e-6);
    }

    #[test]
    fn live_relative_change_mid_hold_retargets() {
        let mut pad = PadOrigin::default();
        let t0 = Instant::now();
        let settle = Duration::from_millis(20);
        let touch = (0.8, 0.0);
        pad.update(Some(touch), t0, settle, 0.0, 0.0, 1.5);
        pad.update(
            Some(touch),
            t0 + Duration::from_millis(20),
            settle,
            0.0,
            0.0,
            1.5,
        );
        let moved = (0.9, 0.1);
        let abs = pad
            .update(
                Some(moved),
                t0 + Duration::from_millis(25),
                settle,
                0.0,
                0.0,
                1.5,
            )
            .unwrap();
        assert!((abs.0 - moved.0).abs() < 1e-6);
        let rel = pad
            .update(
                Some(moved),
                t0 + Duration::from_millis(30),
                settle,
                1.0,
                0.0,
                1.5,
            )
            .unwrap();
        assert!((rel.0 - (moved.0 - touch.0)).abs() < 1e-6);
        assert!((rel.1 - (moved.1 - touch.1)).abs() < 1e-6);
    }

    #[test]
    fn origin_tracker_stretch_after_settle_and_reset_on_lift() {
        let mut pad = PadOrigin::default();
        let t0 = Instant::now();
        let settle = Duration::from_millis(20);
        let relative = 1.0;
        let k = 0.3;
        let max_gain = 1.5;
        let touch = (-0.4, 0.1);

        pad.update(Some((-0.5, 0.2)), t0, settle, relative, k, max_gain);
        pad.update(
            Some(touch),
            t0 + Duration::from_millis(20),
            settle,
            relative,
            k,
            max_gain,
        );

        let moved = pad
            .update(
                Some((-0.7, 0.1)),
                t0 + Duration::from_millis(25),
                settle,
                relative,
                k,
                max_gain,
            )
            .expect("still touching");
        let expected = stretch_axis(-0.7, touch.0 * relative, k, max_gain);
        assert!((moved.0 - expected).abs() < 1e-6);

        assert_eq!(
            pad.update(
                None,
                t0 + Duration::from_millis(30),
                settle,
                relative,
                k,
                max_gain
            ),
            None
        );

        let again = pad.update(
            Some((0.2, -0.3)),
            t0 + Duration::from_millis(40),
            settle,
            relative,
            k,
            max_gain,
        );
        assert_eq!(again, Some((0.0, 0.0)), "lift+retouch starts a new origin");
    }
}
