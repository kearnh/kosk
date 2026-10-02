//! CC0 Kenney Input Prompts, embedded for on-screen controller prompts.

use crate::config;
use crate::controller::{ControllerButton, ControllerKind};
use egui::{Image, Ui, Vec2};

macro_rules! glyph {
    ($file:literal) => {
        include_bytes!(concat!("../../assets/controller-glyphs/kenney/", $file))
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlyphFamily {
    Sc2,
    Ps4,
}

impl GlyphFamily {
    pub fn from_kind(kind: ControllerKind) -> Self {
        match kind {
            ControllerKind::Ps4 => Self::Ps4,
            ControllerKind::Sc2 => Self::Sc2,
            ControllerKind::Replay => {
                for k in config::get().preferred_controller {
                    match k {
                        ControllerKind::Ps4 => return Self::Ps4,
                        ControllerKind::Sc2 => return Self::Sc2,
                        ControllerKind::Replay => {}
                    }
                }
                Self::Sc2
            }
        }
    }
}

pub fn svg_bytes(family: GlyphFamily, button: ControllerButton) -> &'static [u8] {
    match (family, button) {
        (GlyphFamily::Sc2, ControllerButton::FaceBottom) => glyph!("steam_button_color_a.svg"),
        (GlyphFamily::Sc2, ControllerButton::FaceRight) => glyph!("steam_button_color_b.svg"),
        (GlyphFamily::Sc2, ControllerButton::FaceLeft) => glyph!("steam_button_color_x.svg"),
        (GlyphFamily::Sc2, ControllerButton::FaceTop) => glyph!("steam_button_color_y.svg"),
        (GlyphFamily::Sc2, ControllerButton::DpadUp) => glyph!("steam_dpad_up.svg"),
        (GlyphFamily::Sc2, ControllerButton::DpadDown) => glyph!("steam_dpad_down.svg"),
        (GlyphFamily::Sc2, ControllerButton::DpadLeft) => glyph!("steam_dpad_left.svg"),
        (GlyphFamily::Sc2, ControllerButton::DpadRight) => glyph!("steam_dpad_right.svg"),
        (GlyphFamily::Sc2, ControllerButton::ShoulderLeft) => glyph!("controller_button_l1.svg"),
        (GlyphFamily::Sc2, ControllerButton::ShoulderRight) => glyph!("controller_button_r1.svg"),
        (GlyphFamily::Sc2, ControllerButton::TriggerLeft) => glyph!("controller_button_l2.svg"),
        (GlyphFamily::Sc2, ControllerButton::TriggerRight) => glyph!("controller_button_r2.svg"),
        (GlyphFamily::Sc2, ControllerButton::Options) => glyph!("controller_button_options.svg"),
        (GlyphFamily::Sc2, ControllerButton::Share) => glyph!("controller_button_view.svg"),
        (GlyphFamily::Sc2, ControllerButton::System) => glyph!("controller_icon.svg"),
        (GlyphFamily::Sc2, ControllerButton::PadLeft) => glyph!("steamdeck_trackpad_l.svg"),
        (GlyphFamily::Sc2, ControllerButton::PadRight) => glyph!("steamdeck_trackpad_r.svg"),

        (GlyphFamily::Ps4, ControllerButton::FaceBottom) => {
            glyph!("playstation_button_color_cross.svg")
        }
        (GlyphFamily::Ps4, ControllerButton::FaceRight) => {
            glyph!("playstation_button_color_circle.svg")
        }
        (GlyphFamily::Ps4, ControllerButton::FaceLeft) => {
            glyph!("playstation_button_color_square.svg")
        }
        (GlyphFamily::Ps4, ControllerButton::FaceTop) => {
            glyph!("playstation_button_color_triangle.svg")
        }
        (GlyphFamily::Ps4, ControllerButton::DpadUp) => glyph!("playstation_dpad_up.svg"),
        (GlyphFamily::Ps4, ControllerButton::DpadDown) => glyph!("playstation_dpad_down.svg"),
        (GlyphFamily::Ps4, ControllerButton::DpadLeft) => glyph!("playstation_dpad_left.svg"),
        (GlyphFamily::Ps4, ControllerButton::DpadRight) => glyph!("playstation_dpad_right.svg"),
        (GlyphFamily::Ps4, ControllerButton::ShoulderLeft) => glyph!("playstation_trigger_l1.svg"),
        (GlyphFamily::Ps4, ControllerButton::ShoulderRight) => glyph!("playstation_trigger_r1.svg"),
        (GlyphFamily::Ps4, ControllerButton::TriggerLeft) => glyph!("playstation_trigger_l2.svg"),
        (GlyphFamily::Ps4, ControllerButton::TriggerRight) => glyph!("playstation_trigger_r2.svg"),
        (GlyphFamily::Ps4, ControllerButton::Options) => glyph!("playstation4_button_options.svg"),
        (GlyphFamily::Ps4, ControllerButton::Share) => glyph!("playstation4_button_share.svg"),
        (GlyphFamily::Ps4, ControllerButton::System) => glyph!("switch_button_home.svg"),
        (GlyphFamily::Ps4, ControllerButton::PadLeft) => {
            glyph!("playstation4_touchpad_press_left.svg")
        }
        (GlyphFamily::Ps4, ControllerButton::PadRight) => {
            glyph!("playstation4_touchpad_press_right.svg")
        }

        (GlyphFamily::Sc2, ControllerButton::StickLeft) => glyph!("steamdeck_stick_l_press.svg"),
        (GlyphFamily::Ps4, ControllerButton::StickLeft) => glyph!("playstation_stick_l_press.svg"),
        (GlyphFamily::Sc2, ControllerButton::StickRight) => glyph!("steamdeck_stick_r_press.svg"),
        (GlyphFamily::Ps4, ControllerButton::StickRight) => glyph!("playstation_stick_r_press.svg"),
        (_, ControllerButton::L4) => glyph!("controller_button_l4.svg"),
        (_, ControllerButton::L5) => glyph!("controller_button_l5.svg"),
        (_, ControllerButton::R4) => glyph!("controller_button_r4.svg"),
        (_, ControllerButton::R5) => glyph!("controller_button_r5.svg"),
        (_, ControllerButton::QuickAccess) => glyph!("controller_button_quickaccess.svg"),
    }
}

fn image_uri(family: GlyphFamily, button: ControllerButton) -> String {
    format!("bytes://controller-glyph/{family:?}/{button:?}.svg")
}

pub fn show(ui: &mut Ui, family: GlyphFamily, button: ControllerButton, size: f32) {
    let bytes = svg_bytes(family, button);
    ui.add_sized(
        Vec2::splat(size),
        Image::from_bytes(image_uri(family, button), bytes)
            .fit_to_exact_size(Vec2::splat(size))
            .bg_fill(egui::Color32::TRANSPARENT),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use strum::VariantArray;

    #[test]
    fn image_uri_ends_with_svg() {
        let uri = image_uri(GlyphFamily::Sc2, ControllerButton::FaceBottom);
        assert!(
            uri.ends_with(".svg"),
            "svg loader only claims uris ending in .svg, got {uri}"
        );
    }

    #[test]
    fn every_button_renders_for_both_families() {
        for family in [GlyphFamily::Sc2, GlyphFamily::Ps4] {
            for button in ControllerButton::VARIANTS {
                let bytes = svg_bytes(family, *button);
                let tree = resvg::usvg::Tree::from_data(bytes, &Default::default())
                    .unwrap_or_else(|error| panic!("{family:?} {button:?}: {error}"));

                for size in [16, 28, 32] {
                    let mut pixmap = resvg::tiny_skia::Pixmap::new(size, size).unwrap();
                    let scale = size as f32 / tree.size().width();
                    resvg::render(
                        &tree,
                        resvg::tiny_skia::Transform::from_scale(scale, scale),
                        &mut pixmap.as_mut(),
                    );

                    assert!(
                        pixmap.pixels().iter().any(|pixel| pixel.alpha() > 0),
                        "{family:?} {button:?} is empty at {size}px"
                    );
                    assert_eq!(pixmap.pixel(0, 0).unwrap().alpha(), 0);
                }
            }
        }
    }

    #[test]
    fn paired_controls_have_distinct_artwork() {
        use ControllerButton::*;

        for family in [GlyphFamily::Sc2, GlyphFamily::Ps4] {
            for (left, right) in [
                (PadLeft, PadRight),
                (StickLeft, StickRight),
                (ShoulderLeft, ShoulderRight),
                (TriggerLeft, TriggerRight),
                (Options, Share),
                (L4, L5),
                (R4, R5),
                (DpadUp, DpadDown),
                (DpadLeft, DpadRight),
            ] {
                assert_ne!(svg_bytes(family, left), svg_bytes(family, right));
            }
        }
    }
}
