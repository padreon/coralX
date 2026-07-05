//! Welcome / mode-selection screen shown on app startup.

use egui::{Color32, RichText, Ui};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    PointCount,
    Measurement,
}

const ACCENT: Color32 = Color32::from_rgb(0x4f, 0xc3, 0xf7);
const CARD_BG: Color32 = Color32::from_rgb(0x2a, 0x2a, 0x2a);
const CARD_BG_HOVER: Color32 = Color32::from_rgb(0x2e, 0x3a, 0x42);
const CARD_BORDER: Color32 = Color32::from_rgb(0x44, 0x44, 0x44);
const DESC_GRAY: Color32 = Color32::from_rgb(0xaa, 0xaa, 0xaa);

/// Draws the welcome screen; returns `Some(mode)` once the user picks a card.
pub fn show(ui: &mut Ui) -> Option<Mode> {
    let mut selected = None;

    ui.vertical_centered(|ui| {
        ui.add_space(30.0);
        ui.label(RichText::new("coralX").size(28.0).strong().color(ACCENT));
        ui.add_space(4.0);
        ui.label(RichText::new("Choose a mode to begin").color(Color32::from_rgb(0x88, 0x88, 0x88)));
        ui.add_space(20.0);

        ui.horizontal(|ui| {
            ui.add_space((ui.available_width() - 490.0).max(0.0) / 2.0);
            if card(ui, "\u{1F3AF}", "Coral Point Count", "Random-point sampling\nfor benthic coverage\nestimation") {
                selected = Some(Mode::PointCount);
            }
            ui.add_space(28.0);
            if card(ui, "\u{1F4CF}", "Fragment Measurement", "Measure coral height &\narea for growth\nmonitoring") {
                selected = Some(Mode::Measurement);
            }
        });
    });

    selected
}

fn card(ui: &mut Ui, icon: &str, title: &str, desc: &str) -> bool {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(230.0, 170.0), egui::Sense::click());
    let hovered = response.hovered();

    let bg = if hovered { CARD_BG_HOVER } else { CARD_BG };
    let border = if hovered { ACCENT } else { CARD_BORDER };
    ui.painter().rect(rect, 10.0, bg, egui::Stroke::new(2.0, border), egui::StrokeKind::Inside);

    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink(10.0)));
    child.vertical_centered(|ui| {
        ui.add_space(6.0);
        ui.label(RichText::new(icon).size(32.0));
        ui.label(RichText::new(title).size(15.0).strong());
        ui.label(RichText::new(desc).size(11.0).color(DESC_GRAY));
        ui.add_space(4.0);
        let clicked_btn = ui
            .add(egui::Button::new(RichText::new("Select").color(Color32::from_rgb(0x11, 0x11, 0x11)).strong()).fill(ACCENT))
            .clicked();
        if clicked_btn {
            ui.ctx().request_repaint();
        }
    });

    response.clicked()
}
