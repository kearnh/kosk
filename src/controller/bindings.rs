use std::collections::{HashMap, HashSet};

use crate::controller::{ControllerBinding, ControllerButton, ControllerInput};
use crate::state::actions::{Action, TriggerMode};

struct ChordEntry<A> {
    leader: ControllerButton,
    follower: ControllerButton,
    action: A,
}

/// Resolves single-button and two-button chord mappings with leader-first chord semantics.
pub struct BindingEngine<A> {
    singles: HashMap<ControllerButton, A>,
    chords: Vec<ChordEntry<A>>,
    buttons: Vec<ControllerButton>,
    prev_held: HashSet<ControllerButton>,
    /// Followers that completed a chord this gesture; their single mapping is suppressed until release.
    suppress_single: HashSet<ControllerButton>,
    /// Chords already fired while both members are still held.
    chords_fired: HashSet<(ControllerButton, ControllerButton)>,
    /// Leaders currently held, for chord completion.
    leaders_active: HashSet<ControllerButton>,
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
        }
    }
}

impl<A: Action + Clone> BindingEngine<A> {
    pub fn try_from_raw(raw: HashMap<ControllerBinding, A>) -> Result<Self, String> {
        let mut singles = HashMap::new();
        let mut chords = Vec::new();
        let mut chord_leaders = HashSet::new();

        for (binding, action) in raw {
            match binding {
                ControllerBinding::Single(button) => {
                    singles.insert(button, action);
                }
                ControllerBinding::Chord { leader, follower } => {
                    chord_leaders.insert(leader.clone());
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
            buttons.insert(c.leader.clone());
            buttons.insert(c.follower.clone());
        }

        Ok(Self {
            singles,
            chords,
            buttons: buttons.into_iter().collect(),
            prev_held: HashSet::new(),
            suppress_single: HashSet::new(),
            chords_fired: HashSet::new(),
            leaders_active: HashSet::new(),
        })
    }

    pub fn reset(&mut self) {
        self.prev_held.clear();
        self.suppress_single.clear();
        self.chords_fired.clear();
        self.leaders_active.clear();
    }

    pub fn evaluate(&mut self, input: Option<&dyn ControllerInput>) -> Vec<(ControllerBinding, A)> {
        let Some(input) = input else {
            self.reset();
            return Vec::new();
        };

        let held = self.compute_held(input);
        let newly_down: HashSet<_> = held.difference(&self.prev_held).cloned().collect();
        let newly_up: HashSet<_> = self.prev_held.difference(&held).cloned().collect();

        for b in newly_up {
            self.suppress_single.remove(&b);
        }

        for chord in &self.chords {
            if chord.leader.query(input) {
                self.leaders_active.insert(chord.leader.clone());
            } else {
                self.leaders_active.remove(&chord.leader);
            }
        }

        self.chords_fired
            .retain(|(leader, follower)| leader.query(input) && follower.query(input));

        let mut fired = Vec::new();

        for chord in &self.chords {
            let key = (chord.leader.clone(), chord.follower.clone());
            if self.chords_fired.contains(&key) {
                continue;
            }
            if newly_down.contains(&chord.follower) && self.leaders_active.contains(&chord.leader) {
                fired.push((
                    ControllerBinding::Chord {
                        leader: chord.leader.clone(),
                        follower: chord.follower.clone(),
                    },
                    chord.action.clone(),
                ));
                self.chords_fired.insert(key);
                self.suppress_single.insert(chord.follower.clone());
            }
        }

        for (button, action) in &self.singles {
            if self.suppress_single.contains(button) {
                continue;
            }
            let should_fire = match action.trigger_mode() {
                TriggerMode::Edge => newly_down.contains(button),
                TriggerMode::WhileHeld => held.contains(button),
            };
            if should_fire {
                fired.push((ControllerBinding::Single(button.clone()), action.clone()));
            }
        }

        self.prev_held = held;
        fired
    }

    fn compute_held(&self, input: &dyn ControllerInput) -> HashSet<ControllerButton> {
        self.buttons
            .iter()
            .filter(|b| b.query(input))
            .cloned()
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
        SingleOptions,
    }

    impl Action for TestAction {
        fn as_any(&self) -> &dyn Any {
            self
        }

        fn trigger_mode(&self) -> TriggerMode {
            match self {
                TestAction::RepeatFaceBottom => TriggerMode::WhileHeld,
                TestAction::Chord | TestAction::SingleFaceTop | TestAction::SingleOptions => {
                    TriggerMode::Edge
                }
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
        BindingEngine::try_from_raw(raw).expect("valid map")
    }

    fn held(buttons: &[ControllerButton]) -> HashSet<ControllerButton> {
        buttons.iter().cloned().collect()
    }

    #[derive(Debug, Clone)]
    struct ButtonSetInput(HashSet<ControllerButton>);

    impl ControllerInput for ButtonSetInput {
        fn left_stick_raw(&self) -> (f32, f32) {
            (0.0, 0.0)
        }
        fn right_stick_raw(&self) -> (f32, f32) {
            (0.0, 0.0)
        }
        fn dpad_up(&self) -> bool {
            self.0.contains(&ControllerButton::DpadUp)
        }
        fn dpad_down(&self) -> bool {
            self.0.contains(&ControllerButton::DpadDown)
        }
        fn dpad_left(&self) -> bool {
            self.0.contains(&ControllerButton::DpadLeft)
        }
        fn dpad_right(&self) -> bool {
            self.0.contains(&ControllerButton::DpadRight)
        }
        fn face_bottom(&self) -> bool {
            self.0.contains(&ControllerButton::FaceBottom)
        }
        fn face_right(&self) -> bool {
            self.0.contains(&ControllerButton::FaceRight)
        }
        fn face_top(&self) -> bool {
            self.0.contains(&ControllerButton::FaceTop)
        }
        fn face_left(&self) -> bool {
            self.0.contains(&ControllerButton::FaceLeft)
        }
        fn shoulder_left(&self) -> bool {
            self.0.contains(&ControllerButton::ShoulderLeft)
        }
        fn shoulder_right(&self) -> bool {
            self.0.contains(&ControllerButton::ShoulderRight)
        }
        fn stick_left(&self) -> bool {
            self.0.contains(&ControllerButton::StickLeft)
        }
        fn stick_right(&self) -> bool {
            self.0.contains(&ControllerButton::StickRight)
        }
        fn trigger_left(&self) -> Option<u8> {
            None
        }
        fn trigger_right(&self) -> Option<u8> {
            None
        }
        fn btn_options(&self) -> bool {
            self.0.contains(&ControllerButton::Options)
        }
        fn btn_share(&self) -> bool {
            false
        }
        fn btn_system(&self) -> bool {
            false
        }
        fn box_clone(&self) -> Box<dyn ControllerInput + Send + Sync> {
            Box::new(self.clone())
        }
    }

    fn step(
        engine: &mut BindingEngine<TestAction>,
        prev: &HashSet<ControllerButton>,
        now: &HashSet<ControllerButton>,
    ) -> Vec<TestAction> {
        engine.prev_held = prev.clone();
        engine
            .evaluate(Some(&ButtonSetInput(now.clone())))
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
        assert!(!fired.iter().any(|a| *a == TestAction::Chord));
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
        let mut e = BindingEngine::try_from_raw(raw).unwrap();
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
        assert!(BindingEngine::try_from_raw(raw).is_err());
    }
}
