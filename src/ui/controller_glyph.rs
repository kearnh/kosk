//! CC0 Kenney Input Prompts, embedded under legacy glyph filenames.

use crate::config;
use crate::controller::{ControllerButton, ControllerKind};
use egui::{Image, Ui, Vec2};

macro_rules! knockout {
    ($file:literal) => {
        include_bytes!(concat!("../../assets/controller-glyphs/knockout/", $file))
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
        (GlyphFamily::Sc2, ControllerButton::FaceBottom) => knockout!("shared_color_button_a.svg"),
        (GlyphFamily::Sc2, ControllerButton::FaceRight) => knockout!("shared_color_button_b.svg"),
        (GlyphFamily::Sc2, ControllerButton::FaceLeft) => knockout!("shared_color_button_x.svg"),
        (GlyphFamily::Sc2, ControllerButton::FaceTop) => knockout!("shared_color_button_y.svg"),
        (GlyphFamily::Sc2, ControllerButton::DpadUp) => knockout!("sc_dpad_up.svg"),
        (GlyphFamily::Sc2, ControllerButton::DpadDown) => knockout!("sc_dpad_down.svg"),
        (GlyphFamily::Sc2, ControllerButton::DpadLeft) => knockout!("sc_dpad_left.svg"),
        (GlyphFamily::Sc2, ControllerButton::DpadRight) => knockout!("sc_dpad_right.svg"),
        (GlyphFamily::Sc2, ControllerButton::ShoulderLeft) => knockout!("sc_l1.svg"),
        (GlyphFamily::Sc2, ControllerButton::ShoulderRight) => knockout!("sc_r1.svg"),
        (GlyphFamily::Sc2, ControllerButton::TriggerLeft) => knockout!("sc_l2.svg"),
        (GlyphFamily::Sc2, ControllerButton::TriggerRight) => knockout!("sc_r2.svg"),
        (GlyphFamily::Sc2, ControllerButton::Options) => knockout!("sd_button_menu.svg"),
        (GlyphFamily::Sc2, ControllerButton::Share) => knockout!("sd_button_view.svg"),
        (GlyphFamily::Sc2, ControllerButton::System) => knockout!("sc_button_steam.svg"),
        (GlyphFamily::Sc2, ControllerButton::PadLeft) => knockout!("sc_touchpad_left.svg"),
        (GlyphFamily::Sc2, ControllerButton::PadRight) => knockout!("sc_touchpad_right.svg"),

        (GlyphFamily::Ps4, ControllerButton::FaceBottom) => knockout!("ps_color_button_x.svg"),
        (GlyphFamily::Ps4, ControllerButton::FaceRight) => knockout!("ps_color_button_circle.svg"),
        (GlyphFamily::Ps4, ControllerButton::FaceLeft) => knockout!("ps_color_button_square.svg"),
        (GlyphFamily::Ps4, ControllerButton::FaceTop) => knockout!("ps_color_button_triangle.svg"),
        (GlyphFamily::Ps4, ControllerButton::DpadUp) => knockout!("ps_dpad_up.svg"),
        (GlyphFamily::Ps4, ControllerButton::DpadDown) => knockout!("ps_dpad_down.svg"),
        (GlyphFamily::Ps4, ControllerButton::DpadLeft) => knockout!("ps_dpad_left.svg"),
        (GlyphFamily::Ps4, ControllerButton::DpadRight) => knockout!("ps_dpad_right.svg"),
        (GlyphFamily::Ps4, ControllerButton::ShoulderLeft) => knockout!("ps4_l1.svg"),
        (GlyphFamily::Ps4, ControllerButton::ShoulderRight) => knockout!("ps4_r1.svg"),
        (GlyphFamily::Ps4, ControllerButton::TriggerLeft) => knockout!("ps4_l2.svg"),
        (GlyphFamily::Ps4, ControllerButton::TriggerRight) => knockout!("ps4_r2.svg"),
        (GlyphFamily::Ps4, ControllerButton::Options) => knockout!("ps4_button_options.svg"),
        (GlyphFamily::Ps4, ControllerButton::Share) => knockout!("ps4_button_share.svg"),
        (GlyphFamily::Ps4, ControllerButton::System) => knockout!("ps4_button_logo.svg"),
        (GlyphFamily::Ps4, ControllerButton::PadLeft) => knockout!("ps4_trackpad_l_click.svg"),
        (GlyphFamily::Ps4, ControllerButton::PadRight) => knockout!("ps4_trackpad_r_click.svg"),

        (_, ControllerButton::StickLeft) => knockout!("shared_l3.svg"),
        (_, ControllerButton::StickRight) => knockout!("shared_r3.svg"),
        (_, ControllerButton::L4) => knockout!("sc_l4.svg"),
        (_, ControllerButton::L5) => knockout!("sc_l5.svg"),
        (_, ControllerButton::R4) => knockout!("sc_r4.svg"),
        (_, ControllerButton::R5) => knockout!("sc_r5.svg"),
        (_, ControllerButton::QuickAccess) => knockout!("qam_icon.svg"),
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
    fn every_button_has_svg_for_both_families() {
        for family in [GlyphFamily::Sc2, GlyphFamily::Ps4] {
            for button in ControllerButton::VARIANTS {
                let bytes = svg_bytes(family, *button);
                assert!(
                    bytes.starts_with(b"<svg"),
                    "{family:?} {button:?} is not an svg"
                );
            }
        }
    }
}
