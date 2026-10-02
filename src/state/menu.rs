use crate::{
    config,
    controller::ControllerButton,
    controller::ControllerInput,
    controller::ControllerKind,
    state::{
        actions::load_bindings,
        event::{Event, EventQueue, EventSource},
        menu_action::MenuAction,
        settings_form::{Effect, FooterButtons, FooterHint, SettingsForm},
        StateId,
    },
    ui::{controller_glyph::GlyphFamily, menu_list},
};

use anyhow::Result;
use egui::{Context, RichText, Ui};
use std::sync::{Mutex, OnceLock};

use crate::controller::bindings::BindingEngine;

const PANEL_WIDTH: f32 = 240.0;

pub struct MenuState {
    form: SettingsForm,
    kind: ControllerKind,
    pending_notify: bool,
    bindings: BindingEngine<MenuAction>,
}

impl MenuState {
    pub fn new() -> Result<Self> {
        Ok(Self {
            form: SettingsForm::new(),
            kind: ControllerKind::Sc2,
            pending_notify: false,
            bindings: load_bindings(StateId::Settings)?.with_left_stick_dpad(),
        })
    }

    pub fn draw_ui(
        &mut self,
        _: &Context,
        ui: &mut Ui,
        events: &mut EventQueue,
        kind: ControllerKind,
    ) {
        self.kind = kind;
        let cfg = config::get();
        self.form.sync_themes(&cfg);
        self.form.ensure_focus(kind);
        let appearance = &cfg.theme().menus;
        let rows = self.form.drawn(&cfg, kind);
        let focus = self.form.focus();
        let focused = rows.get(focus);
        let (view_start, view_end) = self.form.visible_range(kind);
        let (above, below) = self.form.scroll_counts(kind);

        let hints = filter_open_config_hint(self.form.footer(kind), config::uses_user_config());
        menu_list::frame().show(ui, |ui| {
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    menu_list::heading(ui, &self.form.title(), appearance);
                    if above > 0 {
                        ui.label(
                            RichText::new(format!("▲ {above} more above"))
                                .small()
                                .color(crate::theme::color(appearance.muted_text_color)),
                        );
                    }
                    for (index, row) in rows
                        .iter()
                        .enumerate()
                        .skip(view_start)
                        .take(view_end - view_start)
                    {
                        if menu_list::row(
                            ui,
                            &row.label,
                            row.value.as_deref(),
                            index == focus,
                            appearance,
                        )
                        .clicked()
                        {
                            let already = index == focus;
                            self.form.set_focus(index, kind);
                            if !self.form.on_page() || already {
                                self.commit(events, &EventSource::MouseClick, |form, cfg, kind| {
                                    form.activate(cfg, kind)
                                });
                            }
                        }
                    }
                    if below > 0 {
                        ui.label(
                            RichText::new(format!("▼ {below} more below"))
                                .small()
                                .color(crate::theme::color(appearance.muted_text_color)),
                        );
                    }
                    ui.separator();
                    draw_footer(ui, &self.bindings, &hints, GlyphFamily::from_kind(kind));
                });

                if let Some(row) = focused
                    && let Some(explain) = row.explain
                {
                    ui.vertical(|ui| {
                        ui.set_min_width(PANEL_WIDTH);
                        ui.set_max_width(PANEL_WIDTH);
                        ui.label(
                            RichText::new(&row.label)
                                .strong()
                                .color(crate::theme::color(appearance.heading_color)),
                        );
                        ui.add_space(6.0);
                        ui.label(explain);
                    });
                }
            });
        });
    }

    fn commit(
        &mut self,
        events: &mut EventQueue,
        source: &EventSource,
        run: impl FnOnce(&mut SettingsForm, &mut config::Config, ControllerKind) -> Effect,
    ) {
        let kind = self.kind;
        let mut cfg = config::get();
        self.form.sync_themes(&cfg);
        let effect = run(&mut self.form, &mut cfg, kind);
        if effect.changed {
            config::replace_live(cfg);
        }
        if effect.persist {
            self.write_settings();
        }
        if let Some(state) = effect.open {
            let _ = events.push(Event::ChangeState(state), source);
        }
    }

    fn write_settings(&mut self) {
        let rows = self.form.dirty_rows().to_vec();
        if rows.is_empty() {
            return;
        }
        let live = config::get();
        if let Err(e) = config::persist_settings(&live, |disk| {
            for setting in &rows {
                setting.copy(disk, &live);
            }
        }) {
            eprintln!("settings save: {e:#}");
            crate::user_notify::notify(crate::user_notify::Notice::settings_save_failed());
            return;
        }
        self.form.clear_dirty();
        self.pending_notify = true;
    }

    pub fn take_pending_notify(&mut self) -> bool {
        let pending = self.pending_notify;
        self.pending_notify = false;
        pending
    }

    fn do_action(
        &mut self,
        action: &MenuAction,
        events: &mut EventQueue,
        source: &EventSource,
    ) -> Result<()> {
        use MenuAction::*;
        self.form.ensure_focus(self.kind);
        match action {
            SelectUp => self.form.move_focus(-1, self.kind),
            SelectDown => self.form.move_focus(1, self.kind),
            SelectLeft => self.commit(events, source, |form, cfg, kind| Effect {
                open: None,
                persist: false,
                changed: form.nudge(cfg, -1, kind),
            }),
            SelectRight => self.commit(events, source, |form, cfg, kind| Effect {
                open: None,
                persist: false,
                changed: form.nudge(cfg, 1, kind),
            }),
            Activate => self.commit(events, source, |form, cfg, kind| form.activate(cfg, kind)),
            PagePrev => self.commit(events, source, |form, _, kind| form.shift_page(-1, kind)),
            PageNext => self.commit(events, source, |form, _, kind| form.shift_page(1, kind)),
            Back => self.commit(events, source, |form, _, kind| form.back(kind)),
            OpenConfig => {
                config::open_user_config_in_editor();
            }
            ToggleShowMore => {
                self.form.toggle_show_more(self.kind);
            }
            SwitchState(state) => {
                self.write_settings();
                let _ = events.push(Event::ChangeState(*state), source);
            }
        }
        Ok(())
    }

    pub fn reset_controller_input(&mut self, holdover: Option<&dyn ControllerInput>) {
        self.bindings.reset(holdover);
    }

    pub fn handle_controller_input(
        &mut self,
        _: &Context,
        input: &dyn ControllerInput,
        events: &mut EventQueue,
        kind: ControllerKind,
    ) -> Result<()> {
        self.kind = kind;
        for (binding, action) in self
            .bindings
            .evaluate(input, &crate::when::WhenContext::default())
        {
            let src = EventSource::Controller(binding);
            self.do_action(&action, events, &src)?;
        }

        Ok(())
    }

    fn reload_from_config(&mut self) -> Result<()> {
        self.bindings = load_bindings(StateId::Settings)?.with_left_stick_dpad();
        self.form.sync_themes(&config::get());
        Ok(())
    }
}

fn draw_footer(
    ui: &mut Ui,
    bindings: &BindingEngine<MenuAction>,
    hints: &[FooterHint],
    family: GlyphFamily,
) {
    let (primary, secondary): (Vec<_>, Vec<_>) = hints.iter().partition(|hint| {
        !matches!(
            hint.buttons,
            FooterButtons::ShowAllControllers | FooterButtons::ShowAllSettings
        )
    });

    draw_hint_line(ui, bindings, &primary, family);
    draw_hint_line(ui, bindings, &secondary, family);
}

fn draw_hint_line(
    ui: &mut Ui,
    bindings: &BindingEngine<MenuAction>,
    hints: &[&FooterHint],
    family: GlyphFamily,
) {
    menu_list::hints(
        ui,
        family,
        hints
            .iter()
            .map(|hint| (buttons_for(bindings, hint.buttons), hint.label)),
    );
}

fn buttons_for(
    bindings: &BindingEngine<MenuAction>,
    buttons: FooterButtons,
) -> Vec<ControllerButton> {
    match buttons {
        FooterButtons::Activate => {
            bindings.buttons_matching(|action| matches!(action, MenuAction::Activate))
        }
        FooterButtons::Back => {
            bindings.buttons_matching(|action| matches!(action, MenuAction::Back))
        }
        FooterButtons::Adjust => {
            let mut found =
                bindings.buttons_matching(|action| matches!(action, MenuAction::SelectLeft));
            for button in
                bindings.buttons_matching(|action| matches!(action, MenuAction::SelectRight))
            {
                if !found.contains(&button) {
                    found.push(button);
                }
            }
            found
        }
        FooterButtons::Page => {
            let mut found =
                bindings.buttons_matching(|action| matches!(action, MenuAction::PagePrev));
            for button in bindings.buttons_matching(|action| matches!(action, MenuAction::PageNext))
            {
                if !found.contains(&button) {
                    found.push(button);
                }
            }
            found
        }
        FooterButtons::OpenConfig => {
            bindings.buttons_matching(|action| matches!(action, MenuAction::OpenConfig))
        }
        FooterButtons::ShowAllControllers | FooterButtons::ShowAllSettings => {
            bindings.buttons_matching(|action| matches!(action, MenuAction::ToggleShowMore))
        }
    }
}

static MENU: OnceLock<Mutex<MenuState>> = OnceLock::new();

pub fn init() -> Result<()> {
    let menu = MenuState::new()?;
    MENU.set(Mutex::new(menu))
        .map_err(|_| anyhow::anyhow!("menu state already initialized"))?;

    crate::config::on_changed(|| {
        let result = with_mut(|m| m.reload_from_config());
        if let Err(e) = result {
            eprintln!("Failed to reload menu from config: {}", e);
        }
    })?;

    Ok(())
}

pub(crate) fn with_mut<R>(f: impl FnOnce(&mut MenuState) -> R) -> R {
    let mut guard = MENU
        .get()
        .expect("menu state not initialized")
        .lock()
        .unwrap();
    f(&mut guard)
}

pub(crate) fn flush_pending_notify() {
    let pending = with_mut(|m| m.take_pending_notify());
    if pending {
        config::notify_changed();
    }
}

fn filter_open_config_hint(mut hints: Vec<FooterHint>, uses_user_config: bool) -> Vec<FooterHint> {
    if !uses_user_config {
        hints.retain(|hint| hint.buttons != FooterButtons::OpenConfig);
    }
    hints
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMUM_MENU_TEXT_CONTRAST: f32 = 4.5;

    fn row_text_colors(theme: &crate::theme::Theme, selected: bool) -> Vec<egui::Color32> {
        let ctx = Context::default();
        ctx.set_visuals(theme.visuals());
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            menu_list::row(ui, "Label", Some("Value"), selected, &theme.menus);
        });
        output.textures_delta.clear();
        if selected {
            assert!(output.shapes.iter().any(|shape| matches!(
                &shape.shape,
                egui::Shape::Rect(rect)
                    if rect.fill == crate::theme::color(theme.selection_background_color)
            )));
        }
        output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some(text.galley.job.sections[0].format.color),
                _ => None,
            })
            .collect()
    }

    fn contrast_ratio(first: egui::Color32, second: egui::Color32) -> f32 {
        const SRGB_LINEAR_THRESHOLD: f32 = 0.04045;
        const SRGB_LINEAR_SCALE: f32 = 12.92;
        const SRGB_OFFSET: f32 = 0.055;
        const SRGB_SCALE: f32 = 1.055;
        const SRGB_EXPONENT: f32 = 2.4;
        const LUMINANCE_WEIGHTS: [f32; 3] = [0.2126, 0.7152, 0.0722];
        const CONTRAST_LUMINANCE_OFFSET: f32 = 0.05;

        let luminance = |color: egui::Color32| {
            color.to_srgba_unmultiplied()[..3]
                .iter()
                .zip(LUMINANCE_WEIGHTS)
                .map(|(&channel, weight)| {
                    let channel = f32::from(channel) / f32::from(u8::MAX);
                    let linear = if channel <= SRGB_LINEAR_THRESHOLD {
                        channel / SRGB_LINEAR_SCALE
                    } else {
                        ((channel + SRGB_OFFSET) / SRGB_SCALE).powf(SRGB_EXPONENT)
                    };
                    weight * linear
                })
                .sum::<f32>()
        };
        let first = luminance(first);
        let second = luminance(second);
        (first.max(second) + CONTRAST_LUMINANCE_OFFSET)
            / (first.min(second) + CONTRAST_LUMINANCE_OFFSET)
    }

    #[test]
    fn menu_text_colors_keep_dark_and_light_rows_readable() {
        for source in [
            "name = 'Dark'\nbackground_color = [0, 0, 0]\nselection_background_color = [255, 255, 255]\n[noninteractive]\ntext_color = [255, 255, 255]\n[menus]\nselected_text_color = [0, 0, 0]",
            "name = 'Light'\nbackground_color = [255, 255, 255]\nselection_background_color = [0, 0, 0]\n[noninteractive]\ntext_color = [0, 0, 0]\n[menus]\nselected_text_color = [255, 255, 255]",
        ] {
            let document: toml::Value = toml::from_str(source).unwrap();
            let name = document["name"].as_str().unwrap();
            let theme = crate::theme::Theme::parse(source).unwrap();
            for selected in [false, true] {
                let background = crate::theme::color(if selected {
                    theme.selection_background_color
                } else {
                    theme.background_color
                });
                let colors = row_text_colors(&theme, selected);
                assert_eq!(colors.len(), 2, "{name}: label and value must be painted");
                for color in colors {
                    let contrast = contrast_ratio(color, background);
                    assert!(
                        contrast >= MINIMUM_MENU_TEXT_CONTRAST,
                        "{name}: selected={selected}, text contrast {contrast:.2}:1"
                    );
                }
            }
        }
    }

    #[test]
    fn selected_menu_text_override_preserves_unselected_text_and_omitted_defaults() {
        let source = "text_color = [40, 50, 60]\n\
                      [noninteractive]\ntext_color = [70, 80, 90]\n";
        let inherited = crate::theme::Theme::parse(source).unwrap();
        let normal_color = crate::theme::color(inherited.noninteractive.text_color);
        for selected in [false, true] {
            assert_eq!(row_text_colors(&inherited, selected), [normal_color; 2]);
        }

        let overridden = crate::theme::Theme::parse(&format!(
            "{source}[menus]\nselected_text_color = [10, 20, 30, 255]"
        ))
        .unwrap();
        assert_eq!(row_text_colors(&overridden, false), [normal_color; 2]);
        assert_eq!(
            row_text_colors(&overridden, true),
            [crate::theme::color([10, 20, 30, 255]); 2]
        );
    }

    fn test_hints() -> Vec<FooterHint> {
        vec![
            FooterHint {
                buttons: FooterButtons::Activate,
                label: "open",
            },
            FooterHint {
                buttons: FooterButtons::OpenConfig,
                label: "open config",
            },
        ]
    }

    #[test]
    fn user_config_keeps_open_config_hint() {
        let kept = filter_open_config_hint(test_hints(), true);
        assert!(kept
            .iter()
            .any(|hint| hint.buttons == FooterButtons::OpenConfig));
    }

    #[test]
    fn explicit_config_hides_open_config_hint() {
        let kept = filter_open_config_hint(test_hints(), false);
        assert_eq!(kept.len(), 1);
        assert!(kept
            .iter()
            .all(|hint| hint.buttons != FooterButtons::OpenConfig));
    }
}
