use crate::config::schema::{page_settings, settings};
use crate::config::Config;
use crate::config::{device_name, DevicePage, Page, Setting};
use crate::controller::ControllerKind;
use crate::state::StateId;

const MAX_VISIBLE_ROWS: usize = 8;
/// Rows kept on screen past the highlight, so the next row is visible while scrolling.
const SCROLL_OFFSET: usize = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HubEntry {
    Move,
    Mappings,
    Layouts,
    Options,
    Back,
}

impl HubEntry {
    const ALL: [HubEntry; 5] = [
        HubEntry::Move,
        HubEntry::Mappings,
        HubEntry::Layouts,
        HubEntry::Options,
        HubEntry::Back,
    ];

    fn label(self) -> &'static str {
        match self {
            HubEntry::Move => "Move window",
            HubEntry::Mappings => "Mappings",
            HubEntry::Layouts => "Layouts",
            HubEntry::Options => "Options",
            HubEntry::Back => "Back",
        }
    }

    fn index(self) -> usize {
        Self::ALL
            .iter()
            .position(|entry| *entry == self)
            .unwrap_or(0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::state) enum View {
    Hub,
    Index,
    DeviceIndex(ControllerKind),
    Page(Page),
    DevicePage(ControllerKind, DevicePage),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::state) enum OptionEntry {
    Page(Page),
    Device(ControllerKind),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ValueRef {
    Page(Page),
    Device(ControllerKind, DevicePage),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::state) struct Effect {
    pub open: Option<StateId>,
    pub persist: bool,
    pub changed: bool,
}

impl Effect {
    fn none() -> Self {
        Self {
            open: None,
            persist: false,
            changed: false,
        }
    }

    fn open(state: StateId) -> Self {
        Self {
            open: Some(state),
            persist: false,
            changed: false,
        }
    }
}

pub(in crate::state) struct DrawnRow {
    pub label: &'static str,
    pub explain: Option<&'static str>,
    pub value: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::state) enum FooterButtons {
    Activate,
    Back,
    Adjust,
    Page,
    OpenConfig,
    ShowAllControllers,
    ShowAllSettings,
}

pub(in crate::state) struct FooterHint {
    pub buttons: FooterButtons,
    pub label: &'static str,
}

pub(in crate::state) struct SettingsForm {
    view: View,
    focus: usize,
    dirty: Vec<&'static Setting>,
    show_all_controllers: bool,
    show_all_settings: bool,
}

impl SettingsForm {
    pub(in crate::state) fn new() -> Self {
        Self {
            view: View::Hub,
            focus: 0,
            dirty: Vec::new(),
            show_all_controllers: false,
            show_all_settings: false,
        }
    }

    pub(in crate::state) fn toggle_show_all(&mut self, kind: ControllerKind) {
        self.show_all_controllers = !self.show_all_controllers;
        self.ensure_focus(kind);
    }

    pub(in crate::state) fn toggle_show_more(&mut self, kind: ControllerKind) {
        match self.view {
            View::Index => self.toggle_show_all(kind),
            View::Page(_) | View::DevicePage(..) if self.has_hidden_rows() => {
                self.toggle_show_all_settings(kind);
            }
            _ => {}
        }
    }

    pub(in crate::state) fn toggle_show_all_settings(&mut self, kind: ControllerKind) {
        self.show_all_settings = !self.show_all_settings;
        self.ensure_focus(kind);
    }

    fn page_rows(&self, page: Page) -> Vec<&'static Setting> {
        let rows = page_settings(page);
        if self.show_all_settings {
            rows
        } else {
            rows.into_iter().filter(|row| !row.advanced).collect()
        }
    }

    fn has_hidden_rows(&self) -> bool {
        match self.view {
            View::Page(page) => page_settings(page).iter().any(|row| row.advanced),
            View::DevicePage(target, sheet) => page_settings(Page::Device(target, sheet))
                .iter()
                .any(|row| row.advanced),
            _ => false,
        }
    }

    pub(in crate::state) fn focus(&self) -> usize {
        self.focus
    }

    pub(in crate::state) fn on_page(&self) -> bool {
        matches!(self.view, View::Page(_) | View::DevicePage(..))
    }

    pub(in crate::state) fn set_focus(&mut self, index: usize, kind: ControllerKind) {
        self.focus = index;
        self.ensure_focus(kind);
    }

    pub(in crate::state) fn dirty_rows(&self) -> &[&'static Setting] {
        &self.dirty
    }

    pub(in crate::state) fn clear_dirty(&mut self) {
        self.dirty.clear();
    }

    pub(in crate::state) fn ensure_focus(&mut self, kind: ControllerKind) {
        let n = self.len(kind);
        if n == 0 {
            self.focus = 0;
            return;
        }
        if self.focus >= n {
            self.focus = n - 1;
        }
    }

    pub(in crate::state) fn move_focus(&mut self, delta: i32, kind: ControllerKind) {
        let n = self.len(kind);
        if n == 0 {
            return;
        }
        self.focus = (self.focus as i32 + delta).rem_euclid(n as i32) as usize;
    }

    pub(in crate::state) fn back(&mut self, kind: ControllerKind) -> Effect {
        let persist = !self.dirty.is_empty();
        let open = match self.view {
            View::Hub => Some(StateId::Keyboard),
            View::Index => {
                self.view = View::Hub;
                self.focus = HubEntry::Options.index();
                None
            }
            View::DeviceIndex(target) => {
                self.view = View::Index;
                self.focus = device_entry_index(target, kind, self.show_all_controllers);
                None
            }
            View::Page(page) => {
                self.view = View::Index;
                self.focus = page_entry_index(page, kind, self.show_all_controllers);
                None
            }
            View::DevicePage(target, sheet) => {
                self.view = View::DeviceIndex(target);
                self.focus = device_page_index(target, sheet);
                None
            }
        };
        Effect {
            open,
            persist,
            changed: false,
        }
    }

    pub(in crate::state) fn activate(&mut self, cfg: &mut Config, kind: ControllerKind) -> Effect {
        self.ensure_focus(kind);
        match self.view {
            View::Hub => match HubEntry::ALL[self.focus] {
                HubEntry::Move => Effect::open(StateId::MoveWindow),
                HubEntry::Mappings => Effect::open(StateId::Mappings),
                HubEntry::Layouts => Effect::open(StateId::SelectLayout),
                HubEntry::Options => {
                    self.view = View::Index;
                    self.focus = 0;
                    Effect::none()
                }
                HubEntry::Back => Effect::open(StateId::Keyboard),
            },
            View::Index => {
                let entry = options_entries(kind, self.show_all_controllers)[self.focus];
                match entry {
                    OptionEntry::Page(page) => {
                        self.view = View::Page(page);
                    }
                    OptionEntry::Device(target) => {
                        self.view = View::DeviceIndex(target);
                    }
                }
                self.focus = 0;
                Effect::none()
            }
            View::DeviceIndex(target) => {
                let sheet = device_pages(target)[self.focus];
                self.view = View::DevicePage(target, sheet);
                self.focus = 0;
                Effect::none()
            }
            View::Page(_) | View::DevicePage(..) => {
                let Some(setting) = self.focused_edit(kind) else {
                    return Effect::none();
                };
                if !setting.is_toggle() {
                    return Effect::none();
                }
                let changed = self.nudge(cfg, 1, kind);
                Effect {
                    open: None,
                    persist: false,
                    changed,
                }
            }
        }
    }

    pub(in crate::state) fn shift_page(&mut self, dir: i32, kind: ControllerKind) -> Effect {
        let current = match self.view {
            View::Page(page) => ValueRef::Page(page),
            View::DevicePage(target, sheet) => ValueRef::Device(target, sheet),
            _ => return Effect::none(),
        };
        let persist = !self.dirty.is_empty();
        let sequence = value_sequence(kind, self.show_all_controllers);
        let n = sequence.len() as i32;
        let position = sequence.iter().position(|v| *v == current).unwrap_or(0);
        let next = sequence[(position as i32 + dir).rem_euclid(n) as usize];
        match next {
            ValueRef::Page(page) => self.view = View::Page(page),
            ValueRef::Device(target, sheet) => self.view = View::DevicePage(target, sheet),
        }
        self.focus = 0;
        Effect {
            open: None,
            persist,
            changed: false,
        }
    }

    pub(in crate::state) fn nudge(
        &mut self,
        cfg: &mut Config,
        dir: i32,
        kind: ControllerKind,
    ) -> bool {
        self.ensure_focus(kind);
        let Some(setting) = self.focused_edit(kind) else {
            return false;
        };
        if !setting.nudge(cfg, dir) {
            return false;
        }
        if !self.dirty.iter().any(|row| row.key == setting.key) {
            self.dirty.push(setting);
        }
        true
    }

    pub(in crate::state) fn title(&self) -> String {
        match self.view {
            View::Hub => "Settings".to_owned(),
            View::Index => "Options".to_owned(),
            View::DeviceIndex(target) => device_name(target).to_owned(),
            View::Page(page) => page.title(),
            View::DevicePage(target, sheet) => Page::Device(target, sheet).title(),
        }
    }

    pub(in crate::state) fn footer(&self, kind: ControllerKind) -> Vec<FooterHint> {
        match self.view {
            View::Hub => vec![
                FooterHint {
                    buttons: FooterButtons::Activate,
                    label: "open",
                },
                FooterHint {
                    buttons: FooterButtons::Back,
                    label: "keyboard",
                },
            ],
            View::Index => vec![
                FooterHint {
                    buttons: FooterButtons::Activate,
                    label: "open",
                },
                FooterHint {
                    buttons: FooterButtons::Back,
                    label: "back",
                },
                FooterHint {
                    buttons: FooterButtons::ShowAllControllers,
                    label: if self.show_all_controllers {
                        "hide other controllers"
                    } else {
                        "show other controllers"
                    },
                },
                FooterHint {
                    buttons: FooterButtons::OpenConfig,
                    label: "open config",
                },
            ],
            View::DeviceIndex(_) => vec![
                FooterHint {
                    buttons: FooterButtons::Activate,
                    label: "open",
                },
                FooterHint {
                    buttons: FooterButtons::Back,
                    label: "back",
                },
            ],
            View::Page(_) | View::DevicePage(..) => {
                let mut hints = Vec::new();
                if self
                    .focused_edit(kind)
                    .is_some_and(|setting| setting.is_toggle())
                {
                    hints.push(FooterHint {
                        buttons: FooterButtons::Activate,
                        label: "toggle",
                    });
                }
                hints.push(FooterHint {
                    buttons: FooterButtons::Adjust,
                    label: "change",
                });
                hints.push(FooterHint {
                    buttons: FooterButtons::Page,
                    label: "page",
                });
                hints.push(FooterHint {
                    buttons: FooterButtons::Back,
                    label: "back",
                });
                if self.has_hidden_rows() {
                    hints.push(FooterHint {
                        buttons: FooterButtons::ShowAllSettings,
                        label: if self.show_all_settings {
                            "hide extra settings"
                        } else {
                            "show all settings"
                        },
                    });
                }
                hints
            }
        }
    }

    pub(in crate::state) fn drawn(&self, cfg: &Config, kind: ControllerKind) -> Vec<DrawnRow> {
        match self.view {
            View::Hub => HubEntry::ALL
                .iter()
                .map(|entry| DrawnRow {
                    label: entry.label(),
                    explain: None,
                    value: None,
                })
                .collect(),
            View::Index => options_entries(kind, self.show_all_controllers)
                .iter()
                .map(|entry| DrawnRow {
                    label: entry.title(),
                    explain: None,
                    value: None,
                })
                .collect(),
            View::DeviceIndex(target) => device_pages(target)
                .iter()
                .map(|sheet| DrawnRow {
                    label: sheet.title_suffix(),
                    explain: None,
                    value: None,
                })
                .collect(),
            View::Page(page) => self
                .page_rows(page)
                .into_iter()
                .map(|setting| DrawnRow {
                    label: setting.label,
                    explain: Some(setting.explain),
                    value: Some(setting.format(cfg)),
                })
                .collect(),
            View::DevicePage(target, sheet) => self
                .page_rows(Page::Device(target, sheet))
                .into_iter()
                .map(|setting| DrawnRow {
                    label: setting.label,
                    explain: Some(setting.explain),
                    value: Some(setting.format(cfg)),
                })
                .collect(),
        }
    }

    fn len(&self, kind: ControllerKind) -> usize {
        match self.view {
            View::Hub => HubEntry::ALL.len(),
            View::Index => options_entries(kind, self.show_all_controllers).len(),
            View::DeviceIndex(target) => device_pages(target).len(),
            View::Page(page) => self.page_rows(page).len(),
            View::DevicePage(target, sheet) => self.page_rows(Page::Device(target, sheet)).len(),
        }
    }

    fn focused_edit(&self, kind: ControllerKind) -> Option<&'static Setting> {
        match self.view {
            View::Page(page) => self.page_rows(page).get(self.focus).copied(),
            View::DevicePage(target, sheet) => self
                .page_rows(Page::Device(target, sheet))
                .get(self.focus)
                .copied(),
            _ => {
                let _ = kind;
                None
            }
        }
    }

    pub(in crate::state) fn visible_range(&self, kind: ControllerKind) -> (usize, usize) {
        let total = self.len(kind);
        if total <= MAX_VISIBLE_ROWS {
            return (0, total);
        }
        let max_start = total - MAX_VISIBLE_ROWS;
        let anchor = MAX_VISIBLE_ROWS - 1 - SCROLL_OFFSET.min(MAX_VISIBLE_ROWS - 1);
        let start = self.focus.saturating_sub(anchor).min(max_start);
        (start, start + MAX_VISIBLE_ROWS)
    }

    pub(in crate::state) fn scroll_counts(&self, kind: ControllerKind) -> (usize, usize) {
        let (start, end) = self.visible_range(kind);
        (start, self.len(kind).saturating_sub(end))
    }
}

impl OptionEntry {
    fn title(self) -> &'static str {
        match self {
            OptionEntry::Page(page) => match page {
                Page::Suggestions => "Suggestions",
                Page::Overlay => "Overlay",
                Page::Typing => "Typing",
                Page::Debug => "Debug",
                Page::Device(..) => "?",
            },
            OptionEntry::Device(target) => device_name(target),
        }
    }
}

fn canonical_device(kind: ControllerKind) -> ControllerKind {
    match crate::config::resolved_controller(kind) {
        ControllerKind::Ps4 => ControllerKind::Ps4,
        _ => ControllerKind::Sc2,
    }
}

fn visible_devices(kind: ControllerKind, show_all: bool) -> Vec<ControllerKind> {
    if show_all {
        vec![ControllerKind::Sc2, ControllerKind::Ps4]
    } else {
        vec![canonical_device(kind)]
    }
}

/// Device sheets that actually hold settings for this family.
fn device_pages(target: ControllerKind) -> Vec<DevicePage> {
    [DevicePage::Pads, DevicePage::Stick, DevicePage::Triggers]
        .into_iter()
        .filter(|sheet| {
            settings()
                .iter()
                .any(|setting| setting.page == Page::Device(target, *sheet))
        })
        .collect()
}

fn options_entries(kind: ControllerKind, show_all: bool) -> Vec<OptionEntry> {
    let mut entries = vec![
        OptionEntry::Page(Page::Suggestions),
        OptionEntry::Page(Page::Overlay),
        OptionEntry::Page(Page::Typing),
    ];
    for target in visible_devices(kind, show_all) {
        entries.push(OptionEntry::Device(target));
    }
    entries.push(OptionEntry::Page(Page::Debug));
    entries
}

fn value_sequence(kind: ControllerKind, show_all: bool) -> Vec<ValueRef> {
    let mut sequence = Vec::new();
    for entry in options_entries(kind, show_all) {
        match entry {
            OptionEntry::Page(page) => sequence.push(ValueRef::Page(page)),
            OptionEntry::Device(target) => {
                for sheet in device_pages(target) {
                    sequence.push(ValueRef::Device(target, sheet));
                }
            }
        }
    }
    sequence
}

fn page_entry_index(page: Page, kind: ControllerKind, show_all: bool) -> usize {
    options_entries(kind, show_all)
        .iter()
        .position(|entry| *entry == OptionEntry::Page(page))
        .unwrap_or(0)
}

fn device_entry_index(target: ControllerKind, kind: ControllerKind, show_all: bool) -> usize {
    options_entries(kind, show_all)
        .iter()
        .position(|entry| *entry == OptionEntry::Device(target))
        .unwrap_or(0)
}

fn device_page_index(target: ControllerKind, sheet: DevicePage) -> usize {
    device_pages(target)
        .iter()
        .position(|page| *page == sheet)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::schema::setting_for_key;

    fn sample() -> Config {
        toml::from_str(
            r#"
            layouts = { main = "kb.toml" }
            keyboard_opacity = 1.0
            ui_opacity = 0.4
            "#,
        )
        .unwrap()
    }

    /// Position of a setting on its page (all rows visible).
    fn row_position(key: &str) -> usize {
        let setting = setting_for_key(key).unwrap();
        let mut form = SettingsForm::new();
        form.show_all_settings = true;
        form.page_rows(setting.page)
            .iter()
            .position(|row| row.key == key)
            .unwrap()
    }

    #[test]
    fn opacity_clamps_at_both_ends() {
        let mut form = SettingsForm::new();
        form.view = View::Page(Page::Overlay);
        form.focus = row_position("keyboard_opacity");
        let mut cfg = sample();

        for _ in 0..30 {
            form.nudge(&mut cfg, 1, ControllerKind::Sc2);
        }
        assert!((cfg.keyboard_opacity - 1.0).abs() < f32::EPSILON);
        assert!(!form.nudge(&mut cfg, 1, ControllerKind::Sc2));

        for _ in 0..30 {
            form.nudge(&mut cfg, -1, ControllerKind::Sc2);
        }
        assert!((cfg.keyboard_opacity - 0.2).abs() < 0.001);
        assert!(!form.nudge(&mut cfg, -1, ControllerKind::Sc2));

        cfg.keyboard_opacity = 0.3;
        assert!(form.nudge(&mut cfg, 1, ControllerKind::Sc2));
        assert!((cfg.keyboard_opacity - 0.35).abs() < 0.001);
    }

    #[test]
    fn highlight_at_rest_wraps() {
        let mut form = SettingsForm::new();
        form.view = View::Page(Page::Suggestions);
        form.focus = row_position("completion.preselect");
        let mut cfg = sample();
        cfg.completion.preselect = crate::completion::settings::Preselect::None;

        assert!(form.nudge(&mut cfg, 1, ControllerKind::Sc2));
        assert_eq!(
            cfg.completion.preselect,
            crate::completion::settings::Preselect::First
        );
        assert!(form.nudge(&mut cfg, 1, ControllerKind::Sc2));
        assert_eq!(
            cfg.completion.preselect,
            crate::completion::settings::Preselect::None
        );
        assert!(form.nudge(&mut cfg, -1, ControllerKind::Sc2));
        assert_eq!(
            cfg.completion.preselect,
            crate::completion::settings::Preselect::First
        );
    }

    #[test]
    fn toggle_flips_either_direction() {
        let mut form = SettingsForm::new();
        form.view = View::Page(Page::Suggestions);
        form.focus = row_position("completion.enabled");
        let mut cfg = sample();
        cfg.completion.enabled = true;

        assert!(form.nudge(&mut cfg, 1, ControllerKind::Sc2));
        assert!(!cfg.completion.enabled);
        assert!(form.nudge(&mut cfg, -1, ControllerKind::Sc2));
        assert!(cfg.completion.enabled);
    }

    #[test]
    fn back_walks_hub_index_page() {
        let mut form = SettingsForm::new();
        let leave = form.back(ControllerKind::Sc2);
        assert_eq!(leave.open, Some(StateId::Keyboard));
        assert!(!leave.persist);
        assert_eq!(form.view, View::Hub);

        form.focus = HubEntry::Options.index();
        let opened = form.activate(&mut sample(), ControllerKind::Sc2);
        assert!(opened.open.is_none());
        assert_eq!(form.view, View::Index);
        assert_eq!(form.focus, 0);
        assert!(form
            .footer(ControllerKind::Sc2)
            .iter()
            .any(|hint| hint.buttons == FooterButtons::OpenConfig && hint.label == "open config"));
        assert!(form
            .footer(ControllerKind::Sc2)
            .iter()
            .any(|hint| hint.buttons == FooterButtons::ShowAllControllers
                && hint.label == "show other controllers"));
        form.toggle_show_all(ControllerKind::Sc2);
        assert!(form
            .footer(ControllerKind::Sc2)
            .iter()
            .any(|hint| hint.buttons == FooterButtons::ShowAllControllers
                && hint.label == "hide other controllers"));
        form.toggle_show_all(ControllerKind::Sc2);

        let page = form.activate(&mut sample(), ControllerKind::Sc2);
        assert!(!page.persist);
        assert_eq!(form.view, View::Page(Page::Suggestions));

        form.dirty
            .push(setting_for_key("completion.enabled").unwrap());
        let back_page = form.back(ControllerKind::Sc2);
        assert!(back_page.persist);
        assert!(back_page.open.is_none());
        assert_eq!(form.view, View::Index);
        assert_eq!(form.focus, 0);

        let back_index = form.back(ControllerKind::Sc2);
        assert!(back_index.persist);
        assert_eq!(form.view, View::Hub);
        assert_eq!(form.focus, HubEntry::Options.index());
    }

    #[test]
    fn shoulders_change_page_only_while_one_is_open() {
        let mut form = SettingsForm::new();
        let idle = form.shift_page(1, ControllerKind::Sc2);
        assert!(!idle.persist);
        assert_eq!(form.view, View::Hub);

        form.view = View::Page(Page::Debug);
        form.dirty
            .push(setting_for_key("debug.show_hitboxes").unwrap());
        let wrapped = form.shift_page(1, ControllerKind::Sc2);
        assert!(wrapped.persist);
        assert_eq!(form.view, View::Page(Page::Suggestions));
        assert_eq!(form.focus, 0);

        form.clear_dirty();
        let back = form.shift_page(-1, ControllerKind::Sc2);
        assert!(!back.persist);
        assert_eq!(form.view, View::Page(Page::Debug));
    }

    #[test]
    fn options_lists_only_connected_device_until_toggled() {
        let mut form = SettingsForm::new();
        form.view = View::Index;

        let connected = form.drawn(&sample(), ControllerKind::Sc2);
        let labels: Vec<&str> = connected.iter().map(|row| row.label).collect();
        assert!(labels.contains(&"Steam Controller"));
        assert!(!labels.contains(&"DualShock 4"));

        form.toggle_show_all(ControllerKind::Sc2);
        let both = form.drawn(&sample(), ControllerKind::Sc2);
        let labels: Vec<&str> = both.iter().map(|row| row.label).collect();
        assert!(labels.contains(&"Steam Controller"));
        assert!(labels.contains(&"DualShock 4"));

        let mut ps4_only = SettingsForm::new();
        ps4_only.view = View::Index;
        let rows = ps4_only.drawn(&sample(), ControllerKind::Ps4);
        let labels: Vec<&str> = rows.iter().map(|row| row.label).collect();
        assert!(labels.contains(&"DualShock 4"));
        assert!(!labels.contains(&"Steam Controller"));
    }

    #[test]
    fn device_index_sizes_match_family() {
        let mut form = SettingsForm::new();
        form.view = View::DeviceIndex(ControllerKind::Sc2);
        assert_eq!(form.len(ControllerKind::Sc2), 3);
        form.view = View::DeviceIndex(ControllerKind::Ps4);
        assert_eq!(form.len(ControllerKind::Ps4), 2);
        assert_eq!(
            device_pages(ControllerKind::Ps4),
            vec![DevicePage::Stick, DevicePage::Triggers]
        );
        assert_eq!(
            device_pages(ControllerKind::Sc2),
            vec![DevicePage::Pads, DevicePage::Stick, DevicePage::Triggers]
        );
    }

    #[test]
    fn stick_edits_follow_target_device() {
        let mut form = SettingsForm::new();
        form.view = View::DevicePage(ControllerKind::Ps4, DevicePage::Stick);
        form.focus = 0;
        let mut cfg = sample();
        cfg.ps4.stick.scale_x = 1.0;
        cfg.sc2.stick.scale_x = 1.0;

        assert!(form.nudge(&mut cfg, 1, ControllerKind::Sc2));
        assert!((cfg.ps4.stick.scale_x - 1.1).abs() < 0.001);
        assert!((cfg.sc2.stick.scale_x - 1.0).abs() < f32::EPSILON);
        assert_eq!(
            form.dirty_rows()
                .iter()
                .map(|row| row.key)
                .collect::<Vec<_>>(),
            ["ps4.stick.scale_x"]
        );
    }

    #[test]
    fn back_walks_device_value_to_index_to_options() {
        let mut form = SettingsForm::new();
        form.view = View::DevicePage(ControllerKind::Sc2, DevicePage::Stick);
        form.focus = 0;

        let to_index = form.back(ControllerKind::Sc2);
        assert!(!to_index.persist);
        assert_eq!(form.view, View::DeviceIndex(ControllerKind::Sc2));
        assert_eq!(form.focus, 1);

        let _to_options = form.back(ControllerKind::Sc2);
        assert_eq!(form.view, View::Index);
        assert_eq!(
            form.focus,
            device_entry_index(
                ControllerKind::Sc2,
                ControllerKind::Sc2,
                form.show_all_controllers
            )
        );
    }

    #[test]
    fn ps4_device_page_is_only_the_triggers() {
        let mut form = SettingsForm::new();
        form.view = View::DevicePage(ControllerKind::Ps4, DevicePage::Triggers);
        assert_eq!(form.len(ControllerKind::Ps4), 2);
        let rows = form.drawn(&sample(), ControllerKind::Ps4);
        assert_eq!(rows[0].label, "Left trigger");
        assert_eq!(rows[1].label, "Right trigger");
    }

    #[test]
    fn pads_page_holds_pad_feel() {
        let mut form = SettingsForm::new();
        form.view = View::DevicePage(ControllerKind::Sc2, DevicePage::Pads);
        assert_eq!(form.len(ControllerKind::Sc2), 8);
        form.toggle_show_all_settings(ControllerKind::Sc2);
        assert_eq!(form.len(ControllerKind::Sc2), 10);
        let rows = form.drawn(&sample(), ControllerKind::Sc2);
        let labels: Vec<&str> = rows.iter().map(|row| row.label).collect();
        assert!(labels.contains(&"Thumb rest"));
        assert!(labels.contains(&"Pad click"));
        assert!(labels.contains(&"Stretch limit"));
        assert!(labels.contains(&"Touch settle time"));
    }

    #[test]
    fn show_more_toggles_what_is_visible() {
        let mut form = SettingsForm::new();

        form.view = View::Index;
        form.toggle_show_more(ControllerKind::Sc2);
        assert!(form.show_all_controllers);
        assert!(!form.show_all_settings);

        form.view = View::Page(Page::Suggestions);
        form.toggle_show_more(ControllerKind::Sc2);
        assert!(form.show_all_settings);

        form.view = View::Page(Page::Typing);
        form.show_all_settings = false;
        form.toggle_show_more(ControllerKind::Sc2);
        assert!(!form.show_all_settings);

        form.view = View::Hub;
        form.show_all_controllers = false;
        form.toggle_show_more(ControllerKind::Sc2);
        assert!(!form.show_all_controllers);
    }

    #[test]
    fn curated_rows_show_by_default() {
        let mut form = SettingsForm::new();
        form.view = View::Page(Page::Suggestions);
        assert_eq!(form.len(ControllerKind::Sc2), 8);
        assert_eq!(form.scroll_counts(ControllerKind::Sc2), (0, 0));
        assert!(form.has_hidden_rows());
        assert!(form
            .footer(ControllerKind::Sc2)
            .iter()
            .any(|hint| hint.buttons == FooterButtons::ShowAllSettings
                && hint.label == "show all settings"));

        form.toggle_show_all_settings(ControllerKind::Sc2);
        assert!(form.len(ControllerKind::Sc2) > 8);
        assert!(form
            .footer(ControllerKind::Sc2)
            .iter()
            .any(|hint| hint.buttons == FooterButtons::ShowAllSettings
                && hint.label == "hide extra settings"));

        form.view = View::Page(Page::Typing);
        assert!(!form.has_hidden_rows());
        assert!(form
            .footer(ControllerKind::Sc2)
            .iter()
            .all(|hint| hint.buttons != FooterButtons::ShowAllSettings));
    }

    #[test]
    fn long_lists_scroll_eight_at_a_time() {
        let mut form = SettingsForm::new();
        form.show_all_settings = true;
        form.view = View::Page(Page::Suggestions);
        let total = form.len(ControllerKind::Sc2);
        assert!(total > 8);

        form.focus = 0;
        assert_eq!(form.visible_range(ControllerKind::Sc2), (0, 8));
        assert_eq!(form.scroll_counts(ControllerKind::Sc2).0, 0);
        assert_eq!(form.scroll_counts(ControllerKind::Sc2).1, total - 8);

        form.focus = total - 1;
        assert_eq!(form.visible_range(ControllerKind::Sc2), (total - 8, total));
        assert_eq!(form.scroll_counts(ControllerKind::Sc2).1, 0);
        assert_eq!(form.scroll_counts(ControllerKind::Sc2).0, total - 8);

        form.focus = 9;
        let (start, end) = form.visible_range(ControllerKind::Sc2);
        assert!(start <= 9 && 9 < end);
        assert_eq!(end - start, 8);
        assert!(end - 1 - 9 >= SCROLL_OFFSET);
    }

    #[test]
    fn short_lists_show_everything() {
        let mut form = SettingsForm::new();
        form.view = View::Page(Page::Typing);
        assert_eq!(form.visible_range(ControllerKind::Sc2), (0, 2));
        assert_eq!(form.scroll_counts(ControllerKind::Sc2), (0, 0));
    }

    #[test]
    fn new_rows_round_trip() {
        let mut form = SettingsForm::new();
        form.show_all_settings = true;
        let mut cfg = sample();

        form.view = View::Page(Page::Suggestions);
        form.focus = row_position("completion.max_suggestions");
        let before = cfg.completion.max_suggestions;
        assert!(form.nudge(&mut cfg, 1, ControllerKind::Sc2));
        assert_eq!(cfg.completion.max_suggestions, before + 1);

        form.focus = row_position("completion.backend");
        assert!(form.nudge(&mut cfg, 1, ControllerKind::Sc2));
        assert_ne!(cfg.completion.backend, sample().completion.backend);

        form.focus = row_position("completion.learn_on_accept");
        let enabled = cfg.completion.learn_on_accept;
        assert!(form.nudge(&mut cfg, 1, ControllerKind::Sc2));
        assert_eq!(cfg.completion.learn_on_accept, !enabled);

        form.view = View::DevicePage(ControllerKind::Sc2, DevicePage::Pads);
        form.focus = form
            .page_rows(Page::Device(ControllerKind::Sc2, DevicePage::Pads))
            .iter()
            .position(|row| row.key == "sc2.pad_origin_settle_ms")
            .unwrap();
        assert!(form.nudge(&mut cfg, 1, ControllerKind::Sc2));
        assert_eq!(cfg.sc2.pad_origin_settle_ms, 25);

        form.view = View::Page(Page::Overlay);
        form.focus = row_position("battery.draw_button");
        assert!(form.nudge(&mut cfg, 1, ControllerKind::Sc2));
        assert!(cfg.battery.draw_button);
    }

    #[test]
    fn device_page_titles_carry_device() {
        assert_eq!(
            Page::Device(ControllerKind::Sc2, DevicePage::Pads).title(),
            "Steam Controller Pads"
        );
        assert_eq!(
            Page::Device(ControllerKind::Sc2, DevicePage::Stick).title(),
            "Steam Controller Stick"
        );
        assert_eq!(
            Page::Device(ControllerKind::Sc2, DevicePage::Triggers).title(),
            "Steam Controller Triggers"
        );
        assert_eq!(
            Page::Device(ControllerKind::Ps4, DevicePage::Stick).title(),
            "DualShock 4 Stick"
        );
        assert_eq!(
            Page::Device(ControllerKind::Ps4, DevicePage::Triggers).title(),
            "DualShock 4 Triggers"
        );
    }
}
