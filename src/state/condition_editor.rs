use super::menu_action::MenuAction;
use crate::config::{self, ConditionDraft, ConditionTerm, EditingScope, TermJoin};
use crate::{state::StateId, ui::menu_list};
use egui::{Button, Id, TextEdit, Ui};

const GROUP_INDENT: f32 = 14.0;
const ROW_HEIGHT: f32 = 28.0;
const ACTION_HEIGHT: f32 = 32.0;
const CONNECTOR_WIDTH: f32 = 64.0;
const TYPE_WIDTH: f32 = 112.0;
const NEGATION_WIDTH: f32 = 70.0;
const REMOVE_WIDTH: f32 = 24.0;
const MINIMUM_VALUE_WIDTH: f32 = 160.0;
const STATES: &[(StateId, &str)] = &[
    (StateId::Keyboard, "Keyboard"),
    (StateId::Settings, "Settings"),
    (StateId::TextInput, "Text input"),
    (StateId::Mappings, "Mappings"),
    (StateId::MoveWindow, "Move window"),
    (StateId::SelectKey, "Key selection"),
    (StateId::SelectLayout, "Layout selection"),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Focus {
    Target,
    Name,
    Join(usize),
    Kind(usize),
    Negation(usize),
    App(usize),
    Remove(usize),
    Add,
    Cancel,
    Use,
}

pub(super) struct ConditionEditor {
    choices: Vec<ConditionDraft>,
    selection: usize,
    draft: ConditionDraft,
    focus: Focus,
    request_name_focus: bool,
    apps: Vec<(String, String)>,
    current_app: Option<String>,
    picker: Option<usize>,
    app_focus: usize,
    search: String,
    error: String,
    finished: Option<EditingScope>,
    cancelled: bool,
    reveal_focus: bool,
}

impl ConditionEditor {
    pub(super) fn new(scope: &EditingScope) -> Self {
        let choices = config::condition_choices();
        let selection = choices
            .iter()
            .position(|draft| &draft.scope == scope)
            .map_or(0, |index| index + 1);
        let draft = selection
            .checked_sub(1)
            .map(|index| choices[index].clone())
            .unwrap_or_else(ConditionDraft::new);
        Self {
            choices,
            selection,
            draft,
            focus: Focus::Target,
            request_name_focus: false,
            apps: config::condition_apps(),
            current_app: crate::when::WhenContext::with_focused_app().focused_app,
            picker: None,
            app_focus: 0,
            search: String::new(),
            error: String::new(),
            finished: None,
            cancelled: false,
            reveal_focus: true,
        }
    }

    fn is_main(&self) -> bool {
        self.selection == 0
    }

    pub(super) fn back_hint(&self) -> &'static str {
        if self.picker.is_some() {
            "back"
        } else {
            "cancel"
        }
    }

    fn selection_label(&self) -> String {
        if self.is_main() {
            return "Main config".into();
        }
        self.choices
            .get(self.selection - 1)
            .map(|draft| draft.name.clone())
            .unwrap_or_else(|| "New condition".into())
    }

    fn select(&mut self, selection: usize) {
        self.selection = selection;
        self.draft = selection
            .checked_sub(1)
            .and_then(|index| self.choices.get(index))
            .cloned()
            .unwrap_or_else(ConditionDraft::new);
        self.error.clear();
        self.picker = None;
    }

    fn fields(&self) -> Vec<Focus> {
        let mut fields = vec![Focus::Target];
        if !self.is_main() {
            fields.push(Focus::Name);
            for index in 0..self.draft.terms.len() {
                if index > 0 {
                    fields.push(Focus::Join(index));
                }
                fields.extend([
                    Focus::Kind(index),
                    Focus::Negation(index),
                    Focus::App(index),
                    Focus::Remove(index),
                ]);
            }
            if self.draft.row_editable() {
                fields.push(Focus::Add);
            }
        }
        fields.extend([Focus::Cancel, Focus::Use]);
        fields
    }

    fn add(&mut self) {
        if !self.draft.row_editable() {
            return;
        }
        self.draft.terms.push(ConditionTerm::default());
        self.focus = Focus::App(self.draft.terms.len() - 1);
    }

    fn remove(&mut self, index: usize) {
        self.draft.terms.remove(index);
        if let Some(first) = self.draft.terms.first_mut() {
            first.join = TermJoin::And;
        }
        self.focus = Focus::Add;
    }

    fn toggle_join(&mut self, index: usize) {
        let join = &mut self.draft.terms[index].join;
        *join = if *join == TermJoin::And {
            TermJoin::Or
        } else {
            TermJoin::And
        };
    }

    fn use_condition(&mut self) {
        if self.is_main() {
            self.finished = Some(EditingScope::Main);
            return;
        }
        match config::save_condition(&self.draft) {
            Ok(scope) => self.finished = Some(scope),
            Err(error) => self.error = format!("{error:#}"),
        }
    }

    fn filtered_apps(&self) -> Vec<(String, String)> {
        let search = self.search.to_lowercase();
        self.apps
            .iter()
            .filter(|(label, _)| label.to_lowercase().contains(&search))
            .cloned()
            .collect()
    }

    pub(super) fn handle(&mut self, action: &MenuAction) {
        self.reveal_focus = true;
        if self.picker.is_some() {
            self.handle_picker(action);
            return;
        }
        match action {
            MenuAction::Back | MenuAction::EditConfigCondition => self.cancelled = true,
            MenuAction::SelectUp | MenuAction::SelectDown => {
                let fields = self.fields();
                let index = fields
                    .iter()
                    .position(|field| *field == self.focus)
                    .unwrap_or(0) as i32;
                let direction = if *action == MenuAction::SelectUp {
                    -1
                } else {
                    1
                };
                self.focus = fields[(index + direction).rem_euclid(fields.len() as i32) as usize];
            }
            MenuAction::SelectLeft | MenuAction::SelectRight => {
                let direction = if *action == MenuAction::SelectLeft {
                    -1
                } else {
                    1
                };
                match self.focus {
                    Focus::Target => self.select(
                        (self.selection as i32 + direction)
                            .rem_euclid((self.choices.len() + 2) as i32)
                            as usize,
                    ),
                    Focus::Join(index) => self.toggle_join(index),
                    Focus::Kind(index) => self.toggle_kind(index),
                    Focus::App(index) if self.draft.terms[index].state.is_some() => {
                        self.change_state(index, direction)
                    }
                    Focus::Negation(index) => {
                        self.draft.terms[index].negated = !self.draft.terms[index].negated
                    }
                    Focus::Cancel => self.focus = Focus::Use,
                    Focus::Use => self.focus = Focus::Cancel,
                    _ => {}
                }
            }
            MenuAction::Activate => match self.focus {
                Focus::Target => self.select((self.selection + 1) % (self.choices.len() + 2)),
                Focus::Name => self.request_name_focus = true,
                Focus::Join(index) => self.toggle_join(index),
                Focus::Kind(index) => self.toggle_kind(index),
                Focus::Negation(index) => {
                    self.draft.terms[index].negated = !self.draft.terms[index].negated
                }
                Focus::App(index) if self.draft.terms[index].state.is_some() => {
                    self.change_state(index, 1)
                }
                Focus::App(index) => self.open_picker(index),
                Focus::Remove(index) => self.remove(index),
                Focus::Add => self.add(),
                Focus::Cancel => self.cancelled = true,
                Focus::Use => self.use_condition(),
            },
            _ => {}
        }
    }

    fn open_picker(&mut self, index: usize) {
        self.picker = Some(index);
        self.app_focus = 0;
        self.search.clear();
    }

    fn handle_picker(&mut self, action: &MenuAction) {
        let apps = self.filtered_apps();
        match action {
            MenuAction::Back => self.picker = None,
            MenuAction::SelectUp => self.app_focus = self.app_focus.saturating_sub(1),
            MenuAction::SelectDown => self.app_focus = (self.app_focus + 1).min(apps.len()),
            MenuAction::Activate => {
                if let Some((_, executable)) = apps.get(self.app_focus) {
                    self.draft.terms[self.picker.unwrap()].executable = executable.clone();
                }
                self.picker = None;
            }
            _ => {}
        }
    }

    fn toggle_kind(&mut self, index: usize) {
        let state = &mut self.draft.terms[index].state;
        *state = if state.is_some() {
            None
        } else {
            Some(StateId::Keyboard)
        };
    }

    fn change_state(&mut self, index: usize, direction: i32) {
        let state = &mut self.draft.terms[index].state;
        let position = STATES
            .iter()
            .position(|(value, _)| Some(*value) == *state)
            .unwrap_or(0);
        *state =
            Some(STATES[(position as i32 + direction).rem_euclid(STATES.len() as i32) as usize].0);
    }

    fn control(&mut self, ui: &mut Ui, focus: Focus, label: &str, width: f32) -> egui::Response {
        let response = ui.add_sized(
            [width, ROW_HEIGHT],
            Button::new(label).selected(self.focus == focus).truncate(),
        );
        if self.reveal_focus && self.focus == focus {
            response.scroll_to_me(Some(egui::Align::Center));
        }
        if response.clicked() {
            self.focus = focus;
        }
        response
    }

    fn button(&mut self, ui: &mut Ui, focus: Focus, label: &str, width: f32) -> bool {
        self.control(ui, focus, label, width).clicked()
    }

    fn switch(&mut self, ui: &mut Ui, focus: Focus, label: &str, width: f32) -> Option<i32> {
        let response = self.control(ui, focus, &format!("< {label} >"), width);
        response.clicked().then(|| {
            if response
                .interact_pointer_pos()
                .is_some_and(|position| position.x < response.rect.center().x)
            {
                -1
            } else {
                1
            }
        })
    }

    fn draw_term(&mut self, ui: &mut Ui, index: usize) -> Option<usize> {
        let mut remove = None;
        ui.horizontal(|ui| {
            let connector = if self.draft.terms[index].join == TermJoin::And {
                "AND"
            } else {
                "OR"
            };
            if index == 0 {
                ui.allocate_space(egui::vec2(CONNECTOR_WIDTH, ROW_HEIGHT));
            } else if self
                .switch(ui, Focus::Join(index), connector, CONNECTOR_WIDTH)
                .is_some()
            {
                self.toggle_join(index);
            }
            let kind = if self.draft.terms[index].state.is_some() {
                "State"
            } else {
                "App focused"
            };
            if self
                .switch(ui, Focus::Kind(index), kind, TYPE_WIDTH)
                .is_some()
            {
                self.toggle_kind(index);
            }
            let label = if self.draft.terms[index].negated {
                "is not"
            } else {
                "is"
            };
            if self
                .switch(ui, Focus::Negation(index), label, NEGATION_WIDTH)
                .is_some()
            {
                self.draft.terms[index].negated = !self.draft.terms[index].negated;
            }
            let term = &self.draft.terms[index];
            let label = if let Some(state) = term.state {
                STATES
                    .iter()
                    .find(|(value, _)| *value == state)
                    .unwrap()
                    .1
                    .to_owned()
            } else if term.executable.is_empty() {
                "Select app…".into()
            } else {
                term.executable.clone()
            };
            let width =
                (ui.available_width() - REMOVE_WIDTH - ui.spacing().item_spacing.x).max(40.0);
            if self.draft.terms[index].state.is_some() {
                if let Some(direction) = self.switch(ui, Focus::App(index), &label, width) {
                    self.change_state(index, direction);
                }
            } else if self.button(ui, Focus::App(index), &label, width) {
                self.open_picker(index);
            }
            if self.button(ui, Focus::Remove(index), "−", REMOVE_WIDTH) {
                remove = Some(index);
            }
        });
        remove
    }

    fn draw_terms(&mut self, ui: &mut Ui) {
        let mut remove = None;
        for group in self.draft.groups() {
            let response = ui.indent(Id::new(("condition_group", group.start)), |ui| {
                for index in group.clone() {
                    remove = self.draw_term(ui, index).or(remove);
                }
            });
            if group.len() > 1 {
                let rect = response.response.rect;
                let x = rect.left() - GROUP_INDENT / 2.0;
                let stroke = ui.visuals().widgets.noninteractive.bg_stroke;
                ui.painter().line_segment(
                    [egui::pos2(x + 5.0, rect.top()), egui::pos2(x, rect.top())],
                    stroke,
                );
                ui.painter().line_segment(
                    [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                    stroke,
                );
                ui.painter().line_segment(
                    [
                        egui::pos2(x, rect.bottom()),
                        egui::pos2(x + 5.0, rect.bottom()),
                    ],
                    stroke,
                );
            }
        }
        if !self.draft.row_editable() {
            ui.label(
                "This condition is too large for the row editor. Its name can still be edited.",
            );
        } else {
            ui.indent("condition_add_row", |ui| {
                if self.button(ui, Focus::Add, "[+]", ui.available_width()) {
                    self.add();
                }
            });
        }
        if let Some(index) = remove {
            self.remove(index);
        }
    }

    fn draw_destination(&mut self, ui: &mut Ui) {
        if let Some(direction) = self.switch(
            ui,
            Focus::Target,
            &self.selection_label(),
            ui.available_width(),
        ) {
            self.select(
                (self.selection as i32 + direction).rem_euclid((self.choices.len() + 2) as i32)
                    as usize,
            );
        }
        if !self.is_main() {
            ui.horizontal(|ui| {
                ui.label("Name");
                let response = ui.add(
                    TextEdit::singleline(&mut self.draft.name)
                        .id(Id::new("condition_name"))
                        .desired_width(ui.available_width()),
                );
                if self.focus == Focus::Name {
                    ui.painter().rect_stroke(
                        response.rect,
                        ui.visuals().widgets.inactive.corner_radius,
                        ui.visuals().selection.stroke,
                        egui::StrokeKind::Outside,
                    );
                    if self.reveal_focus {
                        response.scroll_to_me(Some(egui::Align::Center));
                    }
                }
                if self.request_name_focus {
                    response.request_focus();
                    self.request_name_focus = false;
                }
                if response.clicked() {
                    self.focus = Focus::Name;
                }
            });
            ui.label("Apply settings when");
        }
    }

    pub(super) fn draw(
        &mut self,
        ui: &mut Ui,
        mut footer: impl FnMut(&mut Ui),
    ) -> (bool, Option<EditingScope>) {
        let drawn_focus = (self.focus, self.picker, self.app_focus);
        let row_width = CONNECTOR_WIDTH
            + TYPE_WIDTH
            + NEGATION_WIDTH
            + MINIMUM_VALUE_WIDTH
            + REMOVE_WIDTH
            + ui.spacing().indent
            + 4.0 * ui.spacing().item_spacing.x;
        ui.set_width(ui.available_width().max(row_width));
        ui.heading(if self.picker.is_some() {
            "Select app"
        } else {
            "Condition"
        });
        if let Some(index) = self.picker {
            self.draw_picker_inputs(ui, index);
        } else {
            self.draw_destination(ui);
        }
        let (mut body, mut bottom) = menu_list::page(ui, ACTION_HEIGHT);
        if let Some(index) = self.picker {
            self.draw_picker(&mut body, &mut bottom, index);
        } else {
            egui::ScrollArea::vertical()
                .id_salt("condition_editor_body")
                .auto_shrink([false, false])
                .show(&mut body, |ui| {
                    if !self.is_main() {
                        self.draw_terms(ui);
                    }
                });
            bottom.horizontal(|ui| {
                if self.button(ui, Focus::Cancel, "Cancel", 80.0) {
                    self.cancelled = true;
                }
                if self.button(ui, Focus::Use, "Use condition", 128.0) {
                    self.use_condition();
                }
            });
        }
        bottom.separator();
        footer(&mut bottom);
        if !self.error.is_empty() {
            bottom
                .add(
                    egui::Label::new(
                        egui::RichText::new(&self.error).color(bottom.visuals().error_fg_color),
                    )
                    .truncate(),
                )
                .on_hover_text(&self.error);
        }
        self.reveal_focus = drawn_focus != (self.focus, self.picker, self.app_focus);
        (self.cancelled, self.finished.take())
    }

    fn draw_picker_inputs(&mut self, ui: &mut Ui, index: usize) {
        ui.horizontal(|ui| {
            ui.label("Search");
            if ui.text_edit_singleline(&mut self.search).changed() {
                self.app_focus = 0;
            }
        });
        ui.horizontal(|ui| {
            ui.label("Executable");
            ui.text_edit_singleline(&mut self.draft.terms[index].executable);
        });
        if let Some(current) = &self.current_app
            && ui.button(format!("Use current app: {current}")).clicked()
        {
            self.draft.terms[index].executable = current.clone();
            self.picker = None;
        }
    }

    fn draw_picker(&mut self, body: &mut Ui, bottom: &mut Ui, index: usize) {
        egui::ScrollArea::vertical()
            .id_salt("condition_app_picker_body")
            .auto_shrink([false, false])
            .show(body, |ui| {
                for (position, (label, executable)) in self.filtered_apps().iter().enumerate() {
                    let response = ui.add_sized(
                        [ui.available_width(), ROW_HEIGHT],
                        Button::new(label)
                            .selected(self.app_focus == position)
                            .truncate(),
                    );
                    if self.reveal_focus && self.app_focus == position {
                        response.scroll_to_me(Some(egui::Align::Center));
                    }
                    if response.clicked() {
                        self.draft.terms[index].executable = executable.clone();
                        self.picker = None;
                    }
                }
            });
        if bottom
            .add_sized(
                [80.0, ROW_HEIGHT],
                Button::new("Done").selected(self.app_focus == self.filtered_apps().len()),
            )
            .clicked()
        {
            self.picker = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::Context;

    fn editor() -> ConditionEditor {
        ConditionEditor {
            choices: Vec::new(),
            selection: 0,
            draft: ConditionDraft::new(),
            focus: Focus::Target,
            request_name_focus: false,
            apps: vec![("Factorio".into(), "factorio.exe".into())],
            current_app: None,
            picker: None,
            app_focus: 0,
            search: String::new(),
            error: String::new(),
            finished: None,
            cancelled: false,
            reveal_focus: true,
        }
    }

    #[test]
    fn controller_add_select_app_toggle_connector_and_cancel_stays_in_popup() {
        let mut editor = editor();
        assert!(editor.is_main());
        editor.handle(&MenuAction::SelectRight);
        assert!(!editor.is_main());
        editor.focus = Focus::Add;
        editor.handle(&MenuAction::Activate);
        assert_eq!(editor.focus, Focus::App(0));
        editor.handle(&MenuAction::Activate);
        assert_eq!(editor.picker, Some(0));
        editor.handle(&MenuAction::Activate);
        assert_eq!(editor.draft.terms[0].executable, "factorio.exe");
        editor.focus = Focus::Add;
        editor.handle(&MenuAction::Activate);
        assert_eq!(editor.draft.terms[1].join, TermJoin::And);
        editor.focus = Focus::Join(1);
        editor.handle(&MenuAction::SelectRight);
        assert_eq!(editor.draft.groups(), [0..1, 1..2]);
        editor.handle(&MenuAction::SelectLeft);
        assert_eq!(
            editor.draft.groups(),
            std::iter::once(0..2).collect::<Vec<_>>()
        );
        editor.focus = Focus::App(1);
        editor.handle(&MenuAction::Activate);
        editor.handle(&MenuAction::Back);
        assert!(!editor.cancelled);
        editor.handle(&MenuAction::Back);
        assert!(editor.cancelled);
        assert!(editor.finished.is_none());
    }

    #[test]
    fn footer_stays_at_window_bottom_when_focus_or_content_changes() {
        let ctx = Context::default();
        let mut editor = editor();
        editor.select(1);
        editor.add();
        let mut bottoms = Vec::new();
        for focus in [Focus::Target, Focus::Name, Focus::App(0), Focus::Add] {
            editor.focus = focus;
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(440.0, 313.0),
                    )),
                    ..Default::default()
                },
                |ui| {
                    editor.draw(ui, |ui| {
                        bottoms.push(ui.label("footer").rect.bottom());
                    });
                },
            );
            output.textures_delta.clear();
        }
        for _ in 0..16 {
            editor.add();
        }
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(440.0, 313.0),
                )),
                ..Default::default()
            },
            |ui| {
                editor.draw(ui, |ui| {
                    bottoms.push(ui.label("footer").rect.bottom());
                });
            },
        );
        output.textures_delta.clear();
        assert!(
            bottoms
                .iter()
                .all(|bottom| (*bottom - bottoms[0]).abs() < 1.0),
            "{bottoms:?}"
        );
        assert!(bottoms[0] > 250.0, "{bottoms:?}");
    }

    #[test]
    fn controller_changes_condition_type_and_state_in_both_directions() {
        let mut editor = editor();
        editor.select(1);
        editor.add();
        editor.focus = Focus::Kind(0);
        editor.handle(&MenuAction::SelectRight);
        assert_eq!(editor.draft.terms[0].state, Some(StateId::Keyboard));
        editor.focus = Focus::App(0);
        editor.handle(&MenuAction::SelectRight);
        assert_eq!(editor.draft.terms[0].state, Some(StateId::Settings));
        editor.handle(&MenuAction::SelectLeft);
        assert_eq!(editor.draft.terms[0].state, Some(StateId::Keyboard));
        assert!(editor.picker.is_none());
        assert_eq!(
            editor.draft.expression().unwrap().as_deref(),
            Some("state.keyboard")
        );
        editor.focus = Focus::Kind(0);
        editor.handle(&MenuAction::SelectLeft);
        assert!(editor.draft.terms[0].state.is_none());
    }

    #[test]
    fn focused_controls_allow_manual_scroll_without_recentring() {
        let ctx = Context::default();
        let mut editor = editor();
        editor.select(1);
        for _ in 0..16 {
            editor.add();
        }
        editor.focus = Focus::App(0);
        let mut area_id = Id::NULL;
        for frame in 0..5 {
            if frame == 2 {
                let mut state = egui::scroll_area::State::default();
                state.offset.y = 160.0;
                state.store(&ctx, area_id);
            }
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(440.0, 313.0),
                    )),
                    time: Some(frame as f64),
                    ..Default::default()
                },
                |ui| {
                    let (mut body, _) = menu_list::page(ui, ACTION_HEIGHT);
                    let area = egui::ScrollArea::vertical()
                        .id_salt("condition_editor_body")
                        .auto_shrink([false, false])
                        .show(&mut body, |ui| {
                            editor.draw_terms(ui);
                        });
                    area_id = area.id;
                    editor.reveal_focus = false;
                },
            );
            output.textures_delta.clear();
        }
        assert_eq!(
            egui::scroll_area::State::load(&ctx, area_id)
                .unwrap()
                .offset
                .y,
            160.0
        );
    }

    #[test]
    fn cancel_precedes_use_in_controller_navigation() {
        let mut editor = editor();
        editor.focus = Focus::Cancel;
        editor.handle(&MenuAction::SelectDown);
        assert_eq!(editor.focus, Focus::Use);
        editor.handle(&MenuAction::SelectLeft);
        assert_eq!(editor.focus, Focus::Cancel);
        editor.handle(&MenuAction::Activate);
        assert!(editor.cancelled);
        assert!(editor.finished.is_none());
    }
}
