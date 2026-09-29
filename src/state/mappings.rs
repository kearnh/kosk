//! In-app Key Mappings editor (controller-operable, action-centric).

use crate::config;
use crate::controller::bindings::StickDpad;
use crate::controller::mapping::{validate_rule_order, MappingPill, MappingRule, MappingValue};
use crate::controller::{ControllerBinding, ControllerButton, ControllerInput, ControllerKind};
use crate::state::actions::get_action;
use crate::state::event::{CallRequest, Event, EventQueue, EventSource, ReturnStateResult};
use crate::state::keyboard::display_icon::LabelCache;
use crate::state::keyboard::{self, KeyboardAction};
use crate::state::menu_action::MenuAction;
use crate::state::move_window_action::MoveWindowAction;
use crate::state::select_layout_action::SelectLayoutAction;
use crate::state::text_input_action::TextInputAction;
use crate::state::StateId;
use crate::ui::controller_glyph::{self, GlyphFamily};
use anyhow::Result;
use egui::{
    Align, Button, Color32, Context, Frame, Label, Margin, RichText, ScrollArea, Sense, Stroke, Ui,
    Vec2,
};
use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use std::sync::{Mutex, OnceLock};
use strum::{VariantArray, VariantNames};

// --- Hardcoded Mappings chrome (not user-remappable; edit here only) ---
const NAV_UP: ControllerButton = ControllerButton::DpadUp;
const NAV_DOWN: ControllerButton = ControllerButton::DpadDown;
const NAV_LEFT: ControllerButton = ControllerButton::DpadLeft;
const NAV_RIGHT: ControllerButton = ControllerButton::DpadRight;
const TAB_PREV: ControllerButton = ControllerButton::ShoulderLeft; // L1
const TAB_NEXT: ControllerButton = ControllerButton::ShoulderRight; // R1
const ACTIVATE: ControllerButton = ControllerButton::FaceBottom; // A
const DELETE_CONFIRM: ControllerButton = ControllerButton::FaceTop; // Y
const BACK_CANCEL: ControllerButton = ControllerButton::FaceRight; // B
const HINT_GLYPH: f32 = 16.0;
const HINT_GAP: f32 = 10.0;
// ----------------------------------------------------------------------

fn draw_mapping_hints(ui: &mut Ui, family: GlyphFamily) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        hint_group(ui, family, &[ACTIVATE], "enter table / edit");
        ui.add_space(HINT_GAP);
        hint_group(ui, family, &[BACK_CANCEL], "back out / Cancel");
        ui.add_space(HINT_GAP);
        hint_group(ui, family, &[DELETE_CONFIRM], "delete binding");
    });
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        hint_group(ui, family, &[TAB_PREV, TAB_NEXT], "mode tab");
        ui.add_space(HINT_GAP);
        hint_group(
            ui,
            family,
            &[NAV_UP, NAV_DOWN, NAV_LEFT, NAV_RIGHT],
            "move focus / row / pill",
        );
    });
}

fn hint_group(ui: &mut Ui, family: GlyphFamily, buttons: &[ControllerButton], label: &str) {
    for button in buttons {
        controller_glyph::show(ui, family, *button, HINT_GLYPH);
    }
    ui.label(label);
}

/// Keep bindings this version does not understand, so a save does not drop them.
fn retain_unrecognised_bindings(
    new_map: &mut HashMap<StateId, HashMap<ControllerBinding, MappingValue>>,
    live: &HashMap<StateId, HashMap<ControllerBinding, MappingValue>>,
) {
    for (mode, live_bindings) in live {
        let mode_new = new_map.entry(*mode).or_default();
        for (binding, value) in live_bindings {
            let Ok(rules) = value.rules() else {
                mode_new
                    .entry(binding.clone())
                    .or_insert_with(|| value.clone());
                continue;
            };
            let unknown: Vec<MappingRule> = rules
                .into_iter()
                .filter(|rule| get_action(*mode, &rule.action).is_none())
                .collect();
            if unknown.is_empty() {
                continue;
            }
            match mode_new.get_mut(binding) {
                None => {
                    mode_new.insert(binding.clone(), value.clone());
                }
                Some(existing) => {
                    let known = existing.rules().unwrap_or_default();
                    *existing = MappingValue::Rules(merge_rules_keeping_order(known, unknown));
                }
            }
        }
    }
}

/// `when` rules first, then at most one fallback; a known fallback beats an unknown one.
fn merge_rules_keeping_order(
    known: Vec<MappingRule>,
    unknown: Vec<MappingRule>,
) -> Vec<MappingRule> {
    let mut merged: Vec<MappingRule> = Vec::new();
    let mut fallback: Option<MappingRule> = None;
    for rule in known.into_iter().chain(unknown) {
        if rule.when.is_none() {
            fallback.get_or_insert(rule);
            continue;
        }
        if !merged.contains(&rule) {
            merged.push(rule);
        }
    }

    merged.extend(fallback);
    merged
}

const EDITABLE_MODES: [StateId; 5] = [
    StateId::Keyboard,
    StateId::Settings,
    StateId::SelectLayout,
    StateId::TextInput,
    StateId::MoveWindow,
];

const SEND_KEY_GATEWAY: &str = "sendKey";

type ModeDraft = HashMap<String, Vec<MappingPill>>;

const ACTION_COL_WIDTH: f32 = 200.0;
const BINDING_COL_GAP: f32 = 16.0;

fn row_focus_fill(theme: &crate::theme::MappingsTheme) -> Color32 {
    crate::theme::color(theme.row_focus_color)
}

fn table_focus_fill(theme: &crate::theme::MappingsTheme) -> Color32 {
    crate::theme::color(theme.table_focus_color)
}

fn table_focus_stroke(theme: &crate::theme::MappingsTheme) -> Stroke {
    Stroke::new(
        theme.focus_border_width,
        crate::theme::color(theme.focus_border_color),
    )
}

const UI_FONT_SIZE: f32 = 14.0;

fn status_icon_template(status: &str) -> String {
    if status == "ready" {
        "{icon:check} ready".to_owned()
    } else if status == "saved" {
        "{icon:check-circle} saved".to_owned()
    } else if status.starts_with("warning:")
        || status.starts_with("conflict:")
        || status.starts_with("invalid")
        || status.starts_with("save failed")
    {
        format!("{{icon:warning}} {status}")
    } else {
        status.to_owned()
    }
}

fn status_color(status: &str, theme: &crate::theme::MappingsTheme) -> Color32 {
    if status.starts_with("warning:") || status.starts_with("conflict:") {
        crate::theme::color(theme.warning_color)
    } else if status.starts_with("invalid") || status.starts_with("save failed") {
        crate::theme::color(theme.error_color)
    } else if status == "saved" {
        crate::theme::color(theme.success_color)
    } else {
        crate::theme::color(theme.text_color)
    }
}

/// Why SelectKey was opened; consumed in [`MappingsState::on_return`].
enum PendingSelect {
    Replace {
        action: String,
        pill: usize,
    },
    /// Target action comes back from SelectKey; prefill only guides the Action field.
    Add,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FocusZone {
    Tabs,
    Table,
    Cancel,
    Save,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DeleteModalFocus {
    Cancel,
    Confirm,
}

/// Shared conflict check used by `on_return` and Save.
/// `ignore` = binding currently on the pill being replaced (treated as absent).
fn validate_binding_candidate(
    draft_mode: &ModeDraft,
    candidate: &ControllerBinding,
    candidate_when: Option<&str>,
    ignore: Option<&MappingPill>,
) -> Result<(), String> {
    if let ControllerBinding::Chord { leader, follower } = candidate {
        if leader == follower {
            return Err("conflict: chord leader and follower must differ".to_owned());
        }
    }

    let mut occupied: HashMap<(ControllerBinding, Option<String>), String> = HashMap::new();
    let mut singles: HashSet<ControllerButton> = HashSet::new();
    let mut leaders: HashSet<ControllerButton> = HashSet::new();

    for (action, bindings) in draft_mode {
        for pill in bindings {
            if ignore.is_some_and(|ig| ig == pill) {
                continue;
            }
            occupied.insert((pill.binding.clone(), pill.when.clone()), action.clone());
            match &pill.binding {
                ControllerBinding::Single(b) => {
                    singles.insert(*b);
                }
                ControllerBinding::Chord { leader, .. } => {
                    leaders.insert(*leader);
                }
            }
        }
    }

    let cand_when = candidate_when.map(str::to_owned);
    if let Some(action) = occupied.get(&(candidate.clone(), cand_when)) {
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
    crate::state::actions::to_camel(pascal)
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

fn pill_count(action: &str, draft: &ModeDraft) -> usize {
    if action == SEND_KEY_GATEWAY {
        0
    } else {
        draft.get(action).map(|v| v.len()).unwrap_or(0)
    }
}

fn max_focus_col(n_pills: usize) -> usize {
    n_pills // index of [+]
}

fn unit_and_switch_rows(variants: &[&str]) -> Vec<String> {
    let mut rows = unit_names(variants, &["SwitchState"]);
    rows.extend(switch_state_catalog());
    rows.sort();
    rows
}

fn catalog_rows(mode: StateId, draft: &ModeDraft) -> Vec<String> {
    match mode {
        StateId::Keyboard => {
            let mut rows = unit_names(
                KeyboardAction::VARIANTS,
                &[
                    "SendKey",
                    "SendEnigoKey",
                    "SwitchState",
                    "SwitchLayout",
                    "AcceptSuggestion",
                ],
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
        StateId::Settings => unit_and_switch_rows(MenuAction::VARIANTS),
        StateId::SelectLayout => unit_and_switch_rows(SelectLayoutAction::VARIANTS),
        StateId::TextInput => {
            let mut rows = unit_names(
                TextInputAction::VARIANTS,
                &["SwitchState", "AcceptSuggestion"],
            );
            rows.extend(switch_state_catalog());
            rows.sort();
            rows
        }
        StateId::MoveWindow => unit_and_switch_rows(MoveWindowAction::VARIANTS),
        StateId::Mappings | StateId::SelectKey => Vec::new(),
    }
}

fn mode_label(mode: StateId) -> &'static str {
    match mode {
        StateId::Keyboard => "Keyboard",
        StateId::Settings => "Settings",
        StateId::SelectLayout => "SelectLayout",
        StateId::TextInput => "TextInput",
        StateId::MoveWindow => "MoveWindow",
        StateId::Mappings => "Mappings",
        StateId::SelectKey => "SelectKey",
    }
}

fn tab_index(mode: StateId) -> usize {
    EDITABLE_MODES.iter().position(|m| *m == mode).unwrap_or(0)
}

fn mapping_values_from_draft(
    draft: &ModeDraft,
) -> Result<HashMap<ControllerBinding, MappingValue>, String> {
    let mut grouped: HashMap<ControllerBinding, Vec<(Option<String>, String)>> = HashMap::new();
    for (action, pills) in draft {
        for pill in pills {
            grouped
                .entry(pill.binding.clone())
                .or_default()
                .push((pill.when.clone(), action.clone()));
        }
    }

    let mut out = HashMap::new();
    for (binding, mut rows) in grouped {
        rows.sort_by(|a, b| match (&a.0, &b.0) {
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (sa, sb) => sa.cmp(sb).then_with(|| a.1.cmp(&b.1)),
        });
        let rules: Vec<MappingRule> = rows
            .into_iter()
            .map(|(when, action)| MappingRule { action, when })
            .collect();
        validate_rule_order(&rules)?;
        let value = match rules.as_slice() {
            [one] if one.when.is_none() => MappingValue::Action(one.action.clone()),
            [one] => MappingValue::Rule(one.clone()),
            _ => MappingValue::Rules(rules),
        };
        out.insert(binding, value);
    }
    Ok(out)
}

fn held_set(input: &dyn ControllerInput) -> HashSet<ControllerButton> {
    ControllerButton::VARIANTS
        .iter()
        .filter(|b| input.query(**b))
        .copied()
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
    draft: HashMap<StateId, ModeDraft>,
    dirty: bool,
    status: String,
    tab: StateId,
    focus_zone: FocusZone,
    focus_row: usize,
    /// 0..n_pills = pill index; n_pills = trailing add (`+`) control.
    focus_col: usize,
    /// When `focus_zone == Table`, true means row/pill navigation is active.
    table_entered: bool,
    pending_select: Option<PendingSelect>,
    /// Fake delete modal payload: (action, pill). While set, input is modal-only.
    delete_confirm: Option<(String, usize)>,
    /// Which modal button is focused while `delete_confirm` is set.
    delete_modal_focus: DeleteModalFocus,
    prev_held: HashSet<ControllerButton>,
    stick_dpad: StickDpad,
    label_cache: LabelCache,
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
            table_entered: false,
            pending_select: None,
            delete_confirm: None,
            delete_modal_focus: DeleteModalFocus::Cancel,
            prev_held: HashSet::new(),
            stick_dpad: StickDpad::default(),
            label_cache: LabelCache::new(),
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
        self.table_entered = false;
        self.pending_select = None;
        self.delete_confirm = None;
        self.delete_modal_focus = DeleteModalFocus::Cancel;
        self.prev_held.clear();
    }

    fn load_draft_from_config(&mut self) {
        let cfg = config::get();
        let mut draft: HashMap<StateId, ModeDraft> = HashMap::new();
        for mode in EDITABLE_MODES {
            draft.insert(mode, HashMap::new());
        }
        let mut unknown = 0usize;
        for mode in EDITABLE_MODES {
            let Some(raw) = cfg.controller_map.get(&mode) else {
                continue;
            };
            let mode_draft = draft.get_mut(&mode).expect("mode present");
            for (binding, value) in raw {
                let rules = match value.rules() {
                    Ok(r) => r,
                    Err(_) => {
                        unknown += 1;
                        continue;
                    }
                };
                for rule in rules {
                    if get_action(mode, &rule.action).is_none() {
                        unknown += 1;
                        continue;
                    }
                    mode_draft
                        .entry(rule.action)
                        .or_default()
                        .push(MappingPill {
                            binding: binding.clone(),
                            when: rule.when,
                        });
                }
            }
        }
        self.draft = draft;
        if unknown > 0 {
            self.status = format!("warning: {unknown} unknown actions ignored");
        } else {
            self.status = "ready".to_owned();
        }
    }

    fn current_draft(&self) -> &ModeDraft {
        self.draft.get(&self.tab).expect("editable tab draft")
    }

    fn current_draft_mut(&mut self) -> &mut ModeDraft {
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
        let Some(pending) = self.pending_select.take() else {
            return;
        };
        let ReturnStateResult::SelectKey { binding, action } = result else {
            return; // Cancelled; draft unchanged — no Repaint required
        };
        let candidate = match ControllerBinding::from_str(&binding) {
            Ok(candidate) => candidate,
            Err(err) => {
                self.status = err;
                let _ = out.push(Event::Repaint, &EventSource::FollowUp);
                return;
            }
        };
        if get_action(self.tab, &action).is_none() {
            self.status = format!("invalid action: {action}");
            let _ = out.push(Event::Repaint, &EventSource::FollowUp);
            return;
        }

        match pending {
            PendingSelect::Replace {
                action: old_action,
                pill,
            } => {
                let ignore = self
                    .current_draft()
                    .get(&old_action)
                    .and_then(|v| v.get(pill))
                    .cloned();
                let keep_when = ignore.as_ref().and_then(|p| p.when.clone());
                if let Err(err) = validate_binding_candidate(
                    self.current_draft(),
                    &candidate,
                    keep_when.as_deref(),
                    ignore.as_ref(),
                ) {
                    self.status = err;
                    let _ = out.push(Event::Repaint, &EventSource::FollowUp);
                    return;
                }
                let new_pill = MappingPill {
                    binding: candidate,
                    when: keep_when,
                };
                if action == old_action {
                    if let Some(vec) = self.current_draft_mut().get_mut(&old_action) {
                        if pill < vec.len() {
                            vec[pill] = new_pill;
                            self.dirty = true;
                            self.status = "ready".to_owned();
                        }
                    }
                    self.focus_action_pill(&old_action, pill);
                } else {
                    let draft = self.current_draft_mut();
                    if let Some(vec) = draft.get_mut(&old_action) {
                        if pill < vec.len() {
                            vec.remove(pill);
                            if vec.is_empty() {
                                draft.remove(&old_action);
                            }
                        }
                    }
                    draft.entry(action.clone()).or_default().push(new_pill);
                    self.dirty = true;
                    self.status = "ready".to_owned();
                    let last = self
                        .current_draft()
                        .get(&action)
                        .map(|v| v.len())
                        .unwrap_or(1)
                        .saturating_sub(1);
                    self.focus_action_pill(&action, last);
                }
            }
            PendingSelect::Add => {
                if let Err(err) =
                    validate_binding_candidate(self.current_draft(), &candidate, None, None)
                {
                    self.status = err;
                    let _ = out.push(Event::Repaint, &EventSource::FollowUp);
                    return;
                }
                self.current_draft_mut()
                    .entry(action.clone())
                    .or_default()
                    .push(MappingPill::always(candidate));
                self.dirty = true;
                self.status = "ready".to_owned();
                let last = self
                    .current_draft()
                    .get(&action)
                    .map(|v| v.len())
                    .unwrap_or(1)
                    .saturating_sub(1);
                self.focus_action_pill(&action, last);
            }
        }
        let _ = out.push(Event::Repaint, &EventSource::FollowUp);
    }

    fn focus_action_pill(&mut self, action: &str, pill: usize) {
        let rows = self.rows();
        if let Some(idx) = rows.iter().position(|r| r == action) {
            self.focus_row = idx;
            self.focus_col = pill;
            self.focus_zone = FocusZone::Table;
            self.table_entered = true;
        }
    }

    fn push_select_key_call(
        &self,
        binding: String,
        action: String,
        editing: Option<MappingPill>,
        events: &mut EventQueue,
        source: &EventSource,
    ) {
        let _ = events.push(
            Event::CallState(CallRequest::SelectKey {
                binding,
                action,
                mode: self.tab,
                draft_mode: self.current_draft().clone(),
                editing,
            }),
            source,
        );
    }

    /// Replace flow: A (or click) on an existing pill.
    fn open_select_key(
        &mut self,
        pending: PendingSelect,
        events: &mut EventQueue,
        source: &EventSource,
    ) {
        let (action, pill) = match &pending {
            PendingSelect::Replace { action, pill } => (action.clone(), *pill),
            PendingSelect::Add => return,
        };
        let pill = self
            .current_draft()
            .get(&action)
            .and_then(|v| v.get(pill))
            .cloned();
        let binding = pill
            .as_ref()
            .map(|p| p.binding.to_string())
            .unwrap_or_default();
        self.pending_select = Some(pending);
        self.push_select_key_call(binding, action, pill, events, source);
    }

    /// Add flow: A (or click) on a row's `+`. `row_action` guides the Action
    /// prefill only; the returned action comes from SelectKey.
    fn open_add_for_row(
        &mut self,
        row_action: String,
        events: &mut EventQueue,
        source: &EventSource,
    ) {
        let action = if row_action == SEND_KEY_GATEWAY {
            format!("{SEND_KEY_GATEWAY}.")
        } else {
            row_action
        };
        self.pending_select = Some(PendingSelect::Add);
        self.push_select_key_call(String::new(), action, None, events, source);
    }

    fn set_tab(&mut self, mode: StateId) {
        self.tab = mode;
        self.focus_row = 0;
        self.focus_col = 0;
        self.focus_zone = FocusZone::Tabs;
        self.table_entered = false;
        self.delete_confirm = None;
        self.clamp_focus();
    }

    fn delete_pill(&mut self, action: &str, pill: usize) {
        let draft = self.current_draft_mut();
        let Some(vec) = draft.get_mut(action) else {
            return;
        };
        if pill >= vec.len() {
            return;
        }
        vec.remove(pill);
        if vec.is_empty() {
            draft.remove(action);
        }
        self.dirty = true;
        self.status = "ready".to_owned();
        self.clamp_focus();
    }

    /// Rising Y on a real pill opens the fake delete modal; elsewhere a no-op.
    fn request_delete_focused(&mut self) {
        if self.focus_zone != FocusZone::Table || !self.table_entered {
            return;
        }
        let rows = self.rows();
        if self.focus_row >= rows.len() {
            return;
        }
        let action = rows[self.focus_row].clone();
        let n_pills = pill_count(&action, self.current_draft());
        if action == SEND_KEY_GATEWAY || self.focus_col >= n_pills {
            return; // on [+] or gateway
        }
        self.delete_confirm = Some((action, self.focus_col));
        self.delete_modal_focus = DeleteModalFocus::Cancel;
    }

    fn activate_focused(&mut self, events: &mut EventQueue, source: &EventSource) {
        match self.focus_zone {
            FocusZone::Tabs => {}
            FocusZone::Cancel => self.do_cancel(events, source),
            FocusZone::Save => self.do_save(),
            FocusZone::Table => {
                if !self.table_entered {
                    self.table_entered = true;
                    return;
                }
                let rows = self.rows();
                if self.focus_row >= rows.len() {
                    return;
                }
                let action = rows[self.focus_row].clone();
                let n_pills = pill_count(&action, self.current_draft());
                if self.focus_col == n_pills {
                    self.open_add_for_row(action, events, source);
                } else {
                    let pill = self.focus_col;
                    self.open_select_key(PendingSelect::Replace { action, pill }, events, source);
                }
            }
        }
    }

    fn do_cancel(&mut self, events: &mut EventQueue, source: &EventSource) {
        self.pending_select = None;
        self.delete_confirm = None;
        let _ = events.push(Event::ChangeState(StateId::Settings), source);
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
                for (i, pill) in bindings.iter().enumerate() {
                    let mut temp = draft_mode.clone();
                    if let Some(vec) = temp.get_mut(action) {
                        if i < vec.len() {
                            vec.remove(i);
                        }
                    }
                    if let Err(err) =
                        validate_binding_candidate(&temp, &pill.binding, pill.when.as_deref(), None)
                    {
                        self.status = err;
                        self.tab = mode;
                        return;
                    }
                }
            }
        }

        let mut new_map: HashMap<StateId, HashMap<ControllerBinding, MappingValue>> =
            HashMap::new();
        for mode in EDITABLE_MODES {
            let inverted = match self.draft.get(&mode) {
                Some(draft_mode) => match mapping_values_from_draft(draft_mode) {
                    Ok(m) => m,
                    Err(err) => {
                        self.status = err;
                        self.tab = mode;
                        return;
                    }
                },
                None => HashMap::new(),
            };
            new_map.insert(mode, inverted);
        }

        let mut cfg = config::get();
        retain_unrecognised_bindings(&mut new_map, &cfg.controller_map);
        cfg.controller_map = new_map;
        if let Err(e) = config::save(cfg) {
            self.status = format!("save failed: {e:#}");
            return;
        }
        config::notify_changed();
        self.dirty = false;
        self.status = "saved".to_owned();
    }

    fn handle_browse_input(&mut self, input: &dyn ControllerInput, events: &mut EventQueue) {
        let extra = self.stick_dpad.update(input.left_stick_raw());
        let mut held = held_set(input);
        if let Some(dir) = extra {
            held.insert(dir.to_button());
        }
        let prev = self.prev_held.clone();

        if let Some((action, pill)) = self.delete_confirm.clone() {
            if rising(BACK_CANCEL, &held, &prev) {
                self.delete_confirm = None;
                self.delete_modal_focus = DeleteModalFocus::Cancel;
            } else if rising(ACTIVATE, &held, &prev) {
                match self.delete_modal_focus {
                    DeleteModalFocus::Confirm => {
                        self.delete_pill(&action, pill);
                        self.delete_confirm = None;
                        self.delete_modal_focus = DeleteModalFocus::Cancel;
                    }
                    DeleteModalFocus::Cancel => {
                        self.delete_confirm = None;
                    }
                }
            } else if rising(NAV_LEFT, &held, &prev) || rising(NAV_RIGHT, &held, &prev) {
                self.delete_modal_focus = match self.delete_modal_focus {
                    DeleteModalFocus::Cancel => DeleteModalFocus::Confirm,
                    DeleteModalFocus::Confirm => DeleteModalFocus::Cancel,
                };
            }
            self.prev_held = held;
            return;
        }

        if rising(BACK_CANCEL, &held, &prev) {
            if self.focus_zone == FocusZone::Table && self.table_entered {
                self.table_entered = false;
            } else {
                self.do_cancel(events, &source_for(BACK_CANCEL));
            }
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
        if rising(DELETE_CONFIRM, &held, &prev) {
            self.request_delete_focused();
            self.prev_held = held;
            return;
        }
        if rising(ACTIVATE, &held, &prev) {
            self.activate_focused(events, &source_for(ACTIVATE));
            self.prev_held = held;
            return;
        }

        if rising(NAV_UP, &held, &prev) {
            match self.focus_zone {
                FocusZone::Tabs => {}
                FocusZone::Table => {
                    if self.table_entered {
                        if self.focus_row > 0 {
                            self.focus_row -= 1;
                            self.focus_col = 0;
                            self.clamp_focus();
                        }
                    } else {
                        self.focus_zone = FocusZone::Tabs;
                    }
                }
                FocusZone::Cancel => {
                    self.focus_zone = FocusZone::Table;
                    self.table_entered = false;
                }
                FocusZone::Save => {
                    self.focus_zone = FocusZone::Cancel;
                }
            }
        } else if rising(NAV_DOWN, &held, &prev) {
            match self.focus_zone {
                FocusZone::Tabs => {
                    self.focus_zone = FocusZone::Table;
                    self.table_entered = false;
                }
                FocusZone::Table => {
                    if self.table_entered {
                        let n = self.rows().len();
                        if n > 0 && self.focus_row + 1 < n {
                            self.focus_row += 1;
                            self.focus_col = 0;
                            self.clamp_focus();
                        }
                    } else {
                        self.focus_zone = FocusZone::Cancel;
                    }
                }
                FocusZone::Cancel => {
                    self.focus_zone = FocusZone::Save;
                }
                FocusZone::Save => {}
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
                    if self.table_entered && self.focus_col > 0 {
                        self.focus_col -= 1;
                        self.clamp_focus();
                    }
                }
                FocusZone::Cancel => {}
                FocusZone::Save => {
                    self.focus_zone = FocusZone::Cancel;
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
                    if self.table_entered {
                        self.focus_col += 1;
                        self.clamp_focus();
                    }
                }
                FocusZone::Cancel => {
                    self.focus_zone = FocusZone::Save;
                }
                FocusZone::Save => {}
            }
        }

        self.prev_held = held;
    }

    pub fn draw_ui(
        &mut self,
        ctx: &Context,
        ui: &mut Ui,
        events: &mut EventQueue,
        kind: ControllerKind,
    ) {
        let cfg = config::get();
        let appearance = &cfg.theme().mappings;
        let text_color = crate::theme::color(appearance.text_color);
        ui.horizontal(|ui| {
            ui.heading(RichText::new("Key Mappings").color(text_color));
            if self.dirty {
                let dirty = self.label_cache.get(
                    "{icon:circle:fill} unsaved",
                    UI_FONT_SIZE,
                    crate::theme::color(appearance.unsaved_color),
                );
                ui.label(dirty);
            }
        });

        ui.horizontal(|ui| {
            for mode in EDITABLE_MODES {
                let selected = self.tab == mode;
                if ui
                    .add(Button::new(mode_label(mode)).selected(selected))
                    .clicked()
                {
                    self.set_tab(mode);
                    self.focus_zone = FocusZone::Tabs;
                }
            }
        });

        if self.tab == StateId::TextInput {
            ui.label(
                RichText::new("Unmapped inputs inherit Keyboard bindings.")
                    .italics()
                    .weak(),
            );
        }

        ui.separator();

        let rows = self.rows();
        let draft_snapshot = self.current_draft().clone();
        let focus_zone = self.focus_zone;
        let table_entered = self.table_entered;
        let focus_row = self.focus_row;
        let focus_col = self.focus_col;
        let plus_label = self
            .label_cache
            .get("{icon:plus}", UI_FONT_SIZE, text_color);
        let cancel_label = self
            .label_cache
            .get("{icon:x} Cancel", UI_FONT_SIZE, text_color);
        let save_label = self
            .label_cache
            .get("{icon:floppy-disk} Save", UI_FONT_SIZE, text_color);

        let mut clicked_replace: Option<(String, usize)> = None;
        let mut clicked_add: Option<String> = None;
        let mut clicked_focus: Option<(usize, usize)> = None;

        ui.add_enabled_ui(self.delete_confirm.is_none(), |ui| {
            // Always reserve stroke width so content does not jump when focus changes.
            let stroke = if focus_zone == FocusZone::Table {
                table_focus_stroke(appearance)
            } else {
                Stroke::new(table_focus_stroke(appearance).width, Color32::TRANSPARENT)
            };
            let fill = if focus_zone == FocusZone::Table && !table_entered {
                table_focus_fill(appearance)
            } else {
                Color32::TRANSPARENT
            };
            Frame::NONE
                .fill(fill)
                .stroke(stroke)
                .inner_margin(Margin::same(4))
                .show(ui, |ui| {
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
                                let row_focused = focus_zone == FocusZone::Table
                                    && table_entered
                                    && focus_row == row_idx;
                                let n_pills = pills.len();
                                let row_fill = if row_focused {
                                    row_focus_fill(appearance)
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

                                            for (pill_idx, pill) in pills.iter().enumerate() {
                                                let col = pill_idx;
                                                let selected = row_focused && focus_col == col;
                                                let resp = ui.add(
                                                    Button::new(format!("[{}]", pill.label()))
                                                        .selected(selected),
                                                );
                                                if resp.clicked() {
                                                    clicked_focus = Some((row_idx, col));
                                                    clicked_replace =
                                                        Some((action.clone(), pill_idx));
                                                }
                                            }

                                            let plus_col = n_pills;
                                            let plus_selected =
                                                row_focused && focus_col == plus_col;
                                            let plus_resp = ui.add(
                                                Button::new(plus_label.clone())
                                                    .selected(plus_selected),
                                            );
                                            if plus_resp.clicked() {
                                                clicked_focus = Some((row_idx, plus_col));
                                                clicked_add = Some(action.clone());
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
        });

        if let Some((row, col)) = clicked_focus {
            self.focus_zone = FocusZone::Table;
            self.table_entered = true;
            self.focus_row = row;
            self.focus_col = col;
            self.clamp_focus();
        }
        if let Some((action, pill)) = clicked_replace {
            self.open_select_key(
                PendingSelect::Replace { action, pill },
                events,
                &EventSource::MouseClick,
            );
        }
        if let Some(action) = clicked_add {
            self.open_add_for_row(action, events, &EventSource::MouseClick);
        }

        ui.separator();

        let status = self.status.clone();
        let status_label = self.label_cache.get(
            &status_icon_template(&status),
            UI_FONT_SIZE,
            status_color(&status, appearance),
        );
        ui.label(status_label);
        ui.horizontal(|ui| {
            let cancel_sel = self.focus_zone == FocusZone::Cancel;
            if ui
                .add(Button::new(cancel_label).selected(cancel_sel))
                .clicked()
            {
                self.focus_zone = FocusZone::Cancel;
                self.do_cancel(events, &EventSource::MouseClick);
            }
            let save_sel = self.focus_zone == FocusZone::Save;
            if ui.add(Button::new(save_label).selected(save_sel)).clicked() {
                self.focus_zone = FocusZone::Save;
                self.do_save();
            }
        });
        draw_mapping_hints(ui, GlyphFamily::from_kind(kind));

        if let Some((action, pill)) = self.delete_confirm.clone() {
            let binding_text = self
                .current_draft()
                .get(&action)
                .and_then(|v| v.get(pill))
                .map(|p| format!("[{}]", p.label()))
                .unwrap_or_else(|| "[?]".to_owned());
            let mut confirm = false;
            let mut cancel = false;
            egui::Area::new(egui::Id::new("delete_confirm_modal"))
                // Slightly above center so the dialog sits over the table, not the footer.
                .anchor(egui::Align2::CENTER_CENTER, Vec2::new(0.0, -72.0))
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    // Overlay visuals force window_fill transparent; override for readability.
                    Frame::window(ui.style())
                        .fill(crate::theme::color(appearance.editor_background_color))
                        .stroke(Stroke::new(
                            appearance.editor_border_width,
                            crate::theme::color(appearance.editor_border_color),
                        ))
                        .corner_radius(appearance.editor_corner_radius)
                        .show(ui, |ui| {
                            ui.label(format!("Delete {binding_text} from {action}?"));
                            ui.horizontal(|ui| {
                                let cancel_sel =
                                    self.delete_modal_focus == DeleteModalFocus::Cancel;
                                if ui.add(Button::new("Cancel").selected(cancel_sel)).clicked() {
                                    cancel = true;
                                }
                                let confirm_sel =
                                    self.delete_modal_focus == DeleteModalFocus::Confirm;
                                if ui
                                    .add(Button::new("Confirm").selected(confirm_sel))
                                    .clicked()
                                {
                                    confirm = true;
                                }
                            });
                        });
                });
            if confirm {
                self.delete_pill(&action, pill);
                self.delete_confirm = None;
                self.delete_modal_focus = DeleteModalFocus::Cancel;
            } else if cancel {
                self.delete_confirm = None;
                self.delete_modal_focus = DeleteModalFocus::Cancel;
            }
        }
    }

    pub fn reset_controller_input(&mut self, holdover: Option<&dyn ControllerInput>) {
        self.stick_dpad.reset();
        match holdover {
            None => self.prev_held.clear(),
            Some(input) => {
                let extra = self.stick_dpad.update(input.left_stick_raw());
                let mut held = held_set(input);
                if let Some(dir) = extra {
                    held.insert(dir.to_button());
                }
                self.prev_held = held;
            }
        }
    }

    pub fn handle_controller_input(
        &mut self,
        _: &Context,
        input: &dyn ControllerInput,
        events: &mut EventQueue,
    ) -> Result<()> {
        self.handle_browse_input(input, events);
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
    fn save_keeps_an_action_this_version_does_not_know() {
        let binding = single(ControllerButton::FaceTop);
        let mut live_mode = HashMap::new();
        live_mode.insert(binding.clone(), MappingValue::from_action("fromTheFuture"));
        let mut live = HashMap::new();
        live.insert(StateId::Keyboard, live_mode);
        let mut new_map = HashMap::new();
        new_map.insert(StateId::Keyboard, HashMap::new());
        retain_unrecognised_bindings(&mut new_map, &live);
        let kept = &new_map[&StateId::Keyboard][&binding];
        assert_eq!(kept.rules().unwrap()[0].action, "fromTheFuture");
    }

    #[test]
    fn save_keeps_unknown_when_rule_before_fallback() {
        let binding = single(ControllerButton::FaceTop);
        let unknown = MappingRule {
            action: "fromTheFuture".to_owned(),
            when: Some("shiftActive".to_owned()),
        };
        let fallback = MappingRule {
            action: "toggleShift".to_owned(),
            when: None,
        };
        let mut live_mode = HashMap::new();
        live_mode.insert(
            binding.clone(),
            MappingValue::Rules(vec![unknown.clone(), fallback.clone()]),
        );
        let mut live = HashMap::new();
        live.insert(StateId::Keyboard, live_mode);
        let mut new_mode = HashMap::new();
        new_mode.insert(binding.clone(), MappingValue::Rules(vec![fallback.clone()]));
        let mut new_map = HashMap::new();
        new_map.insert(StateId::Keyboard, new_mode);

        retain_unrecognised_bindings(&mut new_map, &live);

        let kept = new_map[&StateId::Keyboard][&binding].rules().unwrap();
        assert_eq!(kept, vec![unknown, fallback]);
    }

    fn pill(b: ControllerBinding) -> MappingPill {
        MappingPill::always(b)
    }

    #[test]
    fn rejects_duplicate_binding() {
        let mut draft = HashMap::new();
        draft.insert(
            "toggleShift".to_owned(),
            vec![pill(single(ControllerButton::FaceTop))],
        );
        let err =
            validate_binding_candidate(&draft, &single(ControllerButton::FaceTop), None, None)
                .unwrap_err();
        assert_eq!(err, "conflict: faceTop already bound to toggleShift");
    }

    #[test]
    fn rejects_single_on_chord_leader() {
        let mut draft = HashMap::new();
        draft.insert(
            "switchState.settings".to_owned(),
            vec![pill(chord(
                ControllerButton::Options,
                ControllerButton::FaceTop,
            ))],
        );
        let err =
            validate_binding_candidate(&draft, &single(ControllerButton::Options), None, None)
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
            vec![pill(single(ControllerButton::Options))],
        );
        let err = validate_binding_candidate(
            &draft,
            &chord(ControllerButton::Options, ControllerButton::FaceTop),
            None,
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
            None,
        )
        .unwrap_err();
        assert_eq!(err, "conflict: chord leader and follower must differ");
    }

    #[test]
    fn replace_ignores_old_pill_binding() {
        let mut draft = HashMap::new();
        let old = pill(single(ControllerButton::FaceTop));
        draft.insert("toggleShift".to_owned(), vec![old.clone()]);
        validate_binding_candidate(&draft, &old.binding, None, Some(&old)).unwrap();
        validate_binding_candidate(
            &draft,
            &single(ControllerButton::FaceBottom),
            None,
            Some(&old),
        )
        .unwrap();
    }

    #[test]
    fn allows_follower_as_single_elsewhere() {
        let mut draft = HashMap::new();
        draft.insert(
            "switchState.settings".to_owned(),
            vec![pill(chord(
                ControllerButton::Options,
                ControllerButton::FaceTop,
            ))],
        );
        validate_binding_candidate(&draft, &single(ControllerButton::FaceTop), None, None).unwrap();
    }

    #[test]
    fn allows_same_button_with_different_when() {
        let mut draft = HashMap::new();
        draft.insert(
            "acceptSuggestion".to_owned(),
            vec![MappingPill {
                binding: single(ControllerButton::FaceBottom),
                when: Some("suggestionSelected".to_owned()),
            }],
        );
        validate_binding_candidate(&draft, &single(ControllerButton::FaceBottom), None, None)
            .unwrap();
    }

    #[test]
    fn on_return_add_appends_pill_and_repaints() {
        let mut m = MappingsState::new();
        m.draft.insert(StateId::Keyboard, HashMap::new());
        m.pending_select = Some(PendingSelect::Add);
        let mut out = EventQueue::passthrough();
        m.on_return(
            ReturnStateResult::SelectKey {
                binding: "share".to_owned(),
                action: "sendKey.enter".to_owned(),
            },
            &mut out,
        );
        let pills = m
            .draft
            .get(&StateId::Keyboard)
            .and_then(|d| d.get("sendKey.enter"))
            .cloned()
            .unwrap_or_default();
        assert_eq!(pills, vec![pill(single(ControllerButton::Share))]);
        assert!(m.dirty);
        assert!(m.pending_select.is_none());
        let drained = out.drain_pending();
        assert!(matches!(drained[0].0, Event::Repaint));
        assert_eq!(drained[0].1, EventSource::FollowUp);
    }

    #[test]
    fn on_return_cancelled_clears_pending_without_repaint() {
        let mut m = MappingsState::new();
        m.draft.insert(StateId::Keyboard, HashMap::new());
        m.pending_select = Some(PendingSelect::Add);
        let mut out = EventQueue::passthrough();
        m.on_return(ReturnStateResult::Cancelled, &mut out);
        assert!(m.pending_select.is_none());
        assert!(!m.dirty);
        assert!(m
            .draft
            .get(&StateId::Keyboard)
            .map(|d| d.is_empty())
            .unwrap_or(true));
        assert!(out.drain_pending().is_empty());
    }

    #[test]
    fn on_return_rejects_invalid_action() {
        let mut m = MappingsState::new();
        m.draft.insert(StateId::Keyboard, HashMap::new());
        m.pending_select = Some(PendingSelect::Add);
        let mut out = EventQueue::passthrough();
        m.on_return(
            ReturnStateResult::SelectKey {
                binding: "share".to_owned(),
                action: "notAnAction".to_owned(),
            },
            &mut out,
        );
        assert_eq!(m.status, "invalid action: notAnAction");
        assert!(!m.dirty);
        assert!(m.pending_select.is_none());
        let drained = out.drain_pending();
        assert!(matches!(drained[0].0, Event::Repaint));
    }

    #[test]
    fn on_return_rejects_unparseable_binding() {
        let mut m = MappingsState::new();
        m.draft.insert(StateId::Keyboard, HashMap::new());
        m.pending_select = Some(PendingSelect::Add);
        let mut out = EventQueue::passthrough();
        m.on_return(
            ReturnStateResult::SelectKey {
                binding: "not-a-button".to_owned(),
                action: "toggleShift".to_owned(),
            },
            &mut out,
        );
        assert!(m.status != "ready" && !m.status.is_empty());
        assert!(!m.dirty);
        assert!(m.pending_select.is_none());
        let drained = out.drain_pending();
        assert!(matches!(drained[0].0, Event::Repaint));
    }

    #[test]
    fn on_return_replace_same_action_updates_pill() {
        let mut m = MappingsState::new();
        m.draft.insert(
            StateId::Keyboard,
            HashMap::from([(
                "toggleShift".to_owned(),
                vec![pill(single(ControllerButton::FaceTop))],
            )]),
        );
        m.pending_select = Some(PendingSelect::Replace {
            action: "toggleShift".to_owned(),
            pill: 0,
        });
        let mut out = EventQueue::passthrough();
        m.on_return(
            ReturnStateResult::SelectKey {
                binding: "faceLeft".to_owned(),
                action: "toggleShift".to_owned(),
            },
            &mut out,
        );
        let pills = m
            .draft
            .get(&StateId::Keyboard)
            .and_then(|d| d.get("toggleShift"))
            .cloned()
            .unwrap_or_default();
        assert_eq!(pills, vec![pill(single(ControllerButton::FaceLeft))]);
        assert!(m.dirty);
        assert_eq!(m.status, "ready");
        let rows = m.rows();
        assert_eq!(rows[m.focus_row], "toggleShift");
        assert_eq!(m.focus_col, 0);
    }

    #[test]
    fn on_return_replace_other_action_moves_pill() {
        let mut m = MappingsState::new();
        m.draft.insert(
            StateId::Keyboard,
            HashMap::from([
                (
                    "toggleShift".to_owned(),
                    vec![pill(single(ControllerButton::FaceTop))],
                ),
                (
                    "toggleCtrl".to_owned(),
                    vec![pill(single(ControllerButton::FaceBottom))],
                ),
            ]),
        );
        m.pending_select = Some(PendingSelect::Replace {
            action: "toggleShift".to_owned(),
            pill: 0,
        });
        let mut out = EventQueue::passthrough();
        m.on_return(
            ReturnStateResult::SelectKey {
                binding: "faceTop".to_owned(),
                action: "toggleCtrl".to_owned(),
            },
            &mut out,
        );
        let draft = m.draft.get(&StateId::Keyboard).expect("keyboard draft");
        assert!(!draft.contains_key("toggleShift"), "empty action dropped");
        assert_eq!(
            draft.get("toggleCtrl").cloned().unwrap_or_default(),
            vec![
                pill(single(ControllerButton::FaceBottom)),
                pill(single(ControllerButton::FaceTop))
            ]
        );
        assert!(m.dirty);
        let rows = m.rows();
        assert_eq!(rows[m.focus_row], "toggleCtrl");
        assert_eq!(m.focus_col, 1, "focused on appended last pill");
    }

    #[test]
    fn on_return_replace_move_conflict_leaves_draft_unchanged() {
        let mut m = MappingsState::new();
        m.draft.insert(
            StateId::Keyboard,
            HashMap::from([
                (
                    "toggleShift".to_owned(),
                    vec![pill(single(ControllerButton::FaceTop))],
                ),
                (
                    "toggleCtrl".to_owned(),
                    vec![pill(single(ControllerButton::FaceLeft))],
                ),
            ]),
        );
        m.pending_select = Some(PendingSelect::Replace {
            action: "toggleShift".to_owned(),
            pill: 0,
        });
        let mut out = EventQueue::passthrough();
        m.on_return(
            ReturnStateResult::SelectKey {
                binding: "faceLeft".to_owned(),
                action: "toggleCtrl".to_owned(),
            },
            &mut out,
        );
        assert!(m.status.starts_with("conflict:"));
        assert!(!m.dirty);
        let draft = m.draft.get(&StateId::Keyboard).expect("keyboard draft");
        assert_eq!(
            draft.get("toggleShift").cloned().unwrap_or_default(),
            vec![pill(single(ControllerButton::FaceTop))]
        );
        assert_eq!(
            draft.get("toggleCtrl").cloned().unwrap_or_default(),
            vec![pill(single(ControllerButton::FaceLeft))]
        );
    }

    #[test]
    fn on_return_add_conflict_leaves_draft_unchanged() {
        let mut m = MappingsState::new();
        m.draft.insert(
            StateId::Keyboard,
            HashMap::from([(
                "toggleShift".to_owned(),
                vec![pill(single(ControllerButton::FaceTop))],
            )]),
        );
        m.pending_select = Some(PendingSelect::Add);
        let mut out = EventQueue::passthrough();
        m.on_return(
            ReturnStateResult::SelectKey {
                binding: "faceTop".to_owned(),
                action: "toggleCtrl".to_owned(),
            },
            &mut out,
        );
        assert!(m.status.starts_with("conflict:"));
        assert!(!m.dirty);
        assert!(!m
            .draft
            .get(&StateId::Keyboard)
            .expect("keyboard draft")
            .contains_key("toggleCtrl"));
    }

    #[test]
    fn keyboard_catalog_orders_send_key_gateway_above_concrete() {
        let mut draft = HashMap::new();
        draft.insert(
            "sendKey.z".to_owned(),
            vec![pill(single(ControllerButton::FaceTop))],
        );
        draft.insert(
            "sendKey.a".to_owned(),
            vec![pill(single(ControllerButton::FaceBottom))],
        );
        let rows = catalog_rows(StateId::Keyboard, &draft);
        let gateway = rows
            .iter()
            .position(|r| r == SEND_KEY_GATEWAY)
            .expect("gateway");
        let send_a = rows
            .iter()
            .position(|r| r == "sendKey.a")
            .expect("sendKey.a");
        let send_z = rows
            .iter()
            .position(|r| r == "sendKey.z")
            .expect("sendKey.z");
        assert!(gateway < send_a);
        assert!(send_a < send_z);
        assert!(rows
            .iter()
            .take(gateway)
            .all(|r| !r.starts_with("sendKey.")));
    }

    use crate::controller::test_input::{AnalogInput, ButtonSetInput};

    /// Mirror `begin_session` focus/edge baseline without `config::get()`.
    fn enter_browse_focus(m: &mut MappingsState) {
        m.draft.insert(StateId::Keyboard, HashMap::new());
        m.dirty = false;
        m.status = "ready".to_owned();
        m.tab = StateId::Keyboard;
        m.focus_zone = FocusZone::Table;
        m.focus_row = 0;
        m.focus_col = 0;
        m.table_entered = false;
        m.pending_select = None;
        m.delete_confirm = None;
        m.prev_held.clear();
    }

    fn enter_table_cells(m: &mut MappingsState) {
        enter_browse_focus(m);
        m.table_entered = true;
    }

    fn gateway_row(m: &MappingsState) -> usize {
        m.rows()
            .iter()
            .position(|r| r == SEND_KEY_GATEWAY)
            .expect("gateway row")
    }

    #[test]
    fn holdover_missing_after_enter_activates_plus() {
        let mut m = MappingsState::new();
        enter_browse_focus(&mut m);
        m.focus_row = gateway_row(&m);
        m.focus_col = 0; // gateway has no pills; col 0 is [+]
        let held = ButtonSetInput(HashSet::from([ControllerButton::FaceBottom]));
        let mut events = EventQueue::passthrough();
        // Outer Table absorbs rising A as drill-in; no SelectKey until cells entered.
        m.handle_browse_input(&held, &mut events);
        let drained = events.drain_pending();
        assert!(
            drained.is_empty(),
            "outer Table rising A must not push CallState"
        );
        assert!(m.table_entered, "outer rising A enters the table");
    }

    #[test]
    fn holdover_reset_after_enter_suppresses_activate() {
        let mut m = MappingsState::new();
        enter_browse_focus(&mut m);
        m.focus_row = gateway_row(&m);
        m.focus_col = 0;
        let held = ButtonSetInput(HashSet::from([ControllerButton::FaceBottom]));
        m.reset_controller_input(Some(&held));
        let mut events = EventQueue::passthrough();
        m.handle_browse_input(&held, &mut events);
        assert!(
            events.drain_pending().is_empty(),
            "holdover reset must not activate [+] while A still held"
        );
        assert!(m.pending_select.is_none());
        assert!(!m.table_entered);
    }

    #[test]
    fn activate_plus_on_gateway_prefills_send_key_action() {
        let mut m = MappingsState::new();
        enter_table_cells(&mut m);
        m.focus_row = gateway_row(&m);
        m.focus_col = 0;
        let mut events = EventQueue::passthrough();
        m.activate_focused(&mut events, &EventSource::MouseClick);
        assert!(matches!(m.pending_select, Some(PendingSelect::Add)));
        let drained = events.drain_pending();
        assert_eq!(
            drained.first().map(|(e, _)| e),
            Some(&Event::CallState(CallRequest::SelectKey {
                binding: String::new(),
                action: "sendKey.".to_owned(),
                mode: StateId::Keyboard,
                draft_mode: HashMap::new(),
                editing: None,
            }))
        );
    }

    #[test]
    fn activate_pill_prefills_replace_binding_and_editing() {
        let mut m = MappingsState::new();
        enter_table_cells(&mut m);
        m.current_draft_mut().insert(
            "toggleShift".to_owned(),
            vec![pill(single(ControllerButton::FaceTop))],
        );
        m.focus_row = m
            .rows()
            .iter()
            .position(|r| r == "toggleShift")
            .expect("toggleShift row");
        m.focus_col = 0;
        let mut events = EventQueue::passthrough();
        m.activate_focused(&mut events, &EventSource::MouseClick);
        assert!(matches!(
            m.pending_select,
            Some(PendingSelect::Replace { .. })
        ));
        let drained = events.drain_pending();
        assert_eq!(
            drained.first().map(|(e, _)| e),
            Some(&Event::CallState(CallRequest::SelectKey {
                binding: "faceTop".to_owned(),
                action: "toggleShift".to_owned(),
                mode: StateId::Keyboard,
                draft_mode: HashMap::from([(
                    "toggleShift".to_owned(),
                    vec![pill(single(ControllerButton::FaceTop))]
                )]),
                editing: Some(pill(single(ControllerButton::FaceTop))),
            }))
        );
    }

    #[test]
    fn delete_modal_confirm_removes_pill() {
        let mut m = MappingsState::new();
        enter_table_cells(&mut m);
        m.current_draft_mut().insert(
            "toggleShift".to_owned(),
            vec![pill(single(ControllerButton::FaceTop))],
        );
        m.focus_row = m
            .rows()
            .iter()
            .position(|r| r == "toggleShift")
            .expect("toggleShift row");
        m.focus_col = 0;
        let mut events = EventQueue::passthrough();

        let y = ButtonSetInput(HashSet::from([ControllerButton::FaceTop]));
        let none = ButtonSetInput(HashSet::new());
        let right = ButtonSetInput(HashSet::from([ControllerButton::DpadRight]));
        let a = ButtonSetInput(HashSet::from([ControllerButton::FaceBottom]));
        m.handle_browse_input(&y, &mut events);
        assert_eq!(
            m.delete_confirm,
            Some(("toggleShift".to_owned(), 0)),
            "rising Y on a pill opens the delete modal"
        );
        assert_eq!(m.delete_modal_focus, DeleteModalFocus::Cancel);
        // Cancel starts focused; move to Confirm, then A deletes.
        m.handle_browse_input(&none, &mut events);
        m.handle_browse_input(&right, &mut events);
        assert_eq!(m.delete_modal_focus, DeleteModalFocus::Confirm);
        m.handle_browse_input(&none, &mut events);
        m.handle_browse_input(&a, &mut events);
        assert!(m.delete_confirm.is_none());
        assert!(!m.current_draft().contains_key("toggleShift"));
        assert!(m.dirty);
    }

    #[test]
    fn delete_modal_cancel_keeps_pill() {
        let mut m = MappingsState::new();
        enter_table_cells(&mut m);
        m.current_draft_mut().insert(
            "toggleShift".to_owned(),
            vec![pill(single(ControllerButton::FaceTop))],
        );
        m.focus_row = m
            .rows()
            .iter()
            .position(|r| r == "toggleShift")
            .expect("toggleShift row");
        m.focus_col = 0;
        let mut events = EventQueue::passthrough();

        let y = ButtonSetInput(HashSet::from([ControllerButton::FaceTop]));
        let none = ButtonSetInput(HashSet::new());
        let a = ButtonSetInput(HashSet::from([ControllerButton::FaceBottom]));
        m.handle_browse_input(&y, &mut events);
        assert!(m.delete_confirm.is_some());
        assert_eq!(m.delete_modal_focus, DeleteModalFocus::Cancel);
        // A on Cancel dismisses without deleting.
        m.handle_browse_input(&none, &mut events);
        m.handle_browse_input(&a, &mut events);
        assert!(m.delete_confirm.is_none());
        assert_eq!(
            m.current_draft().get("toggleShift").cloned(),
            Some(vec![pill(single(ControllerButton::FaceTop))])
        );
        assert!(!m.dirty);
    }

    #[test]
    fn delete_modal_b_also_dismisses() {
        let mut m = MappingsState::new();
        enter_table_cells(&mut m);
        m.current_draft_mut().insert(
            "toggleShift".to_owned(),
            vec![pill(single(ControllerButton::FaceTop))],
        );
        m.focus_row = m
            .rows()
            .iter()
            .position(|r| r == "toggleShift")
            .expect("toggleShift row");
        m.focus_col = 0;
        let mut events = EventQueue::passthrough();

        let y = ButtonSetInput(HashSet::from([ControllerButton::FaceTop]));
        let none = ButtonSetInput(HashSet::new());
        let b = ButtonSetInput(HashSet::from([ControllerButton::FaceRight]));
        m.handle_browse_input(&y, &mut events);
        assert!(m.delete_confirm.is_some());
        m.handle_browse_input(&none, &mut events);
        m.handle_browse_input(&b, &mut events);
        assert!(m.delete_confirm.is_none());
        assert!(m.current_draft().contains_key("toggleShift"));
        assert!(!m.dirty);
    }

    fn press(m: &mut MappingsState, button: ControllerButton, events: &mut EventQueue) {
        let none = ButtonSetInput(HashSet::new());
        let held = ButtonSetInput(HashSet::from([button]));
        m.handle_browse_input(&none, events);
        m.handle_browse_input(&held, events);
    }

    #[test]
    fn analog_up_moves_focus_like_dpad_up() {
        let mut m = MappingsState::new();
        enter_browse_focus(&mut m);
        let mut events = EventQueue::passthrough();
        press(&mut m, ControllerButton::DpadDown, &mut events);
        assert_eq!(m.focus_zone, FocusZone::Cancel);
        let center = AnalogInput {
            stick: (0.0, 0.0),
            buttons: HashSet::new(),
        };
        let up = AnalogInput {
            stick: (0.0, -0.8),
            buttons: HashSet::new(),
        };
        m.handle_browse_input(&center, &mut events);
        m.handle_browse_input(&up, &mut events);
        assert_eq!(m.focus_zone, FocusZone::Table);
    }

    #[test]
    fn outer_table_down_up_visits_cancel_then_save() {
        let mut m = MappingsState::new();
        enter_browse_focus(&mut m);
        let mut events = EventQueue::passthrough();

        press(&mut m, ControllerButton::DpadDown, &mut events);
        assert_eq!(m.focus_zone, FocusZone::Cancel);

        press(&mut m, ControllerButton::DpadDown, &mut events);
        assert_eq!(m.focus_zone, FocusZone::Save);

        press(&mut m, ControllerButton::DpadUp, &mut events);
        assert_eq!(m.focus_zone, FocusZone::Cancel);

        press(&mut m, ControllerButton::DpadUp, &mut events);
        assert_eq!(m.focus_zone, FocusZone::Table);
        assert!(!m.table_entered);
    }

    #[test]
    fn outer_table_a_enters_b_backs_out_without_leave() {
        let mut m = MappingsState::new();
        enter_browse_focus(&mut m);
        let mut events = EventQueue::passthrough();

        press(&mut m, ControllerButton::FaceBottom, &mut events);
        assert!(m.table_entered);
        assert_eq!(m.focus_zone, FocusZone::Table);

        press(&mut m, ControllerButton::FaceRight, &mut events);
        assert!(!m.table_entered);
        assert_eq!(m.focus_zone, FocusZone::Table);
        assert!(
            events.drain_pending().is_empty(),
            "B while table_entered must not ChangeState"
        );
    }

    #[test]
    fn table_entered_up_down_stay_inside_at_edges() {
        let mut m = MappingsState::new();
        enter_table_cells(&mut m);
        let n = m.rows().len();
        assert!(n > 0);
        m.focus_row = n - 1;
        m.focus_col = 0;
        let mut events = EventQueue::passthrough();

        press(&mut m, ControllerButton::DpadDown, &mut events);
        assert_eq!(m.focus_zone, FocusZone::Table);
        assert!(m.table_entered);
        assert_eq!(m.focus_row, n - 1);

        m.focus_row = 0;
        press(&mut m, ControllerButton::DpadUp, &mut events);
        assert_eq!(m.focus_zone, FocusZone::Table);
        assert!(m.table_entered);
        assert_eq!(m.focus_row, 0);
    }

    #[test]
    fn outer_table_up_to_tabs_then_down_back() {
        let mut m = MappingsState::new();
        enter_browse_focus(&mut m);
        let mut events = EventQueue::passthrough();

        press(&mut m, ControllerButton::DpadUp, &mut events);
        assert_eq!(m.focus_zone, FocusZone::Tabs);

        press(&mut m, ControllerButton::DpadDown, &mut events);
        assert_eq!(m.focus_zone, FocusZone::Table);
        assert!(!m.table_entered);
    }
}
