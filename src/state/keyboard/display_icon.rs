//! Parse layout `display` templates with `{icon:…}` tokens into egui `WidgetText`.

use egui::text::LayoutJob;
use egui::{Color32, FontFamily, FontId, RichText, TextFormat, WidgetText};
use egui_phosphor_icons::Icon;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum IconStyle {
    Regular,
    Bold,
    Fill,
    Light,
    Thin,
}

#[derive(Clone)]
enum Span {
    Text(String),
    /// Phosphor codepoint + style.
    Icon {
        glyph: &'static str,
        style: IconStyle,
    },
    Missing, // renders as "?"
}

/// Parsed template; independent of font size / color.
struct ParsedDisplay {
    spans: Arc<[Span]>,
    /// True when template had no `{icon:` — render as single RichText.
    plain: bool,
}

pub struct LabelCache {
    parsed: HashMap<String, Arc<ParsedDisplay>>,
    widgets: HashMap<(u64, u32, u32), WidgetText>,
}

impl LabelCache {
    pub fn new() -> Self {
        Self {
            parsed: HashMap::new(),
            widgets: HashMap::new(),
        }
    }

    pub fn clear(&mut self) {
        self.parsed.clear();
        self.widgets.clear();
    }

    /// Return cached WidgetText, parsing/building only on miss.
    pub fn get(&mut self, template: &str, font_size: f32, color: Color32) -> WidgetText {
        let key = (
            hash_template(template),
            font_size.to_bits(),
            color_u32(color),
        );
        if let Some(cached) = self.widgets.get(&key) {
            return cached.clone();
        }

        let parsed = self
            .parsed
            .entry(template.to_owned())
            .or_insert_with(|| Arc::new(parse_template(template)))
            .clone();

        let widget = build_widget(&parsed, font_size, color);
        self.widgets.insert(key, widget.clone());
        widget
    }

    #[cfg(test)]
    pub(crate) fn parsed_len(&self) -> usize {
        self.parsed.len()
    }
}

impl Default for LabelCache {
    fn default() -> Self {
        Self::new()
    }
}

/// One-shot expand (tests / callers that do not keep a cache).
#[allow(dead_code)]
pub fn expand(template: &str, font_size: f32, color: Color32) -> WidgetText {
    LabelCache::new().get(template, font_size, color)
}

fn hash_template(template: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    template.hash(&mut hasher);
    hasher.finish()
}

fn color_u32(color: Color32) -> u32 {
    let [r, g, b, a] = color.to_array();
    u32::from_le_bytes([r, g, b, a])
}

fn parse_style(style: &str) -> Option<IconStyle> {
    match style {
        "regular" => Some(IconStyle::Regular),
        "bold" => Some(IconStyle::Bold),
        "fill" => Some(IconStyle::Fill),
        "light" => Some(IconStyle::Light),
        "thin" => Some(IconStyle::Thin),
        _ => None,
    }
}

fn phosphor_family(style: IconStyle) -> FontFamily {
    let name = match style {
        IconStyle::Regular => "phosphor-regular",
        IconStyle::Bold => "phosphor-bold",
        IconStyle::Fill => "phosphor-fill",
        IconStyle::Light => "phosphor-light",
        IconStyle::Thin => "phosphor-thin",
    };
    FontFamily::Name(name.into())
}

fn parse_template(template: &str) -> ParsedDisplay {
    if !template.contains("{icon:") {
        return ParsedDisplay {
            spans: Arc::from([Span::Text(template.to_owned())]),
            plain: true,
        };
    }

    let mut spans = Vec::new();
    let bytes = template.as_bytes();
    let mut i = 0;
    let mut text_start = 0;

    while i < bytes.len() {
        // Only the prefix `{icon:` starts a token.
        if bytes[i] == b'{' && template[i..].starts_with("{icon:") {
            if i > text_start {
                spans.push(Span::Text(template[text_start..i].to_owned()));
            }

            let after_prefix = i + "{icon:".len();
            match template[after_prefix..].find('}') {
                None => {
                    // Unclosed `{icon:` — remainder is plain text.
                    spans.push(Span::Text(template[i..].to_owned()));
                    text_start = bytes.len();
                    break;
                }
                Some(rel_end) => {
                    let inner = &template[after_prefix..after_prefix + rel_end];
                    spans.push(parse_icon_inner(inner));
                    i = after_prefix + rel_end + 1;
                    text_start = i;
                    continue;
                }
            }
        }
        i += 1;
    }

    if text_start < bytes.len() {
        spans.push(Span::Text(template[text_start..].to_owned()));
    }

    if spans.is_empty() {
        spans.push(Span::Text(String::new()));
    }

    ParsedDisplay {
        spans: Arc::from(spans),
        plain: false,
    }
}

fn parse_icon_inner(inner: &str) -> Span {
    let mut parts = inner.split(':');
    let Some(name) = parts.next() else {
        return Span::Missing;
    };
    if name.is_empty() || !is_valid_icon_name(name) {
        return Span::Missing;
    }

    let style = match parts.next() {
        None => IconStyle::Regular,
        Some(s) => match parse_style(s) {
            Some(style) if parts.next().is_none() => style,
            _ => return Span::Missing,
        },
    };

    match Icon::from_name(name) {
        Some(icon) => Span::Icon {
            glyph: icon.as_str(),
            style,
        },
        None => Span::Missing,
    }
}

fn is_valid_icon_name(name: &str) -> bool {
    // `[a-z0-9]+(?:-[a-z0-9]+)*`
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
        return false;
    }
    let mut prev_hyphen = false;
    for c in chars {
        if c == '-' {
            if prev_hyphen {
                return false;
            }
            prev_hyphen = true;
            continue;
        }
        if !c.is_ascii_lowercase() && !c.is_ascii_digit() {
            return false;
        }
        prev_hyphen = false;
    }
    !prev_hyphen
}

fn build_widget(parsed: &ParsedDisplay, font_size: f32, color: Color32) -> WidgetText {
    if parsed.plain {
        let text = match parsed.spans.first() {
            Some(Span::Text(t)) => t.as_str(),
            _ => "",
        };
        return RichText::new(text).size(font_size).color(color).into();
    }

    let mut job = LayoutJob::default();
    for span in parsed.spans.iter() {
        match span {
            Span::Text(t) => {
                if t.is_empty() {
                    continue;
                }
                job.append(
                    t,
                    0.0,
                    TextFormat {
                        font_id: FontId::new(font_size, FontFamily::Proportional),
                        color,
                        ..Default::default()
                    },
                );
            }
            Span::Missing => {
                job.append(
                    "?",
                    0.0,
                    TextFormat {
                        font_id: FontId::new(font_size, FontFamily::Proportional),
                        color,
                        ..Default::default()
                    },
                );
            }
            Span::Icon { glyph, style } => {
                job.append(
                    glyph,
                    0.0,
                    TextFormat {
                        font_id: FontId::new(font_size, phosphor_family(*style)),
                        color,
                        ..Default::default()
                    },
                );
            }
        }
    }
    job.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZE: f32 = 14.0;
    const COLOR: Color32 = Color32::WHITE;

    #[test]
    fn expand_plain_done() {
        let w = expand("Done", SIZE, COLOR);
        assert_eq!(w.text(), "Done");
    }

    #[test]
    fn expand_pure_icon_fill() {
        let glyph = Icon::from_name("arrow-fat-left").unwrap().as_str();
        let w = expand("{icon:arrow-fat-left:fill}", SIZE, COLOR);
        assert_eq!(w.text(), glyph);
    }

    #[test]
    fn expand_mixed_icon_and_text() {
        let glyph = Icon::from_name("arrow-fat-left").unwrap().as_str();
        let w = expand("Go {icon:arrow-fat-left}!", SIZE, COLOR);
        assert_eq!(w.text(), format!("Go {glyph}!"));
    }

    #[test]
    fn expand_unknown_name_and_bad_style() {
        assert_eq!(expand("{icon:not-a-real-icon}", SIZE, COLOR).text(), "?");
        assert_eq!(expand("{icon:house:extra-bold}", SIZE, COLOR).text(), "?");
    }

    #[test]
    fn expand_unclosed_icon_stays_literal() {
        let w = expand("{icon:house", SIZE, COLOR);
        assert_eq!(w.text(), "{icon:house");
    }

    #[test]
    fn label_cache_reuses_parse_and_clears() {
        let mut cache = LabelCache::new();
        let t = "{icon:arrow-fat-left:fill}";
        let a = cache.get(t, SIZE, COLOR);
        let b = cache.get(t, SIZE, COLOR);
        assert_eq!(a.text(), b.text());
        assert_eq!(cache.parsed_len(), 1);

        let other = Color32::from_rgb(255, 0, 0);
        let _c = cache.get(t, SIZE, other);
        assert_eq!(cache.parsed_len(), 1);

        cache.clear();
        let _d = cache.get(t, SIZE, COLOR);
        assert_eq!(cache.parsed_len(), 1);
    }

    #[test]
    fn steam_layout_icon_names_resolve() {
        for name in [
            "backspace",
            "gear",
            "key-return",
            "arrow-fat-up",
            "arrow-line-right",
            "stop",
            "record",
            "battery-empty",
            "battery-low",
            "battery-medium",
            "battery-high",
            "battery-full",
            "battery-charging",
        ] {
            assert!(
                Icon::from_name(name).is_some(),
                "missing phosphor icon {name}"
            );
        }
        // styles used in TOML
        assert_eq!(
            expand("{icon:stop:fill}", SIZE, COLOR).text(),
            Icon::from_name("stop").unwrap().as_str()
        );
        assert_eq!(
            expand("{icon:record:fill}", SIZE, COLOR).text(),
            Icon::from_name("record").unwrap().as_str()
        );
        assert_eq!(
            expand("{icon:gear}", SIZE, COLOR).text(),
            Icon::from_name("gear").unwrap().as_str()
        );
    }
}
