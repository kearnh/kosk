//! Show every controller-button glyph for Steam Controller 2 and DualShock 4.

use eframe::egui;
use kosk::controller::ControllerButton;
use kosk::controller_glyph::{self, GlyphFamily};
use strum::VariantArray;

const GLYPH_SIZE: f32 = 32.0;
const BACKGROUND: egui::Color32 = egui::Color32::from_rgb(20, 20, 20);

fn main() -> eframe::Result {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([480.0, 720.0])
            .with_title("Glyph gallery"),
        centered: true,
        ..Default::default()
    };

    eframe::run_native(
        "Glyph gallery",
        native_options,
        Box::new(|cc| {
            egui_extras::install_image_loaders(&cc.egui_ctx);
            Ok(Box::new(Gallery))
        }),
    )
}

struct Gallery;

impl eframe::App for Gallery {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        BACKGROUND.to_normalized_gamma_f32()
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.ctx().set_visuals(egui::Visuals {
            panel_fill: BACKGROUND,
            window_fill: BACKGROUND,
            ..egui::Visuals::dark()
        });

        ui.heading("Controller glyphs");
        ui.label("Sc2 and Ps4 columns. White knockout art needs this dark background.");

        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Grid::new("glyphs")
                .striped(true)
                .min_col_width(GLYPH_SIZE)
                .show(ui, |ui| {
                    ui.label("Button");
                    ui.label("Sc2");
                    ui.label("Ps4");
                    ui.end_row();

                    for button in ControllerButton::VARIANTS {
                        ui.label(format!("{button:?}"));
                        controller_glyph::show(ui, GlyphFamily::Sc2, *button, GLYPH_SIZE);
                        controller_glyph::show(ui, GlyphFamily::Ps4, *button, GLYPH_SIZE);
                        ui.end_row();
                    }
                });
        });
    }
}
