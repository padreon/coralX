//! Dialogs for the four import workflows.

use egui::Context;

/// Shows a success/error message plus optional warnings after any import.
/// Draws until the user dismisses it; returns `true` once OK is clicked.
pub struct ImportResultDialog {
    title: String,
    message: String,
    warnings: Vec<String>,
}

impl ImportResultDialog {
    pub fn new(title: impl Into<String>, message: impl Into<String>, warnings: Vec<String>) -> Self {
        ImportResultDialog { title: title.into(), message: message.into(), warnings }
    }

    pub fn show(&self, ctx: &Context) -> bool {
        let icon = if self.warnings.iter().any(|w| w.contains("not found")) { "\u{26A0}" } else { "\u{2705}" };
        let mut closed = false;
        egui::Window::new(&self.title).collapsible(false).resizable(false).show(ctx, |ui| {
            ui.set_min_width(440.0);
            ui.label(format!("{icon}  {}", self.message));
            if !self.warnings.is_empty() {
                ui.separator();
                ui.label("Warnings:");
                egui::ScrollArea::vertical().max_height(120.0).show(ui, |ui| {
                    for w in &self.warnings {
                        ui.label(w);
                    }
                });
            }
            ui.separator();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("OK").clicked() {
                    closed = true;
                }
            });
        });
        closed
    }
}

/// Shown before importing coral codes — merge with existing codes or replace them.
pub struct CoralCodesMergeDialog {
    incoming_count: usize,
    existing_count: usize,
    has_groups: bool,
    pub merge: bool,
    pub import_groups: bool,
}

pub enum MergeOutcome {
    Confirmed,
    Cancelled,
}

impl CoralCodesMergeDialog {
    pub fn new(incoming_count: usize, existing_count: usize, has_groups: bool) -> Self {
        CoralCodesMergeDialog { incoming_count, existing_count, has_groups, merge: true, import_groups: true }
    }

    pub fn show(&mut self, ctx: &Context) -> Option<MergeOutcome> {
        let mut outcome = None;
        egui::Window::new("Import Coral Codes").collapsible(false).resizable(false).show(ctx, |ui| {
            ui.set_min_width(380.0);
            ui.label(format!("Found {} code(s) in the file.\nProject currently has {} code(s).", self.incoming_count, self.existing_count));
            ui.separator();
            ui.group(|ui| {
                ui.label("Action");
                ui.radio_value(&mut self.merge, true, "Merge — add new codes, keep existing ones");
                ui.radio_value(&mut self.merge, false, "Replace — remove all existing codes first");
            });
            if self.has_groups {
                ui.checkbox(&mut self.import_groups, "Also import group definitions from file");
            }
            ui.separator();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("OK").clicked() {
                    outcome = Some(MergeOutcome::Confirmed);
                }
                if ui.button("Cancel").clicked() {
                    outcome = Some(MergeOutcome::Cancelled);
                }
            });
        });
        outcome
    }
}

/// Shown before importing station metadata — how to handle name conflicts.
pub struct StationMergeDialog {
    incoming_count: usize,
    new_count: usize,
    overlap_count: usize,
    has_overlap: bool,
    pub update_existing: bool,
}

impl StationMergeDialog {
    pub fn new(incoming_names: &[String], existing_names: &[String]) -> Self {
        let overlap = incoming_names.iter().filter(|n| existing_names.contains(n)).count();
        StationMergeDialog {
            incoming_count: incoming_names.len(),
            new_count: incoming_names.len() - overlap,
            overlap_count: overlap,
            has_overlap: overlap > 0,
            update_existing: true,
        }
    }

    pub fn show(&mut self, ctx: &Context) -> Option<MergeOutcome> {
        let mut outcome = None;
        egui::Window::new("Import Station Metadata").collapsible(false).resizable(false).show(ctx, |ui| {
            ui.set_min_width(400.0);
            ui.label(format!("Found {} station(s) in the file.", self.incoming_count));
            if self.new_count > 0 {
                ui.label(format!("- {} new station(s) will be added.", self.new_count));
            }
            if self.overlap_count > 0 {
                ui.label(format!("- {} existing station(s) match by name.", self.overlap_count));
            }
            if self.has_overlap {
                ui.separator();
                ui.group(|ui| {
                    ui.label("For conflicting stations:");
                    ui.radio_value(&mut self.update_existing, true, "Update metadata (depth, GPS, date, notes)");
                    ui.radio_value(&mut self.update_existing, false, "Skip — keep existing metadata unchanged");
                });
            }
            ui.separator();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("OK").clicked() {
                    outcome = Some(MergeOutcome::Confirmed);
                }
                if ui.button("Cancel").clicked() {
                    outcome = Some(MergeOutcome::Cancelled);
                }
            });
        });
        outcome
    }
}

/// Shown after a successful CPCe import — open as new project or merge stations in.
pub struct CpceImportDialog {
    n_stations: usize,
    n_images: usize,
    n_points: usize,
    has_current_project: bool,
    pub open_as_new: bool,
}

impl CpceImportDialog {
    pub fn new(n_stations: usize, n_images: usize, n_points: usize, has_current_project: bool) -> Self {
        CpceImportDialog { n_stations, n_images, n_points, has_current_project, open_as_new: true }
    }

    pub fn show(&mut self, ctx: &Context) -> Option<MergeOutcome> {
        let mut outcome = None;
        egui::Window::new("Import from CPCe Excel").collapsible(false).resizable(false).show(ctx, |ui| {
            ui.set_min_width(400.0);
            ui.label(format!(
                "Successfully read CPCe data:\n  - {} station(s)\n  - {} image(s)\n  - {} labeled point(s)\n\nHow would you like to import this data?",
                self.n_stations, self.n_images, self.n_points
            ));
            ui.separator();
            ui.group(|ui| {
                ui.label("Action");
                ui.radio_value(&mut self.open_as_new, true, "Open as a new project");
                ui.add_enabled_ui(self.has_current_project, |ui| {
                    ui.radio_value(&mut self.open_as_new, false, "Merge stations into the current project");
                });
            });
            ui.separator();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("OK").clicked() {
                    outcome = Some(MergeOutcome::Confirmed);
                }
                if ui.button("Cancel").clicked() {
                    outcome = Some(MergeOutcome::Cancelled);
                }
            });
        });
        outcome
    }
}
