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
const CHOICE_HEIGHT_LIMIT: f32 = 224.0;
const CHOICE_EDGE_INSET: f32 = 16.0;
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

#[derive(Clone, Copy, Default)]
enum Column {
    Connector,
    #[default]
    ConditionType,
    Comparison,
    Value,
    Remove,
}

impl Focus {
    fn column(self) -> Option<Column> {
        Some(match self {
            Self::Join(_) | Self::Cancel => Column::Connector,
            Self::Kind(_) | Self::Use => Column::ConditionType,
            Self::Negation(_) => Column::Comparison,
            Self::App(_) => Column::Value,
            Self::Remove(_) => Column::Remove,
            Self::Target | Self::Name | Self::Add => return None,
        })
    }
}

struct ChoiceMenu {
    focus: Focus,
    selection: usize,
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
    column: Column,
    choice: Option<ChoiceMenu>,
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
            column: Column::default(),
            choice: None,
        }
    }

    fn is_main(&self) -> bool {
        self.selection == 0
    }

    pub(super) fn back_hint(&self) -> &'static str {
        if self.picker.is_some() || self.choice.is_some() {
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
        self.choice = None;
    }

    fn rows(&self) -> Vec<Vec<Focus>> {
        let mut rows = vec![vec![Focus::Target]];
        if !self.is_main() {
            rows.push(vec![Focus::Name]);
            for index in 0..self.draft.terms.len() {
                let mut fields = Vec::new();
                if index > 0 {
                    fields.push(Focus::Join(index));
                }
                fields.extend([
                    Focus::Kind(index),
                    Focus::Negation(index),
                    Focus::App(index),
                    Focus::Remove(index),
                ]);
                rows.push(fields);
            }
            if self.draft.row_editable() {
                rows.push(vec![Focus::Add]);
            }
        }
        rows.push(vec![Focus::Cancel, Focus::Use]);
        rows
    }

    fn set_focus(&mut self, focus: Focus) {
        self.focus = focus;
        if let Some(column) = focus.column() {
            self.column = column;
        }
    }

    fn navigate(&mut self, action: &MenuAction) {
        let rows = self.rows();
        let Some(row) = rows.iter().position(|row| row.contains(&self.focus)) else {
            return;
        };
        let direction = if matches!(action, MenuAction::SelectUp | MenuAction::SelectLeft) {
            -1
        } else {
            1
        };
        let focus = if matches!(action, MenuAction::SelectLeft | MenuAction::SelectRight) {
            let position = rows[row]
                .iter()
                .position(|field| *field == self.focus)
                .unwrap();
            let position =
                (position as i32 + direction).clamp(0, rows[row].len() as i32 - 1) as usize;
            rows[row][position]
        } else {
            let next_row = (row as i32 + direction).clamp(0, rows.len() as i32 - 1) as usize;
            if next_row == row {
                return;
            }
            *rows[next_row]
                .iter()
                .min_by_key(|field| {
                    field
                        .column()
                        .map_or(0, |column| (column as usize).abs_diff(self.column as usize))
                })
                .unwrap()
        };
        self.set_focus(focus);
    }

    fn add(&mut self) {
        if !self.draft.row_editable() {
            return;
        }
        self.draft.terms.push(ConditionTerm::default());
        self.set_focus(Focus::Kind(self.draft.terms.len() - 1));
    }

    fn remove(&mut self, index: usize) {
        self.draft.terms.remove(index);
        if let Some(first) = self.draft.terms.first_mut() {
            first.join = TermJoin::And;
        }
        self.set_focus(Focus::Add);
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
        if self.choice.is_some() {
            self.handle_choice(action);
            return;
        }
        match action {
            MenuAction::Back | MenuAction::EditConfigCondition => self.cancelled = true,
            MenuAction::SelectUp
            | MenuAction::SelectDown
            | MenuAction::SelectLeft
            | MenuAction::SelectRight => self.navigate(action),
            MenuAction::Activate => match self.focus {
                Focus::Target | Focus::Join(_) | Focus::Kind(_) | Focus::Negation(_) => {
                    self.open_choice(self.focus)
                }
                Focus::Name => self.request_name_focus = true,
                Focus::App(index) if self.draft.terms[index].state.is_some() => {
                    self.open_choice(self.focus)
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

    fn choice_labels(&self, focus: Focus) -> Vec<String> {
        match focus {
            Focus::Target => std::iter::once("Main config".into())
                .chain(self.choices.iter().map(|draft| draft.name.clone()))
                .chain(std::iter::once("New condition".into()))
                .collect(),
            Focus::Join(_) => vec!["AND".into(), "OR".into()],
            Focus::Kind(_) => vec!["App focused".into(), "State".into()],
            Focus::Negation(_) => vec!["is".into(), "is not".into()],
            Focus::App(_) => STATES.iter().map(|(_, label)| (*label).into()).collect(),
            _ => Vec::new(),
        }
    }

    fn open_choice(&mut self, focus: Focus) {
        self.reveal_focus = true;
        let selection = match focus {
            Focus::Target => self.selection,
            Focus::Join(index) => usize::from(self.draft.terms[index].join == TermJoin::Or),
            Focus::Kind(index) => usize::from(self.draft.terms[index].state.is_some()),
            Focus::Negation(index) => usize::from(self.draft.terms[index].negated),
            Focus::App(index) => STATES
                .iter()
                .position(|(state, _)| Some(*state) == self.draft.terms[index].state)
                .unwrap_or(0),
            _ => return,
        };
        self.choice = Some(ChoiceMenu { focus, selection });
    }

    fn apply_choice(&mut self, focus: Focus, selection: usize) {
        match focus {
            Focus::Target => self.select(selection),
            Focus::Join(index) => {
                self.draft.terms[index].join = if selection == 0 {
                    TermJoin::And
                } else {
                    TermJoin::Or
                }
            }
            Focus::Kind(index) => {
                let state = &mut self.draft.terms[index].state;
                *state = if selection == 0 {
                    None
                } else {
                    Some(state.unwrap_or(StateId::Keyboard))
                };
            }
            Focus::Negation(index) => self.draft.terms[index].negated = selection != 0,
            Focus::App(index) => self.draft.terms[index].state = Some(STATES[selection].0),
            _ => {}
        }
        self.choice = None;
    }

    fn handle_choice(&mut self, action: &MenuAction) {
        let focus = self.choice.as_ref().unwrap().focus;
        let count = self.choice_labels(focus).len();
        match action {
            MenuAction::Back | MenuAction::EditConfigCondition => self.choice = None,
            MenuAction::SelectUp => {
                let selection = &mut self.choice.as_mut().unwrap().selection;
                *selection = selection.saturating_sub(1);
            }
            MenuAction::SelectDown => {
                let selection = &mut self.choice.as_mut().unwrap().selection;
                *selection = (*selection + 1).min(count - 1);
            }
            MenuAction::Activate => {
                let selection = self.choice.as_ref().unwrap().selection;
                self.apply_choice(focus, selection);
            }
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

    fn control(
        &mut self,
        ui: &mut Ui,
        focus: Focus,
        button: Button<'_>,
        width: f32,
    ) -> egui::Response {
        let response = ui.add_sized(
            [width, ROW_HEIGHT],
            button.selected(self.focus == focus).truncate(),
        );
        if self.reveal_focus && self.focus == focus {
            response.scroll_to_me(Some(egui::Align::Center));
        }
        if response.clicked() {
            self.set_focus(focus);
        }
        response
    }

    fn button(&mut self, ui: &mut Ui, focus: Focus, label: &str, width: f32) -> bool {
        self.control(ui, focus, Button::new(label), width).clicked()
    }

    fn dropdown(&mut self, ui: &mut Ui, focus: Focus, label: &str, width: f32) {
        let button = Button::new(label).right_text(egui::Atom::custom(
            Id::new("condition_dropdown_arrow"),
            egui::Vec2::splat(ui.spacing().icon_width),
        ));
        let response = self.control(ui, focus, button, width);
        response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, true, label));
        let center = egui::pos2(
            response.rect.right() - ui.spacing().button_padding.x - ui.spacing().icon_width / 2.0,
            response.rect.center().y,
        );
        let half = ui.spacing().icon_width / 4.0;
        let color = if self.focus == focus {
            ui.visuals().selection.stroke.color
        } else {
            ui.style().interact(&response).fg_stroke.color
        };
        ui.painter().add(egui::Shape::convex_polygon(
            vec![
                center + egui::vec2(-half, -half / 2.0),
                center + egui::vec2(half, -half / 2.0),
                center + egui::vec2(0.0, half / 2.0),
            ],
            color,
            egui::Stroke::NONE,
        ));
        if response.clicked() {
            if self
                .choice
                .as_ref()
                .is_some_and(|choice| choice.focus == focus)
            {
                self.choice = None;
            } else {
                self.open_choice(focus);
            }
        }
        let mut open = self
            .choice
            .as_ref()
            .is_some_and(|choice| choice.focus == focus);
        if !open {
            return;
        }
        let labels = self.choice_labels(focus);
        let selected = self.choice.as_ref().unwrap().selection;
        let height = (ui.ctx().content_rect().height() / 2.0 - CHOICE_EDGE_INSET)
            .clamp(ROW_HEIGHT, CHOICE_HEIGHT_LIMIT);
        let mut chosen = None;
        egui::Popup::from_response(&response)
            .kind(egui::PopupKind::Menu)
            .open_bool(&mut open)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .width(width.max(MINIMUM_VALUE_WIDTH))
            .show(|ui| {
                egui::ScrollArea::vertical()
                    .id_salt("condition_choice_list")
                    .max_height(height)
                    .show(ui, |ui| {
                        for (index, label) in labels.iter().enumerate() {
                            let response = ui.add_sized(
                                [ui.available_width(), ROW_HEIGHT],
                                Button::new(label).selected(index == selected),
                            );
                            if self.reveal_focus && index == selected {
                                response.scroll_to_me(None);
                            }
                            if response.clicked() {
                                chosen = Some(index);
                            }
                        }
                    });
            });
        if let Some(selection) = chosen {
            self.apply_choice(focus, selection);
        } else if !open {
            self.choice = None;
        }
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
            } else {
                self.dropdown(ui, Focus::Join(index), connector, CONNECTOR_WIDTH);
            }
            let kind = if self.draft.terms[index].state.is_some() {
                "State"
            } else {
                "App focused"
            };
            self.dropdown(ui, Focus::Kind(index), kind, TYPE_WIDTH);
            let label = if self.draft.terms[index].negated {
                "is not"
            } else {
                "is"
            };
            self.dropdown(ui, Focus::Negation(index), label, NEGATION_WIDTH);
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
                self.dropdown(ui, Focus::App(index), &label, width);
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
        self.dropdown(
            ui,
            Focus::Target,
            &self.selection_label(),
            ui.available_width(),
        );
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
                    self.set_focus(Focus::Name);
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
            column: Column::default(),
            choice: None,
        }
    }

    #[test]
    fn arrows_move_between_controls_and_rows_without_changing_conditions() {
        let mut editor = editor();
        editor.select(1);
        editor.add();
        editor.add();
        let terms = editor.draft.terms.clone();
        editor.focus = Focus::Kind(0);
        editor.handle(&MenuAction::SelectRight);
        assert_eq!(editor.focus, Focus::Negation(0));
        editor.handle(&MenuAction::SelectDown);
        assert_eq!(editor.focus, Focus::Negation(1));
        editor.handle(&MenuAction::SelectLeft);
        assert_eq!(editor.focus, Focus::Kind(1));
        editor.handle(&MenuAction::SelectUp);
        assert_eq!(editor.focus, Focus::Kind(0));
        assert_eq!(editor.draft.terms, terms);
    }

    #[test]
    fn down_from_cancel_stays_in_the_footer_row() {
        let mut editor = editor();
        editor.focus = Focus::Cancel;
        editor.handle(&MenuAction::SelectDown);
        assert_eq!(editor.focus, Focus::Cancel);
        editor.handle(&MenuAction::SelectRight);
        assert_eq!(editor.focus, Focus::Use);
    }

    #[test]
    fn controller_selects_scope_apps_and_connectors_through_choice_lists() {
        let mut editor = editor();
        editor.handle(&MenuAction::SelectRight);
        assert!(editor.is_main());
        editor.handle(&MenuAction::Activate);
        editor.handle(&MenuAction::SelectDown);
        assert!(editor.is_main());
        editor.handle(&MenuAction::Activate);
        assert!(!editor.is_main());
        editor.set_focus(Focus::Add);
        editor.handle(&MenuAction::Activate);
        assert_eq!(editor.focus, Focus::Kind(0));
        editor.handle(&MenuAction::SelectRight);
        editor.handle(&MenuAction::SelectRight);
        editor.handle(&MenuAction::Activate);
        assert_eq!(editor.picker, Some(0));
        editor.handle(&MenuAction::Activate);
        assert_eq!(editor.draft.terms[0].executable, "factorio.exe");
        editor.set_focus(Focus::Add);
        editor.handle(&MenuAction::Activate);
        assert_eq!(editor.draft.terms[1].join, TermJoin::And);
        editor.handle(&MenuAction::SelectLeft);
        assert_eq!(editor.focus, Focus::Join(1));
        editor.handle(&MenuAction::Activate);
        editor.handle(&MenuAction::SelectDown);
        assert_eq!(editor.draft.terms[1].join, TermJoin::And);
        editor.handle(&MenuAction::Activate);
        assert_eq!(editor.draft.groups(), [0..1, 1..2]);
        editor.handle(&MenuAction::Activate);
        editor.handle(&MenuAction::SelectUp);
        editor.handle(&MenuAction::Activate);
        assert_eq!(
            editor.draft.groups(),
            std::iter::once(0..2).collect::<Vec<_>>()
        );
        editor.set_focus(Focus::App(1));
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
    fn choices_only_change_values_on_confirmation_and_back_cancels_the_list() {
        let mut editor = editor();
        editor.select(1);
        editor.add();
        editor.handle(&MenuAction::Activate);
        editor.handle(&MenuAction::SelectDown);
        assert!(editor.draft.terms[0].state.is_none());
        editor.handle(&MenuAction::Back);
        assert!(editor.choice.is_none());
        assert!(!editor.cancelled);
        editor.handle(&MenuAction::Activate);
        editor.handle(&MenuAction::SelectDown);
        editor.handle(&MenuAction::Activate);
        assert_eq!(editor.draft.terms[0].state, Some(StateId::Keyboard));
        editor.handle(&MenuAction::SelectRight);
        editor.handle(&MenuAction::SelectRight);
        assert_eq!(editor.focus, Focus::App(0));
        editor.handle(&MenuAction::Activate);
        editor.handle(&MenuAction::SelectDown);
        editor.handle(&MenuAction::SelectRight);
        assert_eq!(editor.draft.terms[0].state, Some(StateId::Keyboard));
        editor.handle(&MenuAction::Activate);
        assert_eq!(editor.draft.terms[0].state, Some(StateId::Settings));
        editor.handle(&MenuAction::Activate);
        editor.handle(&MenuAction::SelectUp);
        editor.handle(&MenuAction::Back);
        assert_eq!(editor.draft.terms[0].state, Some(StateId::Settings));
        editor.handle(&MenuAction::Activate);
        editor.handle(&MenuAction::SelectUp);
        editor.handle(&MenuAction::Activate);
        assert_eq!(
            editor.draft.expression().unwrap().as_deref(),
            Some("state.keyboard")
        );
        assert!(editor.picker.is_none());
    }

    #[test]
    fn vertical_navigation_preserves_column_through_the_add_row() {
        let mut editor = editor();
        editor.select(1);
        editor.add();
        editor.add();
        editor.set_focus(Focus::App(0));
        editor.handle(&MenuAction::SelectDown);
        assert_eq!(editor.focus, Focus::App(1));
        editor.handle(&MenuAction::SelectDown);
        assert_eq!(editor.focus, Focus::Add);
        editor.handle(&MenuAction::SelectUp);
        assert_eq!(editor.focus, Focus::App(1));
        editor.set_focus(Focus::Kind(0));
        editor.handle(&MenuAction::SelectLeft);
        assert_eq!(editor.focus, Focus::Kind(0));
        editor.handle(&MenuAction::SelectDown);
        assert_eq!(editor.focus, Focus::Kind(1));
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
        editor.handle(&MenuAction::SelectRight);
        assert_eq!(editor.focus, Focus::Use);
        editor.handle(&MenuAction::SelectLeft);
        assert_eq!(editor.focus, Focus::Cancel);
        editor.handle(&MenuAction::Activate);
        assert!(editor.cancelled);
        assert!(editor.finished.is_none());
    }
}
