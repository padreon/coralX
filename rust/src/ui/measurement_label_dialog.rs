//! Shown after a measurement is drawn: the user enters a fragment name and
//! optionally a species/genus. The species list grows as new names are typed.

use egui::{Color32, Context, RichText};

use crate::models::Measurement;

pub enum DialogOutcome {
    Saved(Measurement),
    Cancelled,
}

pub struct MeasurementLabelDialog {
    measurement: Measurement,
    name: String,
    species: String,
}

impl MeasurementLabelDialog {
    pub fn new(measurement: Measurement) -> Self {
        MeasurementLabelDialog { measurement, name: String::new(), species: String::new() }
    }

    fn result_text(&self) -> String {
        let m = &self.measurement;
        let type_name = match m.kind.as_str() {
            "line" => "Length",
            "polyline" => "Length (polyline)",
            "polygon" => "Area",
            other => other,
        };
        let unit_str = if m.kind == "polygon" { format!("{}\u{b2}", m.unit) } else { m.unit.clone() };
        let mut text = format!("{type_name}: {:.3} {unit_str}", m.value);
        if m.kind == "polygon" && m.auto_width > 0.0 {
            text.push_str(&format!(
                "\nWidth: {:.2} {}   Height: {:.2} {}   Perimeter: {:.2} {}",
                m.auto_width, m.unit, m.auto_height, m.unit, m.perimeter_len, m.unit
            ));
        }
        text
    }

    /// Draws the dialog; returns `Some(outcome)` once the user saves or cancels.
    /// `species_list` is grown in place with any newly-typed species name.
    pub fn show(&mut self, ctx: &Context, species_list: &mut Vec<String>) -> Option<DialogOutcome> {
        let mut outcome = None;

        egui::Window::new("Name This Measurement")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.set_min_width(360.0);

                egui::Frame::new().fill(Color32::from_rgb(0x2a, 0x2a, 0x2a)).corner_radius(6.0).inner_margin(8.0).show(ui, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new(self.result_text()).size(13.0));
                    });
                });
                ui.separator();

                ui.horizontal(|ui| {
                    ui.label("Fragment name:");
                    let resp = ui.add(egui::TextEdit::singleline(&mut self.name).hint_text("e.g. Frag-01"));
                    resp.request_focus();
                });

                ui.horizontal(|ui| {
                    ui.label("Spesies/Genus:");
                    ui.add(egui::TextEdit::singleline(&mut self.species).hint_text("Type or select species..."));
                    egui::ComboBox::from_id_salt("species_combo").selected_text("").show_ui(ui, |ui| {
                        for s in species_list.iter() {
                            if ui.selectable_label(false, s).clicked() {
                                self.species = s.clone();
                            }
                        }
                    });
                });

                ui.separator();
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Save").clicked() {
                            let label = if self.name.trim().is_empty() {
                                format!("M-{}", self.measurement.kind.chars().take(3).collect::<String>().to_uppercase())
                            } else {
                                self.name.trim().to_string()
                            };
                            self.measurement.label = label;

                            let species = self.species.trim().to_string();
                            self.measurement.species = species.clone();
                            if !species.is_empty() && !species_list.contains(&species) {
                                species_list.push(species);
                            }
                            outcome = Some(DialogOutcome::Saved(self.measurement.clone()));
                        }
                        if ui.button("Cancel").clicked() {
                            outcome = Some(DialogOutcome::Cancelled);
                        }
                    });
                });
            });

        outcome
    }
}
