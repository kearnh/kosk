use std::collections::HashMap;

use egui::Color32;
use serde::Deserialize;

use crate::theme::{color, KeyColorGroup, KeyboardTheme};

use super::key::RawKey;
use super::layout::{KeyAppearance, KeyButton};

#[derive(Clone, Copy, Default)]
struct GroupColors {
    background: Option<Color32>,
    text: Option<Color32>,
}

#[derive(Default)]
pub(super) struct KeyColorGroups {
    source: Vec<KeyColorGroup>,
    colors: HashMap<RawKey, GroupColors>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum KeySelection {
    None,
    Shift,
    Left,
    Right,
    Dual,
}

pub(super) struct KeyStyle {
    pub(super) background: Option<Color32>,
    pub(super) text: Color32,
    pub(super) selected: bool,
}

impl KeyColorGroups {
    pub(super) fn sync(&mut self, groups: &[KeyColorGroup]) {
        if self.source == groups {
            return;
        }

        self.colors.clear();
        for group in groups {
            for spec in &group.keys {
                let key = RawKey::deserialize(serde::de::value::StrDeserializer::<
                    serde::de::value::Error,
                >::new(spec))
                .expect("string key specifiers are valid");
                let colors = self.colors.entry(key).or_default();
                if let Some(background) = group.background_color {
                    colors.background = Some(color(background));
                }
                if let Some(text) = group.text_color {
                    colors.text = Some(color(text));
                }
            }
        }
        self.source = groups.to_vec();
    }

    pub(super) fn style(
        &self,
        key: &KeyButton,
        appearance: &KeyAppearance,
        theme: &KeyboardTheme,
        selection: KeySelection,
    ) -> KeyStyle {
        let group = self
            .colors
            .get(&key.key(false))
            .copied()
            .unwrap_or_default();
        let selection_background = match selection {
            KeySelection::None => None,
            KeySelection::Shift => Some(theme.selection_background_color),
            KeySelection::Left => Some(theme.left_selection_color),
            KeySelection::Right => Some(theme.right_selection_color),
            KeySelection::Dual => Some(theme.dual_selection_color),
        }
        .map(color);
        let selected = appearance.button_color.is_none() && selection != KeySelection::None;
        let selection_text = match selection {
            KeySelection::Left => theme.left_selection_text_color,
            KeySelection::Right => theme.right_selection_text_color,
            KeySelection::Dual => theme.dual_selection_text_color,
            KeySelection::None | KeySelection::Shift => None,
        }
        .unwrap_or(theme.selection_text_color);

        KeyStyle {
            background: appearance
                .button_color
                .or(selection_background)
                .or(group.background),
            text: appearance
                .text_color
                .or_else(|| selected.then(|| color(selection_text)))
                .or(group.text)
                .unwrap_or_else(|| color(theme.inactive.text_color)),
            selected,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::keyboard::when::DisplayContext;
    use crate::theme::Theme;

    fn groups(theme: &Theme) -> KeyColorGroups {
        let mut groups = KeyColorGroups::default();
        groups.sync(&theme.keyboard.key_groups);
        groups
    }

    fn key(spec: &str) -> KeyButton {
        toml::from_str(spec).unwrap()
    }

    fn style(groups: &KeyColorGroups, theme: &Theme, key: &KeyButton) -> KeyStyle {
        groups.style(
            key,
            &key.appearance(&DisplayContext::default()),
            &theme.keyboard,
            KeySelection::None,
        )
    }

    #[test]
    fn later_groups_override_each_color_independently() {
        let theme = Theme::parse(
            "[[keyboard.key_groups]]\nkeys = ['q', 'w']\nbackground_color = [1, 2, 3, 255]\ntext_color = [4, 5, 6, 255]\n\
             [[keyboard.key_groups]]\nkeys = ['q']\nbackground_color = [7, 8, 9, 255]\n\
             [[keyboard.key_groups]]\nkeys = ['w']\ntext_color = [10, 11, 12, 255]",
        ).unwrap();
        let groups = groups(&theme);
        let q = style(&groups, &theme, &key("key = 'q'"));
        assert_eq!(q.background, Some(color([7, 8, 9, 255])));
        assert_eq!(q.text, color([4, 5, 6, 255]));
        let w = style(&groups, &theme, &key("key = 'w'"));
        assert_eq!(w.background, Some(color([1, 2, 3, 255])));
        assert_eq!(w.text, color([10, 11, 12, 255]));
        let e = style(&groups, &theme, &key("key = 'e'"));
        assert_eq!(e.background, None);
        assert_eq!(e.text, color(theme.keyboard.inactive.text_color));
        assert!(!e.selected);
    }

    #[test]
    fn matches_normal_keys_using_layout_syntax_regardless_of_label_or_shift() {
        let theme = Theme::parse(
            "[[keyboard.key_groups]]\nkeys = ['q', 'é', 'Return', 'EXIT', 'toggleShift', '\\Return', 'hello', ' ']\nbackground_color = [1, 2, 3, 255]",
        ).unwrap();
        let groups = groups(&theme);
        for spec in [
            "key = 'q'",
            "key = {normal = 'q', shift = '!' }\ndisplay = {normal = 'Enter', shift = '{icon:arrow-up}'}",
            "key = 'é'",
            "key = 'Return'\ndisplay = '{icon:arrow-elbow-down-left}'",
            "key = 'exit'\ndisplay = 'Done'",
            "key = 'toggleShift'",
            "key = '\\Return'",
            "key = 'hello'",
            "key = ' '",
        ] {
            let key = key(spec);
            for shift in [false, true] {
                let ctx = DisplayContext {
                    shift,
                    ..Default::default()
                };
                let style = groups.style(
                    &key,
                    &key.appearance(&ctx),
                    &theme.keyboard,
                    KeySelection::None,
                );
                assert_eq!(
                    style.background,
                    Some(color([1, 2, 3, 255])),
                    "{spec}, shift={shift}"
                );
            }
        }
        for spec in [
            "key = 'w'\ndisplay = 'q'",
            "key = {normal = 'w', shift = 'q'}",
        ] {
            let key = key(spec);
            let ctx = DisplayContext {
                shift: true,
                ..Default::default()
            };
            assert_eq!(
                groups
                    .style(
                        &key,
                        &key.appearance(&ctx),
                        &theme.keyboard,
                        KeySelection::None
                    )
                    .background,
                None
            );
        }
    }

    #[test]
    fn layout_colors_and_selection_fills_keep_priority() {
        let theme = Theme::parse(
            "[[keyboard.key_groups]]\nkeys = ['q']\nbackground_color = [1, 2, 3, 255]\ntext_color = [4, 5, 6, 255]",
        ).unwrap();
        let groups = groups(&theme);
        let plain = key("key = 'q'");
        let layout = key(
            "key = 'q'\ndisplay = [{text = 'Q', when = 'shift', button_color = [7, 8, 9, 255], text_color = [10, 11, 12, 255]}]",
        );
        let ctx = DisplayContext {
            shift: true,
            ..Default::default()
        };
        for (selection, background) in [
            (
                KeySelection::Shift,
                theme.keyboard.selection_background_color,
            ),
            (KeySelection::Left, theme.keyboard.left_selection_color),
            (KeySelection::Right, theme.keyboard.right_selection_color),
            (KeySelection::Dual, theme.keyboard.dual_selection_color),
        ] {
            let selected =
                groups.style(&plain, &plain.appearance(&ctx), &theme.keyboard, selection);
            assert_eq!(selected.background, Some(color(background)));
            assert_eq!(selected.text, color(theme.keyboard.selection_text_color));
            assert!(selected.selected);
            let selected = groups.style(
                &layout,
                &layout.appearance(&ctx),
                &theme.keyboard,
                selection,
            );
            assert_eq!(selected.background, Some(color([7, 8, 9, 255])));
            assert_eq!(selected.text, color([10, 11, 12, 255]));
            assert!(!selected.selected);
        }
        assert_eq!(
            style(&groups, &theme, &layout).background,
            Some(color([1, 2, 3, 255]))
        );
        let text_only = key("key = 'q'\ndisplay = [{text = 'Q', text_color = [10, 11, 12, 255]}]");
        let selected = groups.style(
            &text_only,
            &text_only.appearance(&ctx),
            &theme.keyboard,
            KeySelection::Left,
        );
        assert_eq!(
            selected.background,
            Some(color(theme.keyboard.left_selection_color))
        );
        assert_eq!(selected.text, color([10, 11, 12, 255]));
    }

    #[test]
    fn controller_highlights_use_selection_text_over_group_text() {
        let theme = Theme::parse("[keyboard]\nselection_text_color = [10, 20, 30]\n[[keyboard.key_groups]]\nkeys = ['q', 'Return']\ntext_color = [40, 50, 60]").unwrap();
        let groups = groups(&theme);
        for spec in ["key = 'q'", "key = 'Return'"] {
            let key = key(spec);
            for selection in [
                KeySelection::Shift,
                KeySelection::Left,
                KeySelection::Right,
                KeySelection::Dual,
            ] {
                let style = groups.style(
                    &key,
                    &key.appearance(&DisplayContext::default()),
                    &theme.keyboard,
                    selection,
                );
                assert_eq!(style.text, color(theme.keyboard.selection_text_color));
            }
        }
    }

    #[test]
    fn selection_text_overrides_are_independent_and_layout_fills_keep_group_text() {
        let theme = Theme::parse(
            "[colours]\nleft = [1, 2, 3]\nright = [4, 5, 6, 255]\n\
             [keyboard]\nselection_text_color = [10, 20, 30]\n\
             left_selection_text_color = 'left'\nright_selection_text_color = 'right'\n\
             dual_selection_text_color = [7, 8, 9]\n\
             [[keyboard.key_groups]]\nkeys = ['q']\ntext_color = [40, 50, 60]",
        )
        .unwrap();
        let groups = groups(&theme);
        let plain = key("key = 'q'");
        for (selection, text) in [
            (KeySelection::Shift, [10, 20, 30, 255]),
            (KeySelection::Left, [1, 2, 3, 255]),
            (KeySelection::Right, [4, 5, 6, 255]),
            (KeySelection::Dual, [7, 8, 9, 255]),
            (KeySelection::None, [40, 50, 60, 255]),
        ] {
            let style = groups.style(
                &plain,
                &plain.appearance(&DisplayContext::default()),
                &theme.keyboard,
                selection,
            );
            assert_eq!(style.text, color(text));
        }

        let layout = key("key = 'q'\ndisplay = [{text = 'Q', button_color = [100, 110, 120]}]");
        let selected = groups.style(
            &layout,
            &layout.appearance(&DisplayContext::default()),
            &theme.keyboard,
            KeySelection::Left,
        );
        assert_eq!(selected.text, color([40, 50, 60, 255]));
        assert!(!selected.selected);
    }

    #[test]
    fn changed_themes_replace_cached_groups_and_removed_colors() {
        let first = Theme::parse(
            "[[keyboard.key_groups]]\nkeys = ['q', 'w']\nbackground_color = [1, 2, 3, 255]",
        )
        .unwrap();
        let next =
            Theme::parse("[[keyboard.key_groups]]\nkeys = ['q']\ntext_color = [4, 5, 6, 255]")
                .unwrap();
        let mut groups = groups(&first);
        groups.sync(&first.keyboard.key_groups);
        assert_eq!(
            style(&groups, &first, &key("key = 'q'")).background,
            Some(color([1, 2, 3, 255]))
        );
        groups.sync(&next.keyboard.key_groups);
        let q = style(&groups, &next, &key("key = 'q'"));
        assert_eq!(q.background, None);
        assert_eq!(q.text, color([4, 5, 6, 255]));
        assert_eq!(style(&groups, &next, &key("key = 'w'")).background, None);
        groups.sync(&Theme::default().keyboard.key_groups);
        assert_eq!(
            style(&groups, &first, &key("key = 'q'")).text,
            color(first.keyboard.inactive.text_color)
        );
    }

    #[test]
    fn key_group_colors_return_and_done() {
        let theme = Theme::parse(
            "[[keyboard.key_groups]]\nkeys = ['Return', 'exit']\nbackground_color = [63, 100, 58]",
        )
        .unwrap();
        let groups = groups(&theme);
        for spec in ["key = 'Return'", "key = 'exit'\ndisplay = 'Done'"] {
            assert_eq!(
                style(&groups, &theme, &key(spec)).background,
                Some(color([63, 100, 58, 255]))
            );
        }
        assert_eq!(style(&groups, &theme, &key("key = 'q'")).background, None);
    }
}
