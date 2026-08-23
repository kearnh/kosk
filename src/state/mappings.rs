//! In-app Key Mappings editor (controller-operable, action-centric).

use crate::config;
use crate::controller::{ControllerBinding, ControllerButton, ControllerInput};
use crate::state::actions::get_action;
use crate::state::event::{Event, EventQueue, EventSource, ReturnStateResult};
use crate::state::keyboard::{self, KeyboardAction};
use crate::state::menu_action::MenuAction;
use crate::state::move_window_action::MoveWindowAction;
use crate::state::select_key;
use crate::state::text_input_action::TextInputAction;
use crate::state::StateId;
use anyhow::Result;
use egui::{
    Align, Button, Color32, Context, Frame, Label, Margin, RichText, ScrollArea, Sense, Ui,
    Vec2,
};
use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};
use strum::VariantNames;

// --- Hardcoded Mappings chrome (not user-remappable; edit here only) ---
const NAV_UP: ControllerButton = ControllerButton::DpadUp;
const NAV_DOWN: ControllerButton = ControllerButton::DpadDown;
const NAV_LEFT: ControllerButton = ControllerButton::DpadLeft;
const NAV_RIGHT: ControllerButton = ControllerButton::DpadRight;
const TAB_PREV: ControllerButton = ControllerButton::ShoulderLeft; // L1
const TAB_NEXT: ControllerButton = ControllerButton::ShoulderRight; // R1
const ACTIVATE: ControllerButton = ControllerButton::FaceBottom; // A
const LISTEN_SINGLE: ControllerButton = ControllerButton::FaceLeft; // X
const LISTEN_CHORD: ControllerButton = ControllerButton::FaceTop; // Y
const DELETE_BINDING: ControllerButton = ControllerButton::Share;
const BACK_CANCEL: ControllerButton = ControllerButton::FaceRight; // B
                                                                   // ----------------------------------------------------------------------

const EDITABLE_MODES: [StateId; 4] = [
    StateId::Keyboard,
    StateId::Menu,
    StateId::TextInput,
    StateId::MoveWindow,
];

const ALL_BUTTONS: &[ControllerButton] = &[
    ControllerButton::DpadUp,
    ControllerButton::DpadDown,
    ControllerButton::DpadLeft,
    ControllerButton::DpadRight,
    ControllerButton::FaceBottom,
    ControllerButton::FaceRight,
    ControllerButton::FaceLeft,
    ControllerButton::FaceTop,
    ControllerButton::ShoulderLeft,
    ControllerButton::ShoulderRight,
    ControllerButton::StickLeft,
    ControllerButton::StickRight,
    ControllerButton::TriggerLeft,
    ControllerButton::TriggerRight,
    ControllerButton::Options,
    ControllerButton::Share,
    ControllerButton::System,
    ControllerButton::PadLeft,
    ControllerButton::PadRight,
    ControllerButton::L4,
    ControllerButton::L5,
    ControllerButton::R4,
    ControllerButton::R5,
];

const SEND_KEY_GATEWAY: &str = "sendKey";

const ACTION_COL_WIDTH: f32 = 200.0;
const BINDING_COL_GAP: f32 = 16.0;

fn row_focus_fill() -> Color32 {
    Color32::from_rgba_unmultiplied(40, 90, 160, 80)
}

/// Why SelectKey was opened; consumed in [`MappingsState::on_return`].
enum PendingKeyPick {
    AddBinding { binding: ControllerBinding },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FocusZone {
    Tabs,
    Table,
    Footer,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FooterItem {
    Delete,
    Cancel,
    Save,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ListenMode {
    Single,
    Chord,
}

enum ListenTarget {
    Replace { action: String, pill: usize },
    Add { action: String },
}

struct ListenState {
    mode: ListenMode,
    target: ListenTarget,
    chord_leader: Option<ControllerButton>,
}

/// Shared conflict check used by listen-commit and Save.
/// `ignore` = binding currently on the pill being replaced (treated as absent).
fn validate_binding_candidate(
    draft_mode: &HashMap<String, Vec<ControllerBinding>>,
    candidate: &ControllerBinding,
    ignore: Option<&ControllerBinding>,
) -> Result<(), String> {
    if let ControllerBinding::Chord { leader, follower } = candidate {
        if leader == follower {
            return Err("conflict: chord leader and follower must differ".to_owned());
        }
    }

    let mut occupied: HashMap<ControllerBinding, String> = HashMap::new();
    let mut singles: HashSet<ControllerButton> = HashSet::new();
    let mut leaders: HashSet<ControllerButton> = HashSet::new();

    for (action, bindings) in draft_mode {
        for binding in bindings {
            if ignore.is_some_and(|ig| ig == binding) {
                continue;
            }
            occupied.insert(binding.clone(), action.clone());
            match binding {
                ControllerBinding::Single(b) => {
                    singles.insert(b.clone());
                }
                ControllerBinding::Chord { leader, .. } => {
                    leaders.insert(leader.clone());
                }
            }
        }
    }

    if let Some(action) = occupied.get(candidate) {
        return Err(format!("conflict: {candidate} already bound to {action}"));
    }

    match candidate {
        ControllerBinding::Single(button) => {
            if leaders.contains(button) {
                return Err(format!(
                    "conflict: {button} is a chord leader and cannot also be a single"
                ));
            }
        }
        ControllerBinding::Chord { leader, .. } => {
            if singles.contains(leader) {
                return Err(format!(
                    "conflict: {leader} already has a single mapping and cannot be a chord leader"
                ));
            }
        }
    }

    Ok(())
}

fn to_camel(pascal: &str) -> String {
    let mut chars = pascal.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_lowercase().chain(chars).collect(),
    }
}

fn unit_names(variants: &[&str], skip: &[&str]) -> Vec<String> {
    variants
        .iter()
        .filter(|v| !skip.iter().any(|s| s.eq_ignore_ascii_case(v)))
        .map(|v| to_camel(v))
        .collect()
}

fn switch_state_catalog() -> Vec<String> {
    StateId::VARIANTS
        .iter()
        .map(|v| format!("switchState.{}", to_camel(v)))
        .collect()
}

fn pill_count(action: &str, draft: &HashMap<String, Vec<ControllerBinding>>) -> usize {
    if action == SEND_KEY_GATEWAY {
        0
    } else {
        draft.get(action).map(|v| v.len()).unwrap_or(0)
    }
}

fn max_focus_col(n_pills: usize) -> usize {
    n_pills // index of [+]
}

fn catalog_rows(mode: StateId, draft: &HashMap<String, Vec<ControllerBinding>>) -> Vec<String> {
    match mode {
        StateId::Keyboard => {
            let mut rows = unit_names(
                KeyboardAction::VARIANTS,
                &["SendKey", "SendEnigoKey", "SwitchState", "SwitchLayout"],
            );
            rows.extend(switch_state_catalog());
            for layout in keyboard::layout_names() {
                rows.push(format!("switchLayout.{layout}"));
            }
            rows.sort();
            rows.push(SEND_KEY_GATEWAY.to_owned());
            let mut concrete: Vec<String> = draft
                .iter()
                .filter(|(n, b)| n.starts_with("sendKey.") && !b.is_empty())
                .map(|(n, _)| n.clone())
                .collect();
            concrete.sort();
            rows.extend(concrete);
            rows
        }
        StateId::Menu => {
            let mut rows = unit_names(MenuAction::VARIANTS, &["SwitchState"]);
            rows.extend(switch_state_catalog());
            rows.sort();
            rows
        }
        StateId::TextInput => {
            let mut rows = unit_names(TextInputAction::VARIANTS, &["SwitchState"]);
            rows.extend(switch_state_catalog());
            rows.sort();
            rows
        }
        StateId::MoveWindow => {
            let mut rows = unit_names(MoveWindowAction::VARIANTS, &["SwitchState"]);
            rows.extend(switch_state_catalog());
            rows.sort();
            rows
        }
        StateId::Mappings | StateId::SelectKey => Vec::new(),
    }
}

fn mode_label(mode: StateId) -> &'static str {
    match mode {
        StateId::Keyboard => "Keyboard",
        StateId::Menu => "Menu",
        StateId::TextInput => "TextInput",
        StateId::MoveWindow => "MoveWindow",
        StateId::Mappings => "Mappings",
        StateId::SelectKey => "SelectKey",
    }
}

fn tab_index(mode: StateId) -> usize {
    EDITABLE_MODES.iter().position(|m| *m == mode).unwrap_or(0)
}

fn held_set(input: &dyn ControllerInput) -> HashSet<ControllerButton> {
    ALL_BUTTONS
        .iter()
        .filter(|b| b.query(input))
        .cloned()
        .collect()
}

fn rising(
    button: ControllerButton,
    held: &HashSet<ControllerButton>,
    prev: &HashSet<ControllerButton>,
) -> bool {
    held.contains(&button) && !prev.contains(&button)
}

fn source_for(button: ControllerButton) -> EventSource {
    EventSource::Controller(ControllerBinding::Single(button))
}

pub struct MappingsState {
    draft: HashMap<StateId, HashMap<String, Vec<ControllerBinding>>>,
    dirty: bool,
    status: String,
    tab: StateId,
    focus_zone: FocusZone,
    focus_row: usize,
    /// 0..n_pills = pill index; n_pills = trailing `[+]` (no action-label column).
    focus_col: usize,
    footer_focus: FooterItem,
    listen: Option<ListenState>,
    pending_key_pick: Option<PendingKeyPick>,
    prev_held: HashSet<ControllerButton>,
}

impl MappingsState {
    pub fn new() -> Self {
        Self {
            draft: HashMap::new(),
            dirty: false,
            status: "ready".to_owned(),
            tab: StateId::Keyboard,
            focus_zone: FocusZone::Table,
            focus_row: 0,
            focus_col: 0,
            footer_focus: FooterItem::Save,
            listen: None,
            pending_key_pick: None,
            prev_held: HashSet::new(),
        }
    }

    /// Enter via ChangeState — reload draft from config.
    pub fn begin_session(&mut self) {
        self.load_draft_from_config();
        self.dirty = false;
        if !self.status.starts_with("warning:") {
            self.status = "ready".to_owned();
        }
        self.tab = StateId::Keyboard;
        self.focus_zone = FocusZone::Table;
        self.focus_row = 0;
        self.focus_col = 0;
        self.footer_focus = FooterItem::Save;
        self.listen = None;
        self.pending_key_pick = None;
        self.prev_held.clear();
    }

    fn load_draft_from_config(&mut self) {
        let cfg = config::get();
        let mut draft: HashMap<StateId, HashMap<String, Vec<ControllerBinding>>> = HashMap::new();
        for mode in EDITABLE_MODES {
            draft.insert(mode, HashMap::new());
        }
        let mut unknown = 0usize;
        for mode in EDITABLE_MODES {
            let Some(raw) = cfg.controller_map.get(&mode) else {
                continue;
            };
            let mode_draft = draft.get_mut(&mode).expect("mode present");
            for (binding, action_name) in raw {
                if get_action(mode, action_name).is_none() {
                    unknown += 1;
                    continue;
                }
                mode_draft
                    .entry(action_name.clone())
                    .or_default()
                    .push(binding.clone());
            }
        }
        self.draft = draft;
        if unknown > 0 {
            self.status = format!("warning: {unknown} unknown actions ignored");
        } else {
            self.status = "ready".to_owned();
        }
    }

    fn current_draft(&self) -> &HashMap<String, Vec<ControllerBinding>> {
        self.draft.get(&self.tab).expect("editable tab draft")
    }

    fn current_draft_mut(&mut self) -> &mut HashMap<String, Vec<ControllerBinding>> {
        self.draft.get_mut(&self.tab).expect("editable tab draft")
    }

    fn rows(&self) -> Vec<String> {
        catalog_rows(self.tab, self.current_draft())
    }

    fn clamp_focus(&mut self) {
        let rows = self.rows();
        if rows.is_empty() {
            self.focus_row = 0;
            self.focus_col = 0;
            return;
        }
        if self.focus_row >= rows.len() {
            self.focus_row = rows.len() - 1;
        }
        let action = &rows[self.focus_row];
        let n_pills = pill_count(action, self.current_draft());
        let max_col = max_focus_col(n_pills);
        if self.focus_col > max_col {
            self.focus_col = max_col;
        }
    }

    /// Apply SelectKey result. May enqueue follow-ups onto `out` (passthrough
    /// queue from `process_events`); those run in the same `process_events`
    /// call after the current batch. Use [`EventSource::FollowUp`] for
    /// programmatic follow-ups (not MouseClick). When mutating visible
    /// draft/status, also `out.push(Event::Repaint, &EventSource::FollowUp)`.
    pub fn on_return(&mut self, result: ReturnStateResult, out: &mut EventQueue) {
        let Some(pending) = self.pending_key_pick.take() else {
            return;
        };
        let ReturnStateResult::Value(value) = result else {
            return; // Cancelled; draft unchanged — no Repaint required
        };
        let trimmed = value.trim();
        let new_action = format!("sendKey.{trimmed}");
        if get_action(StateId::Keyboard, &new_action).is_none() {
            self.status = format!("invalid key: {trimmed}");
            let _ = out.push(Event::Repaint, &EventSource::FollowUp);
            return;
        }

        match pending {
            PendingKeyPick::AddBinding { binding } => {
                let draft = self.draft.entry(StateId::Keyboard).or_default();
                if let Err(err) = validate_binding_candidate(draft, &binding, None) {
                    self.status = err;
                    let _ = out.push(Event::Repaint, &EventSource::FollowUp);
                    return;
                }
                draft.entry(new_action.clone()).or_default().push(binding);
                self.dirty = true;
                self.status = "ready".to_owned();
                self.tab = StateId::Keyboard;
                let rows = catalog_rows(StateId::Keyboard, draft);
                if let Some(idx) = rows.iter().position(|r| r == &new_action) {
                    self.focus_row = idx;
                    let n = draft.get(&new_action).map(|v| v.len()).unwrap_or(1);
                    // New indexing: last pill is n-1; [+] is n.
                    self.focus_col = n.saturating_sub(1);
                    self.focus_zone = FocusZone::Table;
                }
            }
        }
        let _ = out.push(Event::Repaint, &EventSource::FollowUp);
    }

    fn set_tab(&mut self, mode: StateId) {
        self.tab = mode;
        self.focus_row = 0;
        self.focus_col = 0;
        self.focus_zone = FocusZone::Tabs;
        self.clamp_focus();
    }

    fn start_listen(
        &mut self,
        mode: ListenMode,
        target: ListenTarget,
        held: &HashSet<ControllerButton>,
    ) {
        self.listen = Some(ListenState {
            mode,
            target,
            chord_leader: None,
        });
        self.prev_held = held.clone();
        self.status = match mode {
            ListenMode::Single => "LISTENING — single button".to_owned(),
            ListenMode::Chord => "LISTENING — chord".to_owned(),
        };
    }

    fn cancel_listen(&mut self) {
        self.listen = None;
        self.status = "ready".to_owned();
    }

    fn ignore_for(
        draft: &HashMap<String, Vec<ControllerBinding>>,
        target: &ListenTarget,
    ) -> Option<ControllerBinding> {
        match target {
            ListenTarget::Replace { action, pill } => {
                draft.get(action).and_then(|v| v.get(*pill).cloned())
            }
            ListenTarget::Add { .. } => None,
        }
    }

    fn commit_capture(
        &mut self,
        candidate: ControllerBinding,
        events: &mut EventQueue,
        source: &EventSource,
    ) {
        let Some(listen) = self.listen.take() else {
            return;
        };
        let draft = self.current_draft();
        let ignore = Self::ignore_for(draft, &listen.target);
        if let Err(err) = validate_binding_candidate(draft, &candidate, ignore.as_ref()) {
            self.status = err;
            self.listen = Some(listen);
            return;
        }

        if let ListenTarget::Replace { action, pill } = &listen.target {
            if let Some(old) = self.current_draft().get(action).and_then(|v| v.get(*pill)) {
                if old == &candidate {
                    self.status = "ready".to_owned();
                    return;
                }
            }
        }

        match listen.target {
            ListenTarget::Replace { action, pill } => {
                if let Some(vec) = self.current_draft_mut().get_mut(&action) {
                    if pill < vec.len() {
                        vec[pill] = candidate;
                        self.dirty = true;
                        self.status = "ready".to_owned();
                    }
                }
            }
            ListenTarget::Add { action } => {
                if action == SEND_KEY_GATEWAY {
                    self.pending_key_pick = Some(PendingKeyPick::AddBinding { binding: candidate });
                    select_key::with_mut(|s| s.begin(""));
                    let _ = events.push(Event::CallState(StateId::SelectKey), source);
                    self.status = "ready".to_owned();
                } else {
                    self.current_draft_mut()
                        .entry(action)
                        .or_default()
                        .push(candidate);
                    self.dirty = true;
                    self.status = "ready".to_owned();
                }
            }
        }
    }

    fn delete_focused_pill(&mut self) {
        let rows = self.rows();
        if self.focus_row >= rows.len() {
            return;
        }
        let action = rows[self.focus_row].clone();
        let n_pills = pill_count(&action, self.current_draft());
        if action == SEND_KEY_GATEWAY || self.focus_col >= n_pills {
            return; // on [+] or gateway
        }
        let pill = self.focus_col;
        let draft = self.current_draft_mut();
        let Some(vec) = draft.get_mut(&action) else {
            return;
        };
        if pill >= vec.len() {
            return;
        }
        vec.remove(pill);
        let empty = vec.is_empty();
        if empty {
            draft.remove(&action);
        }
        self.dirty = true;
        self.status = "ready".to_owned();
        self.clamp_focus();
    }

    fn activate_focused(
        &mut self,
        events: &mut EventQueue,
        source: &EventSource,
        held: &HashSet<ControllerButton>,
    ) {
        match self.focus_zone {
            FocusZone::Tabs => {}
            FocusZone::Footer => match self.footer_focus {
                FooterItem::Delete => self.delete_focused_pill(),
                FooterItem::Cancel => self.do_cancel(events, source),
                FooterItem::Save => self.do_save(),
            },
            FocusZone::Table => {
                let rows = self.rows();
                if self.focus_row >= rows.len() {
                    return;
                }
                let action = rows[self.focus_row].clone();
                let n_pills = pill_count(&action, self.current_draft());
                if self.focus_col == n_pills {
                    self.start_listen(ListenMode::Single, ListenTarget::Add { action }, held);
                } else {
                    let pill = self.focus_col;
                    self.start_listen(
                        ListenMode::Single,
                        ListenTarget::Replace { action, pill },
                        held,
                    );
                }
            }
        }
    }

    fn listen_on_focused(&mut self, mode: ListenMode, held: &HashSet<ControllerButton>) {
        if self.focus_zone != FocusZone::Table {
            return;
        }
        let rows = self.rows();
        if self.focus_row >= rows.len() {
            return;
        }
        let action = rows[self.focus_row].clone();
        let n_pills = pill_count(&action, self.current_draft());
        if self.focus_col == n_pills {
            self.start_listen(mode, ListenTarget::Add { action }, held);
        } else {
            let pill = self.focus_col;
            self.start_listen(mode, ListenTarget::Replace { action, pill }, held);
        }
    }

    fn do_cancel(&mut self, events: &mut EventQueue, source: &EventSource) {
        self.listen = None;
        self.pending_key_pick = None;
        let _ = events.push(Event::ChangeState(StateId::Menu), source);
    }

    fn do_save(&mut self) {
        for mode in EDITABLE_MODES {
            let draft_mode = self.draft.get(&mode).cloned().unwrap_or_default();
            for (action, bindings) in &draft_mode {
                if get_action(mode, action).is_none() {
                    self.status = format!("invalid action: {action}");
                    self.tab = mode;
                    return;
                }
                for (i, binding) in bindings.iter().enumerate() {
                    let mut temp = draft_mode.clone();
                    if let Some(vec) = temp.get_mut(action) {
                        if i < vec.len() {
                            vec.remove(i);
                        }
                    }
                    if let Err(err) = validate_binding_candidate(&temp, binding, None) {
                        self.status = err;
                        self.tab = mode;
                        return;
                    }
                }
            }
        }

        let mut new_map: HashMap<StateId, HashMap<ControllerBinding, String>> = HashMap::new();
        for mode in EDITABLE_MODES {
            let mut inverted = HashMap::new();
            if let Some(draft_mode) = self.draft.get(&mode) {
                for (action, bindings) in draft_mode {
                    for binding in bindings {
                        inverted.insert(binding.clone(), action.clone());
                    }
                }
            }
            new_map.insert(mode, inverted);
        }

        let mut cfg = config::get();
        cfg.controller_map = new_map;
        if let Err(e) = config::save(cfg) {
            self.status = format!("save failed: {e:#}");
            return;
        }
        config::notify_changed();
        self.dirty = false;
        self.status = "saved".to_owned();
    }

    fn handle_listen_input(&mut self, input: &dyn ControllerInput, events: &mut EventQueue) {
        let held = held_set(input);
        let newly: Vec<ControllerButton> = held.difference(&self.prev_held).cloned().collect();

        if rising(BACK_CANCEL, &held, &self.prev_held) {
            self.cancel_listen();
            self.prev_held = held;
            return;
        }

        let Some(listen) = self.listen.as_mut() else {
            self.prev_held = held;
            return;
        };

        match listen.mode {
            ListenMode::Single => {
                if let Some(btn) = newly.into_iter().next() {
                    let candidate = ControllerBinding::Single(btn.clone());
                    let source = source_for(btn);
                    self.commit_capture(candidate, events, &source);
                }
            }
            ListenMode::Chord => match listen.chord_leader.clone() {
                None => {
                    if let Some(btn) = newly.into_iter().next() {
                        listen.chord_leader = Some(btn);
                        self.status = "LISTENING — chord (hold leader, press follower)".to_owned();
                    }
                }
                Some(leader) => {
                    if !held.contains(&leader) {
                        listen.chord_leader = None;
                        self.status = "LISTENING — chord".to_owned();
                    } else if let Some(follower) = newly.into_iter().find(|b| *b != leader) {
                        let candidate = ControllerBinding::Chord {
                            leader: leader.clone(),
                            follower: follower.clone(),
                        };
                        let source = EventSource::Controller(candidate.clone());
                        self.commit_capture(candidate, events, &source);
                    }
                }
            },
        }
        self.prev_held = held_set(input);
    }

    fn handle_browse_input(&mut self, input: &dyn ControllerInput, events: &mut EventQueue) {
        let held = held_set(input);
        let prev = self.prev_held.clone();

        if rising(BACK_CANCEL, &held, &prev) {
            self.do_cancel(events, &source_for(BACK_CANCEL));
            self.prev_held = held;
            return;
        }
        if rising(TAB_PREV, &held, &prev) {
            let i = tab_index(self.tab);
            let next = EDITABLE_MODES[(i + EDITABLE_MODES.len() - 1) % EDITABLE_MODES.len()];
            self.set_tab(next);
            self.prev_held = held;
            return;
        }
        if rising(TAB_NEXT, &held, &prev) {
            let i = tab_index(self.tab);
            let next = EDITABLE_MODES[(i + 1) % EDITABLE_MODES.len()];
            self.set_tab(next);
            self.prev_held = held;
            return;
        }
        if rising(DELETE_BINDING, &held, &prev) {
            self.delete_focused_pill();
            self.prev_held = held;
            return;
        }
        if rising(LISTEN_SINGLE, &held, &prev) {
            self.listen_on_focused(ListenMode::Single, &held);
            self.prev_held = held_set(input);
            return;
        }
        if rising(LISTEN_CHORD, &held, &prev) {
            self.listen_on_focused(ListenMode::Chord, &held);
            self.prev_held = held_set(input);
            return;
        }
        if rising(ACTIVATE, &held, &prev) {
            self.activate_focused(events, &source_for(ACTIVATE), &held);
            self.prev_held = held_set(input);
            return;
        }

        if rising(NAV_UP, &held, &prev) {
            match self.focus_zone {
                FocusZone::Tabs => {}
                FocusZone::Table => {
                    if self.focus_row == 0 {
                        self.focus_zone = FocusZone::Tabs;
                    } else {
                        self.focus_row -= 1;
                        self.focus_col = 0;
                        self.clamp_focus();
                    }
                }
                FocusZone::Footer => {
                    self.focus_zone = FocusZone::Table;
                    self.focus_col = 0;
                    self.clamp_focus();
                }
            }
        } else if rising(NAV_DOWN, &held, &prev) {
            match self.focus_zone {
                FocusZone::Tabs => {
                    self.focus_zone = FocusZone::Table;
                    self.focus_col = 0;
                    self.clamp_focus();
                }
                FocusZone::Table => {
                    let n = self.rows().len();
                    if n == 0 || self.focus_row + 1 >= n {
                        self.focus_zone = FocusZone::Footer;
                    } else {
                        self.focus_row += 1;
                        self.focus_col = 0;
                        self.clamp_focus();
                    }
                }
                FocusZone::Footer => {}
            }
        } else if rising(NAV_LEFT, &held, &prev) {
            match self.focus_zone {
                FocusZone::Tabs => {
                    let i = tab_index(self.tab);
                    let next =
                        EDITABLE_MODES[(i + EDITABLE_MODES.len() - 1) % EDITABLE_MODES.len()];
                    self.set_tab(next);
                }
                FocusZone::Table => {
                    if self.focus_col > 0 {
                        self.focus_col -= 1;
                        self.clamp_focus();
                    }
                }
                FocusZone::Footer => {
                    self.footer_focus = match self.footer_focus {
                        FooterItem::Delete => FooterItem::Delete,
                        FooterItem::Cancel => FooterItem::Delete,
                        FooterItem::Save => FooterItem::Cancel,
                    };
                }
            }
        } else if rising(NAV_RIGHT, &held, &prev) {
            match self.focus_zone {
                FocusZone::Tabs => {
                    let i = tab_index(self.tab);
                    let next = EDITABLE_MODES[(i + 1) % EDITABLE_MODES.len()];
                    self.set_tab(next);
                }
                FocusZone::Table => {
                    self.focus_col += 1;
                    self.clamp_focus();
                }
                FocusZone::Footer => {
                    self.footer_focus = match self.footer_focus {
                        FooterItem::Delete => FooterItem::Cancel,
                        FooterItem::Cancel => FooterItem::Save,
                        FooterItem::Save => FooterItem::Save,
                    };
                }
            }
        }

        self.prev_held = held;
    }

    pub fn draw_ui(&mut self, _: &Context, ui: &mut Ui, events: &mut EventQueue) {
        // Defensive: if we somehow still have a pending pick with no active call,
        // drop it on paint when not mid-listen (ChangeState already clears stack).
        if self.pending_key_pick.is_some() && self.listen.is_none() {
            // Keep pending only across CallState; parent clears stack on ChangeState.
            // Soft clear happens when begin_session runs; leave as-is here.
        }

        let listening = self.listen.is_some();

        ui.horizontal(|ui| {
            ui.heading(RichText::new("Key Mappings").color(Color32::WHITE));
            if self.dirty {
                ui.label("unsaved*");
            }
        });

        ui.horizontal(|ui| {
            for mode in EDITABLE_MODES {
                let selected = self.tab == mode;
                let focused = self.focus_zone == FocusZone::Tabs && self.tab == mode;
                let label = if focused {
                    format!("> {} <", mode_label(mode))
                } else {
                    mode_label(mode).to_owned()
                };
                if ui.add(Button::new(label).selected(selected)).clicked() {
                    self.set_tab(mode);
                    self.focus_zone = FocusZone::Tabs;
                }
            }
        });

        ui.separator();

        let rows = self.rows();
        let draft_snapshot = self.current_draft().clone();
        let focus_zone = self.focus_zone;
        let focus_row = self.focus_row;
        let focus_col = self.focus_col;

        let mut clicked_listen: Option<(ListenMode, ListenTarget)> = None;
        let mut clicked_focus: Option<(usize, usize)> = None;

        ui.add_enabled_ui(!listening, |ui| {
            // allocate_ui_with_layout only advances by content width; add_sized
            // keeps Action as a true fixed column under the header.
            ui.horizontal(|ui| {
                ui.add_sized(
                    Vec2::new(ACTION_COL_WIDTH, ui.spacing().interact_size.y),
                    Label::new(RichText::new("Action").strong()),
                );
                ui.add_space(BINDING_COL_GAP);
                ui.strong("Binding");
            });

            ScrollArea::vertical()
                .max_height(8.0 * 28.0)
                .show(ui, |ui| {
                    for (row_idx, action) in rows.iter().enumerate() {
                        let pills = if action.as_str() == SEND_KEY_GATEWAY {
                            Vec::new()
                        } else {
                            draft_snapshot.get(action).cloned().unwrap_or_default()
                        };
                        let row_focused = focus_zone == FocusZone::Table && focus_row == row_idx;
                        let n_pills = pills.len();
                        let row_fill = if row_focused {
                            row_focus_fill()
                        } else {
                            Color32::TRANSPARENT
                        };
                        // Vertical-only margin so Binding lines up with header.
                        let row_resp = Frame::NONE
                            .fill(row_fill)
                            .inner_margin(Margin::symmetric(0, 1))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    let action_resp = ui.add_sized(
                                        Vec2::new(
                                            ACTION_COL_WIDTH,
                                            ui.spacing().interact_size.y,
                                        ),
                                        Label::new(action.as_str()).sense(Sense::click()),
                                    );
                                    if action_resp.clicked() {
                                        clicked_focus = Some((row_idx, 0));
                                    }

                                    ui.add_space(BINDING_COL_GAP);

                                    for (pill_idx, binding) in pills.iter().enumerate() {
                                        let col = pill_idx;
                                        let selected = row_focused && focus_col == col;
                                        let resp = ui.add(
                                            Button::new(format!("[{binding}]")).selected(selected),
                                        );
                                        if resp.clicked() {
                                            clicked_focus = Some((row_idx, col));
                                            clicked_listen = Some((
                                                ListenMode::Single,
                                                ListenTarget::Replace {
                                                    action: action.clone(),
                                                    pill: pill_idx,
                                                },
                                            ));
                                        }
                                        if resp.secondary_clicked() {
                                            clicked_focus = Some((row_idx, col));
                                            clicked_listen = Some((
                                                ListenMode::Chord,
                                                ListenTarget::Replace {
                                                    action: action.clone(),
                                                    pill: pill_idx,
                                                },
                                            ));
                                        }
                                    }

                                    let plus_col = n_pills;
                                    let plus_selected = row_focused && focus_col == plus_col;
                                    let plus_resp = ui
                                        .add(Button::new("[+]").selected(plus_selected));
                                    if plus_resp.clicked() {
                                        clicked_focus = Some((row_idx, plus_col));
                                        clicked_listen = Some((
                                            ListenMode::Single,
                                            ListenTarget::Add {
                                                action: action.clone(),
                                            },
                                        ));
                                    }
                                    if plus_resp.secondary_clicked() {
                                        clicked_focus = Some((row_idx, plus_col));
                                        clicked_listen = Some((
                                            ListenMode::Chord,
                                            ListenTarget::Add {
                                                action: action.clone(),
                                            },
                                        ));
                                    }
                                });
                            })
                            .response;
                        if row_focused {
                            row_resp.scroll_to_me(Some(Align::Center));
                        }
                        if row_resp.clicked() && clicked_focus.is_none() {
                            clicked_focus = Some((row_idx, 0));
                        }
                    }
                });
        });

        if let Some((row, col)) = clicked_focus {
            self.focus_zone = FocusZone::Table;
            self.focus_row = row;
            self.focus_col = col;
            self.clamp_focus();
        }
        if let Some((mode, target)) = clicked_listen {
            self.start_listen(mode, target, &HashSet::new());
        }

        ui.separator();

        if listening {
            let kind = match self.listen.as_ref().map(|l| l.mode) {
                Some(ListenMode::Single) => "single button",
                Some(ListenMode::Chord) => "chord",
                None => "",
            };
            ui.colored_label(egui::Color32::YELLOW, format!("LISTENING — {kind} …"));
            ui.label("B = cancel listen");
        } else {
            ui.label(format!("status: {}", self.status));
            ui.horizontal(|ui| {
                let del_sel =
                    self.focus_zone == FocusZone::Footer && self.footer_focus == FooterItem::Delete;
                if ui
                    .add(Button::new("Delete binding").selected(del_sel))
                    .clicked()
                {
                    self.focus_zone = FocusZone::Footer;
                    self.footer_focus = FooterItem::Delete;
                    self.delete_focused_pill();
                }
                let cancel_sel =
                    self.focus_zone == FocusZone::Footer && self.footer_focus == FooterItem::Cancel;
                if ui.add(Button::new("Cancel").selected(cancel_sel)).clicked() {
                    self.do_cancel(events, &EventSource::MouseClick);
                }
                let save_sel =
                    self.focus_zone == FocusZone::Footer && self.footer_focus == FooterItem::Save;
                if ui.add(Button::new("Save").selected(save_sel)).clicked() {
                    self.focus_zone = FocusZone::Footer;
                    self.footer_focus = FooterItem::Save;
                    self.do_save();
                }
            });
            ui.label("A activate/+   X listen single   Y listen chord   Share delete pill");
            ui.label("B Back/Cancel   L1/R1 mode tab   D-pad move row / pill");
        }
    }

    pub fn handle_controller_input(
        &mut self,
        _: &Context,
        input: &Option<Box<dyn ControllerInput>>,
        events: &mut EventQueue,
    ) -> Result<()> {
        let Some(input) = input.as_ref() else {
            self.prev_held.clear();
            return Ok(());
        };
        if self.listen.is_some() {
            self.handle_listen_input(input.as_ref(), events);
        } else {
            self.handle_browse_input(input.as_ref(), events);
        }
        Ok(())
    }
}

static MAPPINGS: OnceLock<Mutex<MappingsState>> = OnceLock::new();

pub fn init() -> Result<()> {
    MAPPINGS
        .set(Mutex::new(MappingsState::new()))
        .map_err(|_| anyhow::anyhow!("mappings state already initialized"))?;
    Ok(())
}

pub(crate) fn with_mut<R>(f: impl FnOnce(&mut MappingsState) -> R) -> R {
    let mut guard = MAPPINGS
        .get()
        .expect("mappings state not initialized")
        .lock()
        .unwrap();
    f(&mut guard)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn single(b: ControllerButton) -> ControllerBinding {
        ControllerBinding::Single(b)
    }

    fn chord(leader: ControllerButton, follower: ControllerButton) -> ControllerBinding {
        ControllerBinding::Chord { leader, follower }
    }

    #[test]
    fn rejects_duplicate_binding() {
        let mut draft = HashMap::new();
        draft.insert(
            "toggleShift".to_owned(),
            vec![single(ControllerButton::FaceTop)],
        );
        let err = validate_binding_candidate(&draft, &single(ControllerButton::FaceTop), None)
            .unwrap_err();
        assert_eq!(err, "conflict: faceTop already bound to toggleShift");
    }

    #[test]
    fn rejects_single_on_chord_leader() {
        let mut draft = HashMap::new();
        draft.insert(
            "switchState.menu".to_owned(),
            vec![chord(ControllerButton::Options, ControllerButton::FaceTop)],
        );
        let err = validate_binding_candidate(&draft, &single(ControllerButton::Options), None)
            .unwrap_err();
        assert_eq!(
            err,
            "conflict: options is a chord leader and cannot also be a single"
        );
    }

    #[test]
    fn rejects_chord_when_leader_has_single() {
        let mut draft = HashMap::new();
        draft.insert(
            "activate".to_owned(),
            vec![single(ControllerButton::Options)],
        );
        let err = validate_binding_candidate(
            &draft,
            &chord(ControllerButton::Options, ControllerButton::FaceTop),
            None,
        )
        .unwrap_err();
        assert_eq!(
            err,
            "conflict: options already has a single mapping and cannot be a chord leader"
        );
    }

    #[test]
    fn rejects_degenerate_chord() {
        let draft = HashMap::new();
        let err = validate_binding_candidate(
            &draft,
            &chord(ControllerButton::FaceTop, ControllerButton::FaceTop),
            None,
        )
        .unwrap_err();
        assert_eq!(err, "conflict: chord leader and follower must differ");
    }

    #[test]
    fn replace_ignores_old_pill_binding() {
        let mut draft = HashMap::new();
        let old = single(ControllerButton::FaceTop);
        draft.insert("toggleShift".to_owned(), vec![old.clone()]);
        validate_binding_candidate(&draft, &old, Some(&old)).unwrap();
        validate_binding_candidate(&draft, &single(ControllerButton::FaceBottom), Some(&old))
            .unwrap();
    }

    #[test]
    fn allows_follower_as_single_elsewhere() {
        let mut draft = HashMap::new();
        draft.insert(
            "switchState.menu".to_owned(),
            vec![chord(ControllerButton::Options, ControllerButton::FaceTop)],
        );
        validate_binding_candidate(&draft, &single(ControllerButton::FaceTop), None).unwrap();
    }

    #[test]
    fn on_return_value_adds_send_key_and_repaints() {
        let mut m = MappingsState::new();
        m.draft.insert(StateId::Keyboard, HashMap::new());
        m.pending_key_pick = Some(PendingKeyPick::AddBinding {
            binding: single(ControllerButton::Share),
        });
        let mut out = EventQueue::passthrough();
        m.on_return(ReturnStateResult::Value("enter".to_owned()), &mut out);
        let pills = m
            .draft
            .get(&StateId::Keyboard)
            .and_then(|d| d.get("sendKey.enter"))
            .cloned()
            .unwrap_or_default();
        assert_eq!(pills, vec![single(ControllerButton::Share)]);
        assert!(m.dirty);
        assert!(m.pending_key_pick.is_none());
        let drained = out.drain_pending();
        assert!(matches!(drained[0].0, Event::Repaint));
        assert_eq!(drained[0].1, EventSource::FollowUp);
    }

    #[test]
    fn on_return_cancelled_clears_pending_without_repaint() {
        let mut m = MappingsState::new();
        m.draft.insert(StateId::Keyboard, HashMap::new());
        m.pending_key_pick = Some(PendingKeyPick::AddBinding {
            binding: single(ControllerButton::Share),
        });
        let mut out = EventQueue::passthrough();
        m.on_return(ReturnStateResult::Cancelled, &mut out);
        assert!(m.pending_key_pick.is_none());
        assert!(!m.dirty);
        assert!(m
            .draft
            .get(&StateId::Keyboard)
            .map(|d| d.is_empty())
            .unwrap_or(true));
        assert!(out.drain_pending().is_empty());
    }

    #[test]
    fn on_return_rejects_invalid_key() {
        let mut m = MappingsState::new();
        m.draft.insert(StateId::Keyboard, HashMap::new());
        m.pending_key_pick = Some(PendingKeyPick::AddBinding {
            binding: single(ControllerButton::Share),
        });
        let mut out = EventQueue::passthrough();
        m.on_return(
            ReturnStateResult::Value("not-a-real-key".to_owned()),
            &mut out,
        );
        assert_eq!(m.status, "invalid key: not-a-real-key");
        assert!(!m.dirty);
        assert!(m.pending_key_pick.is_none());
        let drained = out.drain_pending();
        assert!(matches!(drained[0].0, Event::Repaint));
    }

    #[test]
    fn keyboard_catalog_orders_send_key_gateway_above_concrete() {
        let mut draft = HashMap::new();
        draft.insert(
            "sendKey.z".to_owned(),
            vec![single(ControllerButton::FaceTop)],
        );
        draft.insert(
            "sendKey.a".to_owned(),
            vec![single(ControllerButton::FaceBottom)],
        );
        let rows = catalog_rows(StateId::Keyboard, &draft);
        let gateway = rows
            .iter()
            .position(|r| r == SEND_KEY_GATEWAY)
            .expect("gateway");
        let send_a = rows.iter().position(|r| r == "sendKey.a").expect("sendKey.a");
        let send_z = rows.iter().position(|r| r == "sendKey.z").expect("sendKey.z");
        assert!(gateway < send_a);
        assert!(send_a < send_z);
        assert!(rows.iter().take(gateway).all(|r| !r.starts_with("sendKey.")));
    }
}
