use std::collections::{HashMap, HashSet};

use crate::controller::{ControllerBinding, ControllerButton, ControllerInput};
use crate::state::actions::{Action, TriggerMode};
use crate::when::{WhenContext, WhenExpr};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DpadDir {
    Up,
    Down,
    Left,
    Right,
}

impl DpadDir {
    pub fn to_button(self) -> ControllerButton {
        match self {
            DpadDir::Up => ControllerButton::DpadUp,
            DpadDir::Down => ControllerButton::DpadDown,
            DpadDir::Left => ControllerButton::DpadLeft,
            DpadDir::Right => ControllerButton::DpadRight,
        }
    }
}

#[derive(Default)]
pub struct StickDpad {
    held: Option<DpadDir>,
}

impl StickDpad {
    pub fn update(&mut self, analog: (f32, f32)) -> Option<DpadDir> {
        let (x, y) = analog;
        let mag = x.abs().max(y.abs());
        if self.held.is_some() {
            if mag < 0.35 {
                self.held = None;
                return None;
            }
            return self.held;
        }
        if mag >= 0.50 {
            let dir = if x.abs() >= y.abs() {
                if x < 0.0 {
                    DpadDir::Left
                } else {
                    DpadDir::Right
                }
            } else if y < 0.0 {
                DpadDir::Up
            } else {
                DpadDir::Down
            };
            self.held = Some(dir);
            return Some(dir);
        }
        None
    }

    pub fn reset(&mut self) {
        self.held = None;
    }

    pub fn held(&self) -> Option<DpadDir> {
        self.held
    }
}

struct ChordEntry<A> {
    leader: ControllerButton,
    follower: ControllerButton,
    action: BindingTarget<A>,
}

/// One compiled mapping: a single action, or `when` arms plus an optional fallback.
#[derive(Clone, Debug)]
pub enum BindingTarget<A> {
    Always(A),
    Conditional {
        arms: Vec<(WhenExpr, A)>,
        otherwise: Option<A>,
    },
}

impl<A> From<A> for BindingTarget<A> {
    fn from(action: A) -> Self {
        Self::Always(action)
    }
}

impl<A: Clone> BindingTarget<A> {
    pub fn resolve(&self, ctx: &WhenContext) -> Option<&A> {
        match self {
            Self::Always(action) => Some(action),
            Self::Conditional { arms, otherwise } => {
                for (expr, action) in arms {
                    if expr.eval(ctx) {
                        return Some(action);
                    }
                }
                otherwise.as_ref()
            }
        }
    }
}

/// Resolves single-button and two-button chord mappings with leader-first chord semantics.
pub struct BindingEngine<A> {
    singles: HashMap<ControllerButton, BindingTarget<A>>,
    chords: Vec<ChordEntry<A>>,
    buttons: Vec<ControllerButton>,
    prev_held: HashSet<ControllerButton>,
    /// Followers that completed a chord this gesture; their single mapping is suppressed until release.
    suppress_single: HashSet<ControllerButton>,
    /// Chords already fired while both members are still held.
    chords_fired: HashSet<(ControllerButton, ControllerButton)>,
    /// Leaders currently held, for chord completion.
    leaders_active: HashSet<ControllerButton>,
    /// Resolved `Conditional` action for the current hold (`None` = clause missed, stay idle).
    latched_singles: HashMap<ControllerButton, Option<A>>,
    stick_dpad: Option<StickDpad>,
}

impl<A> Default for BindingEngine<A> {
    fn default() -> Self {
        Self {
            singles: HashMap::new(),
            chords: Vec::new(),
            buttons: Vec::new(),
            prev_held: HashSet::new(),
            suppress_single: HashSet::new(),
            chords_fired: HashSet::new(),
            leaders_active: HashSet::new(),
            latched_singles: HashMap::new(),
            stick_dpad: None,
        }
    }
}

impl<A: Action + Clone> BindingEngine<A> {
    pub fn try_from_raw(raw: HashMap<ControllerBinding, BindingTarget<A>>) -> Result<Self, String> {
        let mut singles = HashMap::new();
        let mut chords = Vec::new();
        let mut chord_leaders = HashSet::new();

        for (binding, action) in raw {
            match binding {
                ControllerBinding::Single(button) => {
                    singles.insert(button, action);
                }
                ControllerBinding::Chord { leader, follower } => {
                    chord_leaders.insert(leader);
                    chords.push(ChordEntry {
                        leader,
                        follower,
                        action,
                    });
                }
            }
        }

        for leader in &chord_leaders {
            if singles.contains_key(leader) {
                return Err(format!(
                    "\"{}\" is a chord leader but also has a standalone mapping",
                    leader
                ));
            }
        }

        let mut buttons: HashSet<ControllerButton> = singles.keys().cloned().collect();
        for c in &chords {
            buttons.insert(c.leader);
            buttons.insert(c.follower);
        }

        Ok(Self {
            singles,
            chords,
            buttons: buttons.into_iter().collect(),
            prev_held: HashSet::new(),
            suppress_single: HashSet::new(),
            chords_fired: HashSet::new(),
            leaders_active: HashSet::new(),
            latched_singles: HashMap::new(),
            stick_dpad: None,
        })
    }

    pub fn try_from_always(raw: HashMap<ControllerBinding, A>) -> Result<Self, String> {
        Self::try_from_raw(
            raw.into_iter()
                .map(|(k, v)| (k, BindingTarget::Always(v)))
                .collect(),
        )
    }

    pub fn with_left_stick_dpad(mut self) -> Self {
        self.stick_dpad = Some(StickDpad::default());
        self
    }

    /// Clear chord gesture bookkeeping and set the edge baseline.
    ///
    /// `holdover == None`: clear `prev_held` (device idle / UI-driven mode switch
    /// with no controller snapshot).
    /// `holdover == Some(input)`: adopt currently held mapped buttons as already
    /// down so Edge actions do not fire for a press that began before this reset
    /// (controller-driven mode switch holdover / carry-over).
    pub fn reset(&mut self, holdover: Option<&dyn ControllerInput>) {
        self.suppress_single.clear();
        self.chords_fired.clear();
        self.leaders_active.clear();
        self.latched_singles.clear();
        if let Some(sd) = &mut self.stick_dpad {
            sd.reset();
            if let Some(input) = holdover {
                sd.update(input.left_stick_raw());
            }
        }
        match holdover {
            None => self.prev_held.clear(),
            Some(input) => self.prev_held = self.compute_held(input),
        }
    }

    pub fn evaluate(
        &mut self,
        input: &dyn ControllerInput,
        ctx: &WhenContext,
    ) -> Vec<(ControllerBinding, A)> {
        if let Some(sd) = &mut self.stick_dpad {
            sd.update(input.left_stick_raw());
        }
        let held = self.compute_held(input);
        let newly_down: HashSet<_> = held.difference(&self.prev_held).cloned().collect();
        let newly_up: HashSet<_> = self.prev_held.difference(&held).cloned().collect();

        for b in &newly_up {
            self.suppress_single.remove(b);
            self.latched_singles.remove(b);
        }

        for chord in &self.chords {
            if input.query(chord.leader) {
                self.leaders_active.insert(chord.leader);
            } else {
                self.leaders_active.remove(&chord.leader);
            }
        }

        self.chords_fired
            .retain(|(leader, follower)| input.query(*leader) && input.query(*follower));

        let mut fired = Vec::new();

        for chord in &self.chords {
            let key = (chord.leader, chord.follower);
            if self.chords_fired.contains(&key) {
                continue;
            }
            if newly_down.contains(&chord.follower) && self.leaders_active.contains(&chord.leader) {
                let Some(action) = chord.action.resolve(ctx) else {
                    continue;
                };
                fired.push((
                    ControllerBinding::Chord {
                        leader: chord.leader,
                        follower: chord.follower,
                    },
                    action.clone(),
                ));
                self.chords_fired.insert(key);
                self.suppress_single.insert(chord.follower);
            }
        }

        for (button, target) in &self.singles {
            if self.suppress_single.contains(button) {
                continue;
            }
            let Some(action) = Self::resolve_single(
                *button,
                target,
                ctx,
                newly_down.contains(button),
                &mut self.latched_singles,
            ) else {
                continue;
            };
            let should_fire = match action.trigger_mode() {
                TriggerMode::Edge => newly_down.contains(button),
                TriggerMode::WhileHeld => held.contains(button),
            };
            if should_fire {
                fired.push((ControllerBinding::Single(*button), action));
            }
        }

        self.prev_held = held;
        fired
    }

    fn resolve_single(
        button: ControllerButton,
        target: &BindingTarget<A>,
        ctx: &WhenContext,
        newly_down: bool,
        latched: &mut HashMap<ControllerButton, Option<A>>,
    ) -> Option<A> {
        match target {
            BindingTarget::Always(action) => Some(action.clone()),
            BindingTarget::Conditional { .. } => {
                if newly_down {
                    let resolved = target.resolve(ctx).cloned();
                    latched.insert(button, resolved.clone());
                    resolved
                } else {
                    latched.get(&button).cloned().flatten()
                }
            }
        }
    }

    fn compute_held(&self, input: &dyn ControllerInput) -> HashSet<ControllerButton> {
        let extra = self
            .stick_dpad
            .as_ref()
            .and_then(|s| s.held())
            .map(DpadDir::to_button);
        self.buttons
            .iter()
            .filter(|b| input.query(**b) || extra == Some(**b))
            .copied()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controller::ControllerButton;
    use crate::state::actions::{Action, TriggerMode};
    use std::any::Any;

    #[derive(Clone, Debug, PartialEq, Eq)]
    enum TestAction {
        Chord,
        SingleFaceTop,
        RepeatFaceBottom,
        EdgeFaceBottom,
        SingleOptions,
        EdgeDpadUp,
        Accept,
        Type,
    }

    impl Action for TestAction {
        fn as_any(&self) -> &dyn Any {
            self
        }

        fn trigger_mode(&self) -> TriggerMode {
            match self {
                TestAction::RepeatFaceBottom | TestAction::Type => TriggerMode::WhileHeld,
                TestAction::Chord
                | TestAction::SingleFaceTop
                | TestAction::EdgeFaceBottom
                | TestAction::SingleOptions
                | TestAction::EdgeDpadUp
                | TestAction::Accept => TriggerMode::Edge,
            }
        }
    }

    fn engine() -> BindingEngine<TestAction> {
        let mut raw = HashMap::new();
        raw.insert(
            ControllerBinding::Chord {
                leader: ControllerButton::Options,
                follower: ControllerButton::FaceTop,
            },
            TestAction::Chord,
        );
        raw.insert(
            ControllerBinding::Single(ControllerButton::FaceTop),
            TestAction::SingleFaceTop,
        );
        BindingEngine::try_from_always(raw).expect("valid map")
    }

    use crate::controller::test_input::ButtonSetInput;

    fn held(buttons: &[ControllerButton]) -> HashSet<ControllerButton> {
        buttons.iter().cloned().collect()
    }

    fn step(
        engine: &mut BindingEngine<TestAction>,
        prev: &HashSet<ControllerButton>,
        now: &HashSet<ControllerButton>,
    ) -> Vec<TestAction> {
        engine.prev_held = prev.clone();
        engine
            .evaluate(&ButtonSetInput(now.clone()), &WhenContext::default())
            .into_iter()
            .map(|(_, a)| a)
            .collect()
    }

    #[test]
    fn leader_then_follower_fires_chord_once() {
        let mut e = engine();
        let empty = held(&[]);
        let options_only = held(&[ControllerButton::Options]);
        let both = held(&[ControllerButton::Options, ControllerButton::FaceTop]);

        assert!(step(&mut e, &empty, &options_only).is_empty());
        let fired = step(&mut e, &options_only, &both);
        assert_eq!(fired, vec![TestAction::Chord]);
        assert!(step(&mut e, &both, &both).is_empty());
    }

    #[test]
    fn wrong_order_fires_single_only() {
        let mut e = engine();
        let empty = held(&[]);
        let face_only = held(&[ControllerButton::FaceTop]);
        let both = held(&[ControllerButton::Options, ControllerButton::FaceTop]);

        assert_eq!(
            step(&mut e, &empty, &face_only),
            vec![TestAction::SingleFaceTop]
        );
        let fired = step(&mut e, &face_only, &both);
        assert!(!fired.contains(&TestAction::Chord));
    }

    #[test]
    fn face_top_alone_fires_single() {
        let mut e = engine();
        let empty = held(&[]);
        let face_only = held(&[ControllerButton::FaceTop]);
        assert_eq!(
            step(&mut e, &empty, &face_only),
            vec![TestAction::SingleFaceTop]
        );
    }

    #[test]
    fn while_held_fires_every_tick() {
        let mut raw = HashMap::new();
        raw.insert(
            ControllerBinding::Single(ControllerButton::FaceBottom),
            TestAction::RepeatFaceBottom,
        );
        let mut e = BindingEngine::try_from_always(raw).unwrap();
        let empty = held(&[]);
        let held_bottom = held(&[ControllerButton::FaceBottom]);
        assert_eq!(
            step(&mut e, &empty, &held_bottom),
            vec![TestAction::RepeatFaceBottom]
        );
        assert_eq!(
            step(&mut e, &held_bottom, &held_bottom),
            vec![TestAction::RepeatFaceBottom]
        );
    }

    #[test]
    fn edge_single_fires_once_per_hold() {
        let mut e = engine();
        let empty = held(&[]);
        let face_only = held(&[ControllerButton::FaceTop]);
        assert_eq!(
            step(&mut e, &empty, &face_only),
            vec![TestAction::SingleFaceTop]
        );
        assert!(step(&mut e, &face_only, &face_only).is_empty());
    }

    #[test]
    fn rejects_leader_with_single() {
        let mut raw = HashMap::new();
        raw.insert(
            ControllerBinding::Chord {
                leader: ControllerButton::Options,
                follower: ControllerButton::FaceTop,
            },
            TestAction::Chord,
        );
        raw.insert(
            ControllerBinding::Single(ControllerButton::Options),
            TestAction::SingleOptions,
        );
        assert!(BindingEngine::try_from_always(raw).is_err());
    }

    #[test]
    fn reset_holdover_suppresses_edge_until_repress() {
        let mut raw = HashMap::new();
        raw.insert(
            ControllerBinding::Single(ControllerButton::FaceBottom),
            TestAction::EdgeFaceBottom,
        );
        let mut e = BindingEngine::try_from_always(raw).unwrap();
        let held_bottom = ButtonSetInput(held(&[ControllerButton::FaceBottom]));
        let empty = ButtonSetInput(held(&[]));

        e.reset(Some(&held_bottom));
        assert!(e
            .evaluate(&held_bottom, &WhenContext::default())
            .into_iter()
            .map(|(_, a)| a)
            .collect::<Vec<_>>()
            .is_empty());

        assert!(e
            .evaluate(&empty, &WhenContext::default())
            .into_iter()
            .map(|(_, a)| a)
            .collect::<Vec<_>>()
            .is_empty());
        assert_eq!(
            e.evaluate(&held_bottom, &WhenContext::default())
                .into_iter()
                .map(|(_, a)| a)
                .collect::<Vec<_>>(),
            vec![TestAction::EdgeFaceBottom]
        );
    }

    #[test]
    fn reset_none_clears_baseline_so_held_edges() {
        let mut raw = HashMap::new();
        raw.insert(
            ControllerBinding::Single(ControllerButton::FaceBottom),
            TestAction::EdgeFaceBottom,
        );
        let mut e = BindingEngine::try_from_always(raw).unwrap();
        let held_bottom = ButtonSetInput(held(&[ControllerButton::FaceBottom]));

        e.reset(None);
        assert_eq!(
            e.evaluate(&held_bottom, &WhenContext::default())
                .into_iter()
                .map(|(_, a)| a)
                .collect::<Vec<_>>(),
            vec![TestAction::EdgeFaceBottom]
        );
    }

    #[test]
    fn stick_dpad_hysteresis() {
        let mut s = StickDpad::default();
        assert_eq!(s.update((0.0, -0.49)), None);
        assert_eq!(s.update((0.0, -0.50)), Some(DpadDir::Up));
        assert_eq!(s.update((0.0, -0.40)), Some(DpadDir::Up));
        assert_eq!(s.update((0.8, -0.9)), Some(DpadDir::Up), "no snap mid-hold");
        assert_eq!(s.update((0.0, -0.34)), None);
        assert_eq!(s.update((0.8, 0.4)), Some(DpadDir::Right));
        assert_eq!(DpadDir::Up.to_button(), ControllerButton::DpadUp);
        assert_eq!(DpadDir::Down.to_button(), ControllerButton::DpadDown);
        assert_eq!(DpadDir::Left.to_button(), ControllerButton::DpadLeft);
        assert_eq!(DpadDir::Right.to_button(), ControllerButton::DpadRight);
    }

    use crate::controller::test_input::AnalogInput;

    fn dpad_up_engine(stick: bool) -> BindingEngine<TestAction> {
        let mut raw = HashMap::new();
        raw.insert(
            ControllerBinding::Single(ControllerButton::DpadUp),
            TestAction::EdgeDpadUp,
        );
        raw.insert(
            ControllerBinding::Single(ControllerButton::FaceBottom),
            TestAction::EdgeFaceBottom,
        );
        let e = BindingEngine::try_from_always(raw).unwrap();
        if stick {
            e.with_left_stick_dpad()
        } else {
            e
        }
    }

    fn actions(fired: Vec<(ControllerBinding, TestAction)>) -> Vec<TestAction> {
        fired.into_iter().map(|(_, a)| a).collect()
    }

    #[test]
    fn left_stick_dpad_fires_edge_once() {
        let mut e = dpad_up_engine(true);
        let center = AnalogInput {
            stick: (0.0, 0.0),
            buttons: HashSet::new(),
        };
        let tilted = AnalogInput {
            stick: (0.0, -0.8),
            buttons: HashSet::new(),
        };
        assert!(actions(e.evaluate(&center, &WhenContext::default())).is_empty());
        assert_eq!(
            actions(e.evaluate(&tilted, &WhenContext::default())),
            vec![TestAction::EdgeDpadUp]
        );
        assert!(actions(e.evaluate(&tilted, &WhenContext::default())).is_empty());
        assert!(actions(e.evaluate(&tilted, &WhenContext::default())).is_empty());
    }

    #[test]
    fn left_stick_without_option_does_not_fire_dpad() {
        let mut e = dpad_up_engine(false);
        let tilted = AnalogInput {
            stick: (0.0, -0.8),
            buttons: HashSet::new(),
        };
        assert!(actions(e.evaluate(&tilted, &WhenContext::default())).is_empty());
    }

    #[test]
    fn left_stick_dpad_reset_holdover_suppresses_edge() {
        let mut e = dpad_up_engine(true);
        let tilted = AnalogInput {
            stick: (0.0, -0.8),
            buttons: HashSet::new(),
        };
        e.reset(Some(&tilted));
        assert!(actions(e.evaluate(&tilted, &WhenContext::default())).is_empty());
    }

    #[test]
    fn analog_up_does_not_fire_face_bottom() {
        let mut e = dpad_up_engine(true);
        let tilted = AnalogInput {
            stick: (0.0, -0.8),
            buttons: HashSet::new(),
        };
        let fired = actions(e.evaluate(&tilted, &WhenContext::default()));
        assert_eq!(fired, vec![TestAction::EdgeDpadUp]);
        assert!(!fired.contains(&TestAction::EdgeFaceBottom));
    }

    fn suggestion_ctx(selected: bool) -> WhenContext {
        WhenContext {
            suggestion_selected: selected,
            ..WhenContext::default()
        }
    }

    fn accept_or_type_engine() -> BindingEngine<TestAction> {
        let mut raw = HashMap::new();
        raw.insert(
            ControllerBinding::Single(ControllerButton::TriggerLeft),
            BindingTarget::Conditional {
                arms: vec![(
                    WhenExpr::parse("suggestionSelected").unwrap(),
                    TestAction::Accept,
                )],
                otherwise: Some(TestAction::Type),
            },
        );
        BindingEngine::try_from_raw(raw).unwrap()
    }

    #[test]
    fn conditional_first_match_accept_else_type() {
        let mut e = accept_or_type_engine();
        let empty = ButtonSetInput(held(&[]));
        let held_t = ButtonSetInput(held(&[ControllerButton::TriggerLeft]));
        e.evaluate(&empty, &suggestion_ctx(true));
        assert_eq!(
            actions(e.evaluate(&held_t, &suggestion_ctx(true))),
            vec![TestAction::Accept]
        );
        assert!(actions(e.evaluate(&held_t, &suggestion_ctx(true))).is_empty());
    }

    #[test]
    fn latch_survives_suggestion_selected_flip_mid_hold() {
        let mut e = accept_or_type_engine();
        let empty = ButtonSetInput(held(&[]));
        let held_t = ButtonSetInput(held(&[ControllerButton::TriggerLeft]));
        e.evaluate(&empty, &suggestion_ctx(true));
        assert_eq!(
            actions(e.evaluate(&held_t, &suggestion_ctx(true))),
            vec![TestAction::Accept]
        );
        assert!(
            actions(e.evaluate(&held_t, &suggestion_ctx(false))).is_empty(),
            "must not switch to WhileHeld Type after Accept cleared the highlight"
        );
        e.evaluate(&empty, &suggestion_ctx(false));
        assert_eq!(
            actions(e.evaluate(&held_t, &suggestion_ctx(false))),
            vec![TestAction::Type]
        );
        assert_eq!(
            actions(e.evaluate(&held_t, &suggestion_ctx(false))),
            vec![TestAction::Type]
        );
    }

    #[test]
    fn inline_when_without_fallback_is_idle_when_false() {
        let mut raw = HashMap::new();
        raw.insert(
            ControllerBinding::Single(ControllerButton::FaceRight),
            BindingTarget::Conditional {
                arms: vec![(
                    WhenExpr::parse("suggestionSelected").unwrap(),
                    TestAction::Accept,
                )],
                otherwise: None,
            },
        );
        let mut e = BindingEngine::try_from_raw(raw).unwrap();
        let empty = ButtonSetInput(held(&[]));
        let held_b = ButtonSetInput(held(&[ControllerButton::FaceRight]));
        e.evaluate(&empty, &suggestion_ctx(false));
        assert!(actions(e.evaluate(&held_b, &suggestion_ctx(false))).is_empty());
        e.evaluate(&empty, &suggestion_ctx(true));
        assert_eq!(
            actions(e.evaluate(&held_b, &suggestion_ctx(true))),
            vec![TestAction::Accept]
        );
    }
}
