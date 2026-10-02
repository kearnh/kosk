//! Show every controller-button glyph for Steam Controller 2 and DualShock 4.

use eframe::egui;
use kosk::controller::ControllerButton;
use kosk::controller_glyph::{self, GlyphFamily};
use strum::VariantArray;

const GLYPH_SIZE: f32 = 48.0;
const COLUMN_GAP: f32 = 20.0;
const ROW_GAP: f32 = 8.0;
const BACKGROUND: egui::Color32 = egui::Color32::from_rgb(20, 20, 20);

fn column_rule(ui: &mut egui::Ui, height: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(1.0, height), egui::Sense::hover());
    let pad = ROW_GAP * 0.5;

    ui.painter().vline(
        rect.center().x,
        (rect.top() - pad)..=(rect.bottom() + pad),
        ui.visuals().widgets.noninteractive.bg_stroke,
    );
}

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
        ui.label("Steam Controller and DualShock 4. CC0 artwork by Kenney.");

        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Grid::new("glyphs")
                .striped(true)
                .spacing(egui::vec2(COLUMN_GAP, ROW_GAP))
                .show(ui, |ui| {
                    let header_h = ui.spacing().interact_size.y;
                    ui.label("Button");
                    column_rule(ui, header_h);
                    ui.label("Sc2");
                    column_rule(ui, header_h);
                    ui.label("Ps4");
                    ui.end_row();

                    for button in ControllerButton::VARIANTS {
                        ui.label(format!("{button:?}"));
                        column_rule(ui, GLYPH_SIZE);
                        controller_glyph::show(ui, GlyphFamily::Sc2, *button, GLYPH_SIZE);
                        column_rule(ui, GLYPH_SIZE);
                        controller_glyph::show(ui, GlyphFamily::Ps4, *button, GLYPH_SIZE);
                        ui.end_row();
                    }
                });
        });
    }
}
