use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc};

use anyhow::{bail, Context as _, Result};
use egui::{FontData, FontDefinitions, FontFamily, FontId, TextStyle, Ui};
use skrifa::raw::TableProvider as _;

use crate::theme::ThemeFonts;

const MAX_FONT_BYTES: u64 = 8 * 1024 * 1024;
const MAX_SESSION_BYTES: usize = 64 * 1024 * 1024;
const MAX_SESSION_FILES: usize = 32;

#[derive(Clone, Copy)]
pub(crate) enum FontRole {
    Body,
    Menu,
    Heading,
    Keyboard,
}

impl FontRole {
    fn name(self) -> &'static str {
        match self {
            Self::Body => "kosk-body-font",
            Self::Menu => "kosk-menu-font",
            Self::Heading => "kosk-heading-font",
            Self::Keyboard => "kosk-keyboard-font",
        }
    }

    fn family(self) -> FontFamily {
        match self {
            Self::Body => FontFamily::Proportional,
            _ => FontFamily::Name(self.name().into()),
        }
    }

    fn text_style(self) -> TextStyle {
        TextStyle::Name(self.name().into())
    }
}

pub(crate) fn font_id(ui: &Ui, role: FontRole, size: f32) -> FontId {
    let family = ui
        .style()
        .text_styles
        .get(&role.text_style())
        .map(|font| font.family.clone())
        .unwrap_or(FontFamily::Proportional);
    FontId::new(size, family)
}

struct PendingFont {
    path: PathBuf,
    result: mpsc::Receiver<Result<Arc<FontData>>>,
}

#[derive(Default)]
pub(crate) struct ThemeFontService {
    base: Option<FontDefinitions>,
    cache: BTreeMap<PathBuf, Option<Arc<FontData>>>,
    pending: Option<PendingFont>,
    applied: Option<ThemeFonts>,
    cached_bytes: usize,
}

impl ThemeFontService {
    pub(crate) fn update(&mut self, ui: &mut Ui, requested: &ThemeFonts) {
        if self.base.is_none() {
            self.base = Some(ui.fonts(|fonts| fonts.definitions().clone()));
        }
        self.finish_loading();

        let paths = font_paths(requested);
        let full = self.cache.len() >= MAX_SESSION_FILES || self.cached_bytes >= MAX_SESSION_BYTES;
        if !full
            && self.pending.is_none()
            && let Some((_, path)) = paths
                .iter()
                .find(|(_, path)| !self.cache.contains_key(*path))
        {
            self.start_loading(ui, (*path).clone());
        }

        if (full || paths.iter().all(|(_, path)| self.cache.contains_key(*path)))
            && self.applied.as_ref() != Some(requested)
        {
            if full {
                for (_, path) in paths
                    .iter()
                    .filter(|(_, path)| !self.cache.contains_key(*path))
                {
                    report_failure(path, "session font cache limit reached");
                }
            }
            ui.ctx().set_fonts(self.definitions(requested));
            ui.ctx().request_discard("theme fonts changed");
            self.applied = Some(requested.clone());
        }

        for role in [FontRole::Menu, FontRole::Heading, FontRole::Keyboard] {
            let family = ui.fonts(|fonts| {
                if fonts.definitions().families.contains_key(&role.family()) {
                    role.family()
                } else {
                    FontFamily::Proportional
                }
            });
            let size = ui.style().text_styles[&TextStyle::Body].size;
            ui.style_mut()
                .text_styles
                .insert(role.text_style(), FontId::new(size, family.clone()));
            if matches!(role, FontRole::Heading) {
                ui.style_mut()
                    .text_styles
                    .get_mut(&TextStyle::Heading)
                    .expect("heading font")
                    .family = family;
            }
        }
    }

    fn start_loading(&mut self, ui: &Ui, path: PathBuf) {
        let (tx, result) = mpsc::channel();
        let source = path.clone();
        let ctx = ui.ctx().clone();
        let remaining = MAX_SESSION_BYTES - self.cached_bytes;
        match std::thread::Builder::new()
            .name("theme-font".into())
            .spawn(move || {
                let _ = tx.send(load_font(&source, remaining));
                ctx.request_repaint();
            }) {
            Ok(_) => self.pending = Some(PendingFont { path, result }),
            Err(error) => self.failed(path, &error.to_string()),
        }
    }

    fn finish_loading(&mut self) {
        let Some(pending) = &self.pending else {
            return;
        };
        let result = match pending.result.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => Err(anyhow::anyhow!("font worker stopped")),
        };
        let pending = self.pending.take().expect("pending font");
        match result {
            Ok(font) => {
                self.cached_bytes += font.as_ref().as_ref().len();
                self.cache.insert(pending.path, Some(font));
            }
            Err(error) => self.failed(pending.path, &format!("{error:#}")),
        }
    }

    fn failed(&mut self, path: PathBuf, error: &str) {
        report_failure(&path, error);
        self.cache.insert(path, None);
    }

    fn definitions(&self, requested: &ThemeFonts) -> FontDefinitions {
        let mut definitions = self.base.clone().expect("base fonts");
        let fallback = definitions.families[&FontFamily::Proportional].clone();
        let paths = font_paths(requested);
        for role in [
            FontRole::Body,
            FontRole::Menu,
            FontRole::Heading,
            FontRole::Keyboard,
        ] {
            let mut family_fonts = fallback.clone();
            let font = paths
                .iter()
                .find(|(candidate, _)| candidate.name() == role.name())
                .and_then(|(_, path)| self.cache.get(*path))
                .and_then(Option::as_ref);
            if let Some(font) = font {
                definitions
                    .font_data
                    .insert(role.name().into(), font.clone());
                family_fonts.insert(0, role.name().into());
            }
            definitions.families.insert(role.family(), family_fonts);
        }
        definitions
    }
}

fn font_paths(fonts: &ThemeFonts) -> Vec<(FontRole, &PathBuf)> {
    [
        (FontRole::Body, fonts.body_font.as_ref()),
        (FontRole::Menu, fonts.menu_font.as_ref()),
        (FontRole::Heading, fonts.heading_font.as_ref()),
        (FontRole::Keyboard, fonts.keyboard_font.as_ref()),
    ]
    .into_iter()
    .filter_map(|(role, path)| path.map(|path| (role, path)))
    .collect()
}

fn report_failure(path: &Path, error: &str) {
    eprintln!("theme font {}: {error}", path.display());
    crate::user_notify::notify(crate::user_notify::Notice::theme_font_failed(path));
}

fn load_font(path: &Path, remaining: usize) -> Result<Arc<FontData>> {
    let file = std::fs::File::open(path).context("open font")?;
    let limit = MAX_FONT_BYTES.min(remaining as u64);
    if file.metadata()?.len() > limit {
        bail!("font exceeds size limit");
    }
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .context("read font")?;
    if bytes.len() as u64 > limit {
        bail!("font exceeds size limit");
    }
    let font = skrifa::FontRef::from_index(&bytes, 0).context("decode font")?;
    font.head().context("font header")?;
    font.hhea().context("font horizontal metrics")?;
    font.cmap().context("font character map")?;
    if font.maxp().context("font glyph count")?.num_glyphs() == 0 {
        bail!("font has no glyphs");
    }
    Ok(Arc::new(FontData::from_owned(bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_fonts() -> FontDefinitions {
        let mut fonts = FontDefinitions::default();
        egui_phosphor_icons::add_fonts(&mut fonts);
        fonts
    }

    #[test]
    fn roles_keep_icons_monospace_and_glyph_fallbacks_and_restore_defaults() {
        let base = base_fonts();
        let path = PathBuf::from("generated-menu.ttf");
        let data = base.font_data[&base.families[&FontFamily::Monospace][0]].clone();
        let service = ThemeFontService {
            base: Some(base.clone()),
            cache: BTreeMap::from([(path.clone(), Some(data))]),
            ..Default::default()
        };
        let requested = ThemeFonts {
            menu_font: Some(path),
            ..Default::default()
        };
        let themed = service.definitions(&requested);
        assert_eq!(
            themed.families[&FontFamily::Proportional],
            base.families[&FontFamily::Proportional]
        );
        assert_eq!(
            themed.families[&FontFamily::Monospace],
            base.families[&FontFamily::Monospace]
        );
        assert_eq!(
            &themed.families[&FontRole::Menu.family()][1..],
            &base.families[&FontFamily::Proportional]
        );
        assert_eq!(
            themed.families[&FontRole::Keyboard.family()],
            base.families[&FontFamily::Proportional]
        );
        for (family, fonts) in &base.families {
            if let FontFamily::Name(_) = family {
                assert_eq!(&themed.families[family], fonts);
            }
        }
        let restored = service.definitions(&ThemeFonts::default());
        assert_eq!(restored.font_data, base.font_data);
        assert_eq!(
            restored.families[&FontRole::Menu.family()],
            base.families[&FontFamily::Proportional]
        );
    }

    #[test]
    fn stale_load_is_cached_without_applying_it_to_current_theme() {
        let base = base_fonts();
        let path = PathBuf::from("old-theme.ttf");
        let data = base.font_data.values().next().unwrap().clone();
        let (sender, result) = mpsc::channel();
        sender.send(Ok(data)).unwrap();
        let mut service = ThemeFontService {
            base: Some(base.clone()),
            pending: Some(PendingFont {
                path: path.clone(),
                result,
            }),
            ..Default::default()
        };
        service.finish_loading();
        assert!(service.pending.is_none());
        assert!(service.cache[&path].is_some());
        assert!(service.applied.is_none());
        assert_eq!(
            service.definitions(&ThemeFonts::default()).font_data,
            base.font_data
        );
    }

    #[test]
    fn fonts_load_once_even_when_files_change_or_disappear() {
        let base = base_fonts();
        let data = base.font_data.values().next().unwrap();
        let root = std::env::temp_dir().join(format!("kosk-font-test-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("generated.ttf");
        std::fs::write(&path, data.as_ref().as_ref()).unwrap();
        assert!(load_font(&path, MAX_SESSION_BYTES).is_ok());
        assert!(load_font(&path, 1).is_err());
        std::fs::write(&path, b"invalid font").unwrap();
        assert!(load_font(&path, MAX_SESSION_BYTES).is_err());
        std::fs::remove_file(&path).unwrap();
        assert!(load_font(&path, MAX_SESSION_BYTES).is_err());

        let mut service = ThemeFontService {
            base: Some(base.clone()),
            cache: BTreeMap::from([(path.clone(), Some(data.clone()))]),
            ..Default::default()
        };
        let requested = ThemeFonts {
            keyboard_font: Some(path),
            ..Default::default()
        };
        let ctx = egui::Context::default();
        for fonts in [&requested, &ThemeFonts::default(), &requested] {
            let mut output = ctx.run_ui(Default::default(), |ui| service.update(ui, fonts));
            output.textures_delta.clear();
            assert!(service.pending.is_none());
        }
        assert_eq!(service.cache.len(), 1);
        assert_eq!(service.applied.as_ref(), Some(&requested));
        std::fs::remove_dir(&root).unwrap();
    }
}
