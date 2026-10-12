use super::menu_action::MenuAction;
use crate::config::{self, ConditionDraft, ConditionTerm, EditingScope, TermJoin};
use egui::{Button, Context, Id, TextEdit, Ui};

const DIALOG_WIDTH: f32 = 440.0;
const TERMS_HEIGHT: f32 = 170.0;
const APPS_HEIGHT: f32 = 180.0;
const GROUP_INDENT: f32 = 14.0;
const DIALOG_EDGE_INSET: f32 = 16.0;
const CONDITION_CHROME_HEIGHT: f32 = 210.0;
const APP_PICKER_CHROME_HEIGHT: f32 = 170.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Focus {
    Target,
    Name,
    Join(usize),
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
        }
    }

    fn is_main(&self) -> bool {
        self.selection == 0
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
                Focus::Negation(index) => {
                    self.draft.terms[index].negated = !self.draft.terms[index].negated
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

    fn button(&mut self, ui: &mut Ui, focus: Focus, label: &str) -> bool {
        let response = ui.add(Button::new(label).selected(self.focus == focus));
        if self.focus == focus {
            response.scroll_to_me(Some(egui::Align::Center));
        }
        if response.clicked() {
            self.focus = focus;
            return true;
        }
        false
    }

    fn draw_term(&mut self, ui: &mut Ui, index: usize) -> Option<usize> {
        let mut remove = None;
        ui.horizontal(|ui| {
            if index > 0
                && self.draft.terms[index].join == TermJoin::And
                && self.button(ui, Focus::Join(index), "AND")
            {
                self.toggle_join(index);
            }
            ui.label("App focused");
            let label = if self.draft.terms[index].negated {
                "is not"
            } else {
                "is"
            };
            if self.button(ui, Focus::Negation(index), label) {
                self.draft.terms[index].negated = !self.draft.terms[index].negated;
            }
            let executable = self.draft.terms[index].executable.clone();
            let label = if executable.is_empty() {
                "Select app…"
            } else {
                &executable
            };
            if self.button(ui, Focus::App(index), label) {
                self.open_picker(index);
            }
            if self.button(ui, Focus::Remove(index), "−") {
                remove = Some(index);
            }
        });
        remove
    }

    pub(super) fn draw(
        &mut self,
        ctx: &Context,
        style: &egui::Style,
        background: egui::Color32,
        mut footer: impl FnMut(&mut Ui),
    ) -> (bool, Option<EditingScope>) {
        let frame = egui::Frame::popup(style).fill(background);
        let response = egui::Modal::new(Id::new("config_condition_editor"))
            .frame(frame)
            .show(ctx, |ui| {
                ui.set_width(
                    DIALOG_WIDTH.min(ctx.content_rect().width() - DIALOG_EDGE_INSET * 2.0),
                );
                ui.heading("Edit settings for");
                let mut selection = self.selection;
                egui::ComboBox::from_id_salt("condition_destination")
                    .selected_text(self.selection_label())
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut selection, 0, "Main config");
                        for (index, draft) in self.choices.iter().enumerate() {
                            ui.selectable_value(&mut selection, index + 1, &draft.name);
                        }
                        ui.selectable_value(
                            &mut selection,
                            self.choices.len() + 1,
                            "New condition",
                        );
                    })
                    .response
                    .widget_info(|| {
                        egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, true, "Editing scope")
                    });
                if selection != self.selection {
                    self.select(selection);
                }
                if self.focus == Focus::Target {
                    ui.label("◀ Change editing scope ▶");
                }
                if !self.is_main() {
                    ui.horizontal(|ui| {
                        ui.label("Name");
                        let response = ui.add(
                            TextEdit::singleline(&mut self.draft.name)
                                .id(Id::new("condition_name")),
                        );
                        if self.focus == Focus::Name {
                            ui.painter().rect_stroke(response.rect, ui.visuals().widgets.inactive.corner_radius, ui.visuals().selection.stroke, egui::StrokeKind::Outside);
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
                    let mut remove = None;
                    egui::ScrollArea::vertical()
                        .id_salt("condition_terms")
                        .max_height(
                            TERMS_HEIGHT.min(
                                (ctx.content_rect().height() - CONDITION_CHROME_HEIGHT).max(32.0),
                            ),
                        )
                        .show(ui, |ui| {
                            for group in self.draft.groups() {
                                if group.start > 0
                                    && self.button(ui, Focus::Join(group.start), "OR")
                                {
                                    self.toggle_join(group.start);
                                }
                                let response =
                                    ui.indent(Id::new(("condition_group", group.start)), |ui| {
                                        for index in group.clone() {
                                            remove = self.draw_term(ui, index).or(remove);
                                        }
                                    });
                                if group.len() > 1 {
                                    let rect = response.response.rect;
                                    let x = rect.left() - GROUP_INDENT / 2.0;
                                    let stroke = ui.visuals().widgets.noninteractive.bg_stroke;
                                    ui.painter().line_segment(
                                        [
                                            egui::pos2(x + 5.0, rect.top()),
                                            egui::pos2(x, rect.top()),
                                        ],
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
                                ui.label("This condition is too large for the row editor. Its name can still be edited.");
                            } else if self.button(ui, Focus::Add, "+") {
                                self.add();
                            }
                        });
                    if let Some(index) = remove {
                        self.remove(index);
                    }
                }
                if !self.error.is_empty() {
                    ui.colored_label(ui.visuals().error_fg_color, &self.error);
                }
                ui.separator();
                ui.horizontal(|ui| {
                    if self.button(ui, Focus::Cancel, "Cancel") {
                        self.cancelled = true;
                    }
                    if self.button(ui, Focus::Use, "Use condition") {
                        self.use_condition();
                    }
                });
                footer(ui);
            });
        if response.should_close() {
            if self.picker.is_some() {
                self.picker = None;
            } else {
                self.cancelled = true;
            }
        }
        if let Some(index) = self.picker {
            self.draw_picker(ctx, index, style, background, &mut footer);
        }
        (self.cancelled, self.finished.take())
    }

    fn draw_picker(
        &mut self,
        ctx: &Context,
        index: usize,
        style: &egui::Style,
        background: egui::Color32,
        footer: &mut impl FnMut(&mut Ui),
    ) {
        let frame = egui::Frame::popup(style).fill(background);
        let response =
            egui::Modal::new(Id::new("condition_app_picker"))
                .frame(frame)
                .show(ctx, |ui| {
                    ui.set_width(
                        DIALOG_WIDTH.min(ctx.content_rect().width() - DIALOG_EDGE_INSET * 2.0),
                    );
                    ui.heading("Select app");
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
                    let apps = self.filtered_apps();
                    egui::ScrollArea::vertical()
                        .id_salt("condition_apps")
                        .max_height(APPS_HEIGHT.min(
                            (ctx.content_rect().height() - APP_PICKER_CHROME_HEIGHT).max(32.0),
                        ))
                        .show(ui, |ui| {
                            for (position, (label, executable)) in apps.iter().enumerate() {
                                let response =
                                    ui.add(Button::new(label).selected(self.app_focus == position));
                                if self.app_focus == position {
                                    response.scroll_to_me(Some(egui::Align::Center));
                                }
                                if response.clicked() {
                                    self.draft.terms[index].executable = executable.clone();
                                    self.picker = None;
                                }
                            }
                        });
                    if ui
                        .add(Button::new("Done").selected(self.app_focus == apps.len()))
                        .clicked()
                    {
                        self.picker = None;
                    }
                    footer(ui);
                });
        if response.should_close() {
            self.picker = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
