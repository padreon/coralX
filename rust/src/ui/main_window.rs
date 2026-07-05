//! Coral Point Count mode window: the largest screen, wiring together
//! project/station/image management, ImageCanvas point labeling, the coral
//! codes panel, calibration, AI auto-label, analysis, and all import/export
//! flows.
//!
//! Known simplifications vs. the Python version (not yet ported):
//! - No async thumbnail loading — the image tree is text-only.
//! - No splitter-size persistence across sessions.
//! - The points-table label editor is a plain text field, not a popup
//!   autocomplete list (prefix-matching still works on Enter).

use std::collections::HashMap;

use egui::Context;

use crate::core::ai_labeler::{AiLabeler, LabelResult};
use crate::core::analysis::photo_area;
use crate::core::exporter::{export_coral_codes, export_csv, export_excel};
use crate::core::importer::{
    import_coral_codes, import_cpce_cpc, import_cpce_excel, import_labeled_points, import_station_metadata, ImportResult, ImportedGroup,
};
use crate::core::point_generator::generate_points;
use crate::core::statistics::{per_station_table, project_summary};
use crate::models::{CoralGroup, ImageAnnotation, Project, Station};
use crate::ui::ai_label_dialog::{AiLabelDialog, AiProgressDialog};
use crate::ui::analysis_dialog::AnalysisDialog;
use crate::ui::calibration_dialog::CalibrationDialog;
use crate::ui::image_canvas::{AnnotationView, BorderMode, CanvasEvent, ImageCanvas};
use crate::ui::import_dialogs::{CoralCodesMergeDialog, CpceImportDialog, ImportResultDialog, MergeOutcome, StationMergeDialog};

#[derive(PartialEq, Clone, Copy)]
enum ProgressScope {
    Image,
    Station,
    Project,
}

struct Confirm {
    title: String,
    message: String,
    on_yes: ConfirmAction,
}

enum ConfirmAction {
    DeleteStation(usize),
    RemoveImage(usize, usize),
}

struct StationMetaEdit {
    station_idx: usize,
    name: String,
    depth: String,
    date: String,
    lat: String,
    lon: String,
    notes: String,
}

struct ManageGroupsEdit {
    rows: Vec<(String, String)>, // (name, "code, code, ...")
}

struct AddCodeEdit {
    code: String,
    desc: String,
}

pub struct PointCountScreen {
    project: Project,
    current_station: usize,
    current_ann: Option<usize>,
    canvas: ImageCanvas,
    selected_row: Option<usize>,

    point_count: i64,
    distribution: usize, // 0=random 1=stratified 2=uniform
    border: i64,

    sort_az: bool,
    filter_incomplete: bool,
    progress_scope: ProgressScope,
    status: String,

    calibration_dialog: Option<CalibrationDialog>,
    ai_label_dialog: Option<AiLabelDialog>,
    ai_progress_dialog: Option<AiProgressDialog>,
    analysis_dialog: Option<AnalysisDialog>,
    add_code_dialog: Option<AddCodeEdit>,
    manage_groups_dialog: Option<ManageGroupsEdit>,
    station_meta_dialog: Option<StationMetaEdit>,
    import_result: Option<ImportResultDialog>,
    coral_codes_merge: Option<(HashMap<String, String>, Vec<ImportedGroup>, CoralCodesMergeDialog)>,
    station_merge: Option<(Vec<crate::core::importer::ImportedStation>, StationMergeDialog)>,
    cpce_import: Option<(Project, CpceImportDialog)>,
    stats_open: bool,
    confirm: Option<Confirm>,
    pending_error: Option<String>,
    pending_info: Option<(String, String)>,
}

const DISTRIBUTIONS: [&str; 3] = ["random", "stratified", "uniform"];

impl PointCountScreen {
    pub fn new() -> Self {
        let mut project = Project::new("Untitled Project");
        load_default_codes(&mut project);
        project.stations.push(Station::new("Station 1"));
        PointCountScreen {
            project,
            current_station: 0,
            current_ann: None,
            canvas: ImageCanvas::default(),
            selected_row: None,
            point_count: 10,
            distribution: 0,
            border: 0,
            sort_az: false,
            filter_incomplete: false,
            progress_scope: ProgressScope::Image,
            status: "New project created".to_string(),
            calibration_dialog: None,
            ai_label_dialog: None,
            ai_progress_dialog: None,
            analysis_dialog: None,
            add_code_dialog: None,
            manage_groups_dialog: None,
            station_meta_dialog: None,
            import_result: None,
            coral_codes_merge: None,
            station_merge: None,
            cpce_import: None,
            stats_open: false,
            confirm: None,
            pending_error: None,
            pending_info: None,
        }
    }

    fn reload_canvas(&mut self, ctx: &Context) {
        let Some(idx) = self.current_ann else { return };
        let Some(station) = self.project.stations.get(self.current_station) else { return };
        let Some(ann) = station.annotations.get(idx) else { return };
        let path = ann.image_path.clone();

        if let Some(poly) = self.project.border_polygon.clone() {
            let pts: Vec<(f64, f64)> = poly.iter().filter(|p| p.len() >= 2).map(|p| (p[0], p[1])).collect();
            self.canvas.set_border_polygon(Some(pts));
            self.canvas.set_border_rect(None);
        } else if let Some(rect) = &self.project.border_rect {
            if rect.len() == 4 {
                self.canvas.set_border_rect(Some((rect[0] as i64, rect[1] as i64, rect[2] as i64, rect[3] as i64)));
            }
            self.canvas.set_border_polygon(None);
        } else {
            self.canvas.set_border_rect(None);
            self.canvas.set_border_polygon(None);
            self.canvas.set_border(self.border);
        }

        let mut codes: Vec<(String, String)> = self.project.coral_codes.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        codes.sort();
        if let Err(e) = self.canvas.load_image(ctx, &path, codes) {
            self.status = format!("Could not read image file: {path} ({e})");
        }
        let measurements = Vec::new();
        self.canvas.set_measurements(measurements);
        self.canvas.set_selected_index(self.selected_row.map(|r| r as i64));
    }

    fn station(&self) -> &Station {
        &self.project.stations[self.current_station]
    }

    fn generate_points_current(&mut self) {
        let Some(idx) = self.current_ann else { return };
        let border_rect = self.project.border_rect.clone();
        let border_polygon = self.project.border_polygon.clone();
        let dist = DISTRIBUTIONS[self.distribution];
        let ann = &mut self.project.stations[self.current_station].annotations[idx];
        let (w, h) = (if ann.image_width > 0 { ann.image_width as f64 } else { 1000.0 }, if ann.image_height > 0 { ann.image_height as f64 } else { 1000.0 });
        match generate_points(w, h, self.point_count as usize, dist, self.border as f64, border_rect.as_deref(), border_polygon.as_deref()) {
            Ok(points) => {
                let n = points.len();
                ann.points = points;
                self.status = format!("Generated {n} points");
            }
            Err(e) => self.pending_error = Some(e.to_string()),
        }
    }

    fn generate_points_all(&mut self) {
        let border_rect = self.project.border_rect.clone();
        let border_polygon = self.project.border_polygon.clone();
        let dist = DISTRIBUTIONS[self.distribution].to_string();
        let count = self.point_count as usize;
        let border = self.border as f64;
        let mut total = 0;
        for station in &mut self.project.stations {
            for ann in &mut station.annotations {
                let (w, h) = (if ann.image_width > 0 { ann.image_width as f64 } else { 1000.0 }, if ann.image_height > 0 { ann.image_height as f64 } else { 1000.0 });
                if let Ok(points) = generate_points(w, h, count, &dist, border, border_rect.as_deref(), border_polygon.as_deref()) {
                    ann.points = points;
                    total += 1;
                }
            }
        }
        self.status = format!("Points generated for all {total} images");
    }

    fn label_selected_point(&mut self, code: &str) {
        let Some(row) = self.selected_row else { return };
        let Some(idx) = self.current_ann else { return };
        let ann = &mut self.project.stations[self.current_station].annotations[idx];
        if row >= ann.points.len() {
            return;
        }
        ann.points[row].label = Some(code.to_string());
        self.canvas.set_selected_index(Some(row as i64));
        let next_row = row + 1;
        if next_row < ann.points.len() {
            self.selected_row = Some(next_row);
            self.canvas.set_selected_index(Some(next_row as i64));
        }
    }

    fn handle_canvas_events(&mut self, events: Vec<CanvasEvent>) {
        for ev in events {
            match ev {
                CanvasEvent::StatusMessage(msg) => self.status = msg,
                CanvasEvent::PointSelected(idx) => self.selected_row = Some(idx as usize),
                CanvasEvent::PointLabeled(_idx, _label) => {}
                CanvasEvent::BorderDefined(x0, y0, x1, y1) => {
                    self.project.border_rect = Some(vec![x0 as f64, y0 as f64, x1 as f64, y1 as f64]);
                    self.project.border_polygon = None;
                    self.status = format!("Border set: ({x0}, {y0}) -> ({x1}, {y1})");
                }
                CanvasEvent::BorderPolygonDefined(poly) => {
                    let n = poly.len();
                    self.project.border_polygon = Some(poly.into_iter().map(|(x, y)| vec![x, y]).collect());
                    self.project.border_rect = None;
                    self.status = format!("Polygon border set: {n} points");
                }
                CanvasEvent::MeasurementDrawn(_) => {}
                CanvasEvent::PreviewUpdated(_) => {}
            }
        }
    }

    fn quick_stats_text(&self) -> String {
        let Some(summary) = project_summary(&self.project) else { return "-".to_string() };
        let mut lines = vec![
            format!("S: {}", summary.species_richness),
            format!("H': {}", summary.shannon_diversity),
            format!("J': {}", summary.pielou_evenness),
            format!("1-D: {}", summary.simpson_diversity),
            String::new(),
        ];
        for (label, pct) in summary.coverage.iter().take(5) {
            lines.push(format!("{label}: {pct}%"));
        }
        if let Some(idx) = self.current_ann {
            if let Some(ann) = self.station().annotations.get(idx) {
                if ann.scale_factor > 1.0 {
                    if let Some(area) = photo_area(ann) {
                        lines.push(format!("\n{area:.1} {}\u{b2}/photo", ann.scale_unit));
                    }
                }
            }
        }
        lines.join("\n")
    }

    fn progress_text(&self) -> (i64, i64) {
        let Some(idx) = self.current_ann else { return (0, 0) };
        let Some(ann) = self.station().annotations.get(idx) else { return (0, 0) };
        match self.progress_scope {
            ProgressScope::Image => (ann.labeled_count() as i64, ann.points.len() as i64),
            ProgressScope::Station => (self.station().labeled_points() as i64, self.station().total_points() as i64),
            ProgressScope::Project => {
                let total: usize = self.project.annotations().iter().map(|a| a.points.len()).sum();
                let labeled: usize = self.project.annotations().iter().map(|a| a.labeled_count()).sum();
                (labeled as i64, total as i64)
            }
        }
    }

    fn add_images(&mut self) {
        let Some(paths) = rfd::FileDialog::new().add_filter("Images", &["jpg", "jpeg", "png", "tif", "tiff", "bmp"]).pick_files() else { return };
        let existing: std::collections::HashSet<String> = self.project.annotations().iter().map(|a| a.image_path.clone()).collect();
        let mut added = 0;
        for p in paths {
            let p = p.to_string_lossy().into_owned();
            if !existing.contains(&p) {
                let mut ann = ImageAnnotation::new(p.clone());
                if let Ok((w, h)) = image::image_dimensions(&p) {
                    ann.image_width = w as i64;
                    ann.image_height = h as i64;
                }
                self.project.stations[self.current_station].annotations.push(ann);
                added += 1;
            }
        }
        self.status = format!("Added {added} image(s) to {}", self.station().name);
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();

        egui::Panel::top("pc_menu").show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("New Project").clicked() {
                        self.project = Project::new("Untitled Project");
                        load_default_codes(&mut self.project);
                        self.project.stations.push(Station::new("Station 1"));
                        self.current_ann = None;
                        self.current_station = 0;
                        self.status = "New project created".to_string();
                        ui.close();
                    }
                    if ui.button("Open Project...").clicked() {
                        if let Some(path) = rfd::FileDialog::new().add_filter("coralX", &["cpce"]).pick_file() {
                            match Project::load(&path) {
                                Ok(p) => {
                                    self.project = p;
                                    self.current_ann = None;
                                    self.current_station = 0;
                                }
                                Err(e) => self.pending_error = Some(format!("Could not open project:\n{e}")),
                            }
                        }
                        ui.close();
                    }
                    if ui.button("Save Project").clicked() {
                        self.save_project();
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Add Images...").clicked() {
                        self.add_images();
                        ui.close();
                    }
                    ui.separator();
                    ui.menu_button("Import", |ui| {
                        if ui.button("Coral Codes... (JSON/CSV)").clicked() {
                            self.start_import_coral_codes();
                            ui.close();
                        }
                        if ui.button("Station Metadata... (CSV)").clicked() {
                            self.start_import_station_metadata();
                            ui.close();
                        }
                        if ui.button("Labeled Points... (CSV/Excel)").clicked() {
                            self.start_import_labeled_points();
                            ui.close();
                        }
                        ui.separator();
                        if ui.button("From CPCe .cpc File(s)...").clicked() {
                            self.start_import_cpce_cpc();
                            ui.close();
                        }
                        if ui.button("From CPCe Excel...").clicked() {
                            self.start_import_cpce_excel();
                            ui.close();
                        }
                    });
                    ui.separator();
                    if ui.button("Export CSV...").clicked() {
                        if let Some(path) = rfd::FileDialog::new().add_filter("CSV", &["csv"]).save_file() {
                            match export_csv(&self.project, &path) {
                                Ok(()) => self.status = format!("Exported CSV: {}", path.display()),
                                Err(e) => self.pending_error = Some(e.to_string()),
                            }
                        }
                        ui.close();
                    }
                    if ui.button("Export Excel...").clicked() {
                        if let Some(path) = rfd::FileDialog::new().add_filter("Excel", &["xlsx"]).save_file() {
                            match export_excel(&self.project, &path, |_, _, _| {}) {
                                Ok(()) => self.status = format!("Excel exported: {}", path.display()),
                                Err(e) => self.pending_error = Some(e.to_string()),
                            }
                        }
                        ui.close();
                    }
                    if ui.button("Export Coral Codes...").clicked() {
                        if self.project.coral_codes.is_empty() {
                            self.pending_info = Some(("Export Coral Codes".into(), "No coral codes to export.".into()));
                        } else if let Some(path) = rfd::FileDialog::new().add_filter("JSON", &["json"]).add_filter("CSV", &["csv"]).save_file() {
                            match export_coral_codes(&self.project, &path) {
                                Ok(()) => self.status = format!("Exported {} coral codes to: {}", self.project.coral_codes.len(), path.display()),
                                Err(e) => self.pending_error = Some(e.to_string()),
                            }
                        }
                        ui.close();
                    }
                });
                ui.menu_button("Image", |ui| {
                    if ui.button("Calibrate Scale...").clicked() {
                        self.open_calibration(&ctx);
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Generate Points (This Image)").clicked() {
                        self.generate_points_current();
                        ui.close();
                    }
                    if ui.button("Generate Points (All Images)").clicked() {
                        self.generate_points_all();
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("AI Auto-Label...").clicked() {
                        self.ai_label_dialog = Some(AiLabelDialog::new(self.project.coral_codes.clone()));
                        ui.close();
                    }
                });
                ui.menu_button("View", |ui| {
                    if ui.button("Zoom In").clicked() {
                        self.canvas.zoom_in();
                        ui.close();
                    }
                    if ui.button("Zoom Out").clicked() {
                        self.canvas.zoom_out();
                        ui.close();
                    }
                    if ui.button("Fit to Window").clicked() {
                        self.canvas.zoom_fit(ui.max_rect());
                        ui.close();
                    }
                });
                ui.menu_button("Analisa", |ui| {
                    if ui.button("Analisa Lanjutan...").clicked() {
                        self.analysis_dialog = Some(AnalysisDialog::new(&self.project));
                        ui.close();
                    }
                    if ui.button("Stats...").clicked() {
                        self.stats_open = true;
                        ui.close();
                    }
                });
            });
        });

        egui::Panel::top("pc_toolbar").show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.button("+ Add Images").clicked() {
                    self.add_images();
                }
                ui.separator();
                if ui.button("Generate Points").clicked() {
                    self.generate_points_current();
                }
                if ui.button("Generate All").clicked() {
                    self.generate_points_all();
                }
                ui.separator();
                if ui.button("Zoom In").clicked() {
                    self.canvas.zoom_in();
                }
                if ui.button("Zoom Out").clicked() {
                    self.canvas.zoom_out();
                }
                if ui.button("Fit").clicked() {
                    self.canvas.zoom_fit(ui.max_rect());
                }
                ui.separator();
                if ui.button("Calibrate").clicked() {
                    self.open_calibration(&ctx);
                }
                ui.separator();
                if ui.button("AI Label").clicked() {
                    self.ai_label_dialog = Some(AiLabelDialog::new(self.project.coral_codes.clone()));
                }
                ui.separator();
                if ui.button("Export Excel").clicked() {
                    if let Some(path) = rfd::FileDialog::new().add_filter("Excel", &["xlsx"]).save_file() {
                        let _ = export_excel(&self.project, &path, |_, _, _| {});
                        self.status = format!("Excel exported: {}", path.display());
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let (labeled, total) = self.progress_text();
                    ui.label(format!("{labeled}/{total}"));
                    egui::ComboBox::from_id_salt("progress_scope")
                        .selected_text(match self.progress_scope {
                            ProgressScope::Image => "Image",
                            ProgressScope::Station => "Station",
                            ProgressScope::Project => "Project",
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.progress_scope, ProgressScope::Image, "Image");
                            ui.selectable_value(&mut self.progress_scope, ProgressScope::Station, "Station");
                            ui.selectable_value(&mut self.progress_scope, ProgressScope::Project, "Project");
                        });
                });
            });
        });

        egui::Panel::bottom("pc_status").show(ui, |ui| {
            ui.label(&self.status);
        });

        egui::Panel::bottom("pc_codes").exact_size(160.0).show(ui, |ui| {
            self.show_codes_panel(ui);
        });

        egui::Panel::left("pc_left").exact_size(240.0).show(ui, |ui| {
            self.show_left_panel(ui, &ctx);
        });

        egui::Panel::right("pc_right").exact_size(260.0).show(ui, |ui| {
            self.show_right_panel(ui);
        });

        egui::CentralPanel::default().show(ui, |ui| {
            if let Some(idx) = self.current_ann {
                let station = &mut self.project.stations[self.current_station];
                if let Some(ann) = station.annotations.get_mut(idx) {
                    let mut view = AnnotationView { points: &mut ann.points, scale_factor: ann.scale_factor, scale_unit: &ann.scale_unit };
                    let events = self.canvas.show(ui, &mut view);
                    self.handle_canvas_events(events);
                }
            } else {
                ui.centered_and_justified(|ui| ui.label("Add images to begin"));
            }
        });

        self.show_dialogs(&ctx);
    }

    fn show_codes_panel(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Coral Codes");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Groups").clicked() {
                    self.manage_groups_dialog = Some(ManageGroupsEdit {
                        rows: self.project.coral_groups.iter().map(|g| (g.name.clone(), g.codes.join(", "))).collect(),
                    });
                }
                if ui.button("+ Code").clicked() {
                    self.add_code_dialog = Some(AddCodeEdit { code: String::new(), desc: String::new() });
                }
                egui::ComboBox::from_id_salt("sort_combo").selected_text(if self.sort_az { "A -> Z" } else { "Frequency" }).show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.sort_az, false, "Frequency");
                    ui.selectable_value(&mut self.sort_az, true, "A -> Z");
                });
            });
        });

        let mut freq: HashMap<String, i64> = HashMap::new();
        if let Some(idx) = self.current_ann {
            if let Some(ann) = self.station().annotations.get(idx) {
                for p in &ann.points {
                    if let Some(l) = &p.label {
                        *freq.entry(l.clone()).or_insert(0) += 1;
                    }
                }
            }
        }

        let mut grouped: std::collections::HashSet<String> = std::collections::HashSet::new();
        for g in &self.project.coral_groups {
            grouped.extend(g.codes.iter().cloned());
        }
        let ungrouped: Vec<String> = self.project.coral_codes.keys().filter(|c| !grouped.contains(*c)).cloned().collect();

        let mut display_groups: Vec<(String, Vec<String>)> = self.project.coral_groups.iter().map(|g| (g.name.clone(), g.codes.clone())).collect();
        if !ungrouped.is_empty() {
            display_groups.push(("Other".to_string(), ungrouped));
        }

        let mut clicked_code: Option<String> = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (name, codes) in &display_groups {
                let mut group_codes: Vec<&String> = codes.iter().filter(|c| self.project.coral_codes.contains_key(c.as_str())).collect();
                if group_codes.is_empty() {
                    continue;
                }
                if self.sort_az {
                    group_codes.sort();
                } else {
                    group_codes.sort_by(|a, b| freq.get(*b).unwrap_or(&0).cmp(freq.get(*a).unwrap_or(&0)));
                }
                ui.label(egui::RichText::new(name).small().strong());
                ui.horizontal_wrapped(|ui| {
                    for code in group_codes {
                        let count = freq.get(code).copied().unwrap_or(0);
                        let desc = self.project.coral_codes.get(code).cloned().unwrap_or_default();
                        if ui.button(format!("{code}\n{count}")).on_hover_text(format!("{code} - {desc}")).clicked() {
                            clicked_code = Some(code.clone());
                        }
                    }
                });
            }
        });
        if let Some(code) = clicked_code {
            self.label_selected_point(&code);
        }
    }

    fn show_left_panel(&mut self, ui: &mut egui::Ui, ctx: &Context) {
        ui.horizontal(|ui| {
            ui.label("Images");
            ui.checkbox(&mut self.filter_incomplete, "Incomplete only");
        });
        ui.horizontal(|ui| {
            if ui.button("+ Station").clicked() {
                let n = self.project.stations.len() + 1;
                self.project.stations.push(Station::new(format!("Station {n}")));
                self.current_station = self.project.stations.len() - 1;
            }
            if ui.button("Edit").clicked() {
                let idx = self.current_station;
                let st = &self.project.stations[idx];
                self.station_meta_dialog = Some(StationMetaEdit {
                    station_idx: idx,
                    name: st.name.clone(),
                    depth: st.depth_m.map(|d| d.to_string()).unwrap_or_default(),
                    date: st.date.clone().unwrap_or_default(),
                    lat: st.gps_lat.map(|d| d.to_string()).unwrap_or_default(),
                    lon: st.gps_lon.map(|d| d.to_string()).unwrap_or_default(),
                    notes: st.notes.clone(),
                });
            }
        });

        let mut select: Option<(usize, usize)> = None;
        let mut delete_station: Option<usize> = None;
        let mut remove_image: Option<(usize, usize)> = None;
        egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
            for (si, station) in self.project.stations.iter().enumerate() {
                let labeled = station.labeled_points();
                let total = station.total_points();
                let header = egui::CollapsingHeader::new(format!("{}  [{labeled}/{total}]", station.name)).default_open(true).show(ui, |ui| {
                    for (ai, ann) in station.annotations.iter().enumerate() {
                        let complete = ann.is_complete() && !ann.points.is_empty();
                        if self.filter_incomplete && complete {
                            continue;
                        }
                        let name = std::path::Path::new(&ann.image_path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                        let mark = if complete { "\u{2713}" } else { "\u{25CB}" };
                        let is_current = self.current_station == si && self.current_ann == Some(ai);
                        let resp = ui.selectable_label(is_current, format!("{mark} {name}"));
                        if resp.clicked() {
                            select = Some((si, ai));
                        }
                        resp.context_menu(|ui| {
                            if ui.button("Remove Image").clicked() {
                                remove_image = Some((si, ai));
                                ui.close();
                            }
                        });
                    }
                });
                header.header_response.context_menu(|ui| {
                    if ui.button("Add Images Here").clicked() {
                        ui.close();
                    }
                    if self.project.stations.len() > 1 && ui.button("Delete Station").clicked() {
                        delete_station = Some(si);
                        ui.close();
                    }
                });
            }
        });
        if let Some((si, ai)) = select {
            self.current_station = si;
            self.current_ann = Some(ai);
            self.selected_row = None;
            self.reload_canvas(ctx);
        }
        if let Some(si) = delete_station {
            let st = &self.project.stations[si];
            let n_images = st.annotations.len();
            let mut msg = format!("Delete '{}'?", st.name);
            if n_images > 0 {
                msg.push_str(&format!("\n\nThis station has {n_images} image(s) with {} labeled point(s). All data will be lost.", st.labeled_points()));
            }
            self.confirm = Some(Confirm { title: "Delete Station".into(), message: msg, on_yes: ConfirmAction::DeleteStation(si) });
        }
        if let Some((si, ai)) = remove_image {
            let name = std::path::Path::new(&self.project.stations[si].annotations[ai].image_path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            self.confirm = Some(Confirm {
                title: "Remove Image".into(),
                message: format!("Remove '{name}' from this project?"),
                on_yes: ConfirmAction::RemoveImage(si, ai),
            });
        }

        ui.separator();
        ui.collapsing("Point Settings", |ui| {
            ui.horizontal(|ui| {
                ui.label("Count:");
                ui.add(egui::DragValue::new(&mut self.point_count).range(1..=500));
            });
            ui.horizontal(|ui| {
                ui.label("Distribution:");
                egui::ComboBox::from_id_salt("dist_combo").selected_text(DISTRIBUTIONS[self.distribution]).show_ui(ui, |ui| {
                    for (i, d) in DISTRIBUTIONS.iter().enumerate() {
                        ui.selectable_value(&mut self.distribution, i, *d);
                    }
                });
            });
            ui.horizontal(|ui| {
                ui.label("Border:");
                if ui.add(egui::DragValue::new(&mut self.border).range(0..=500).suffix(" px")).changed() {
                    self.project.border_rect = None;
                    self.canvas.set_border_rect(None);
                    self.canvas.set_border(self.border);
                }
            });
            ui.horizontal(|ui| {
                if ui.button("2-pt").on_hover_text("Click 2 diagonal corners to set rectangular border").clicked() {
                    self.canvas.start_border_drawing(BorderMode::TwoPoint);
                }
                if ui.button("4-pt").on_hover_text("Click to add polygon vertices").clicked() {
                    self.canvas.start_border_drawing(BorderMode::Polygon);
                }
                if ui.button("Clear").clicked() {
                    self.project.border_rect = None;
                    self.project.border_polygon = None;
                    self.canvas.set_border_rect(None);
                    self.canvas.set_border_polygon(None);
                    self.canvas.set_border(self.border);
                    self.status = "Custom border cleared".to_string();
                }
            });
        });
    }

    fn show_right_panel(&mut self, ui: &mut egui::Ui) {
        ui.collapsing("Quick Stats", |ui| {
            ui.label(self.quick_stats_text());
        });
        ui.separator();
        ui.label("Points");

        let Some(idx) = self.current_ann else {
            ui.weak("No image selected");
            return;
        };
        let n_points = self.station().annotations[idx].points.len();
        let mut select_row = None;
        let mut label_change: Option<(usize, String)> = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Grid::new("points_table").striped(true).show(ui, |ui| {
                ui.strong("#");
                ui.strong("Label");
                ui.end_row();
                for row in 0..n_points {
                    let p = &self.station().annotations[idx].points[row];
                    let selected = self.selected_row == Some(row);
                    if ui.selectable_label(selected, (row + 1).to_string()).clicked() {
                        select_row = Some(row);
                    }
                    let mut label = p.label.clone().unwrap_or_default();
                    if ui.add(egui::TextEdit::singleline(&mut label).desired_width(60.0)).changed() {
                        label_change = Some((row, label));
                    }
                    ui.end_row();
                }
            });
        });
        if let Some(row) = select_row {
            self.selected_row = Some(row);
            self.canvas.set_selected_index(Some(row as i64));
        }
        if let Some((row, text)) = label_change {
            let text = text.trim().to_uppercase();
            let ann = &mut self.project.stations[self.current_station].annotations[idx];
            if row < ann.points.len() {
                ann.points[row].label = if text.is_empty() { None } else if self.project.coral_codes.contains_key(&text) { Some(text) } else { None };
            }
        }
    }

    fn open_calibration(&mut self, ctx: &Context) {
        let Some(idx) = self.current_ann else {
            self.pending_info = Some(("Calibrate Scale".into(), "Please select an image first.".into()));
            return;
        };
        let ann = self.station().annotations[idx].clone();
        match image::open(&ann.image_path) {
            Ok(img) => {
                let rgba = img.to_rgba8();
                self.calibration_dialog = Some(CalibrationDialog::new(ctx, &ann, rgba.as_raw()));
            }
            Err(e) => self.pending_error = Some(format!("Cannot load image: {e}")),
        }
    }

    fn save_project(&mut self) {
        let path = self.project.save_path.clone().map(std::path::PathBuf::from).or_else(|| rfd::FileDialog::new().add_filter("coralX", &["cpce"]).save_file());
        let Some(path) = path else { return };
        match self.project.save(&path) {
            Ok(()) => self.status = format!("Saved: {}", path.display()),
            Err(e) => self.pending_error = Some(format!("Could not save project:\n{e}")),
        }
    }

    fn start_import_coral_codes(&mut self) {
        let Some(path) = rfd::FileDialog::new().add_filter("Supported", &["txt", "json", "csv", "tsv"]).pick_file() else { return };
        let (codes, groups, result) = import_coral_codes(&path);
        if !result.success {
            self.pending_error = Some(result.message);
            return;
        }
        let has_groups = !groups.is_empty();
        self.coral_codes_merge = Some((codes, groups, CoralCodesMergeDialog::new(0, self.project.coral_codes.len(), has_groups)));
        self.import_result = Some(ImportResultDialog::new("Import Coral Codes", result.message, result.warnings));
    }

    fn start_import_station_metadata(&mut self) {
        let Some(path) = rfd::FileDialog::new().add_filter("CSV", &["csv"]).pick_file() else { return };
        let (incoming, result) = import_station_metadata(&path);
        if !result.success {
            self.pending_error = Some(result.message);
            return;
        }
        let names: Vec<String> = incoming.iter().map(|s| s.name.clone()).collect();
        let existing: Vec<String> = self.project.stations.iter().map(|s| s.name.clone()).collect();
        self.station_merge = Some((incoming, StationMergeDialog::new(&names, &existing)));
    }

    fn start_import_labeled_points(&mut self) {
        if self.project.annotations().is_empty() {
            self.pending_info = Some(("Import Labeled Points".into(), "Add images to the project first, then import labels.".into()));
            return;
        }
        let Some(path) = rfd::FileDialog::new().add_filter("Supported", &["csv", "xlsx", "xls"]).pick_file() else { return };
        let result = import_labeled_points(&path, &mut self.project);
        if !result.success {
            self.pending_error = Some(result.message);
            return;
        }
        self.import_result = Some(ImportResultDialog::new("Import Labeled Points", result.message, result.warnings));
    }

    fn start_import_cpce_cpc(&mut self) {
        let Some(paths) = rfd::FileDialog::new().add_filter("CPCe", &["cpc"]).pick_files() else { return };
        let target_idx = if self.project.stations.is_empty() {
            self.project.stations.push(Station::new("Imported Station"));
            self.project.stations.len() - 1
        } else {
            self.current_station
        };

        let mut warnings = Vec::new();
        let mut success = 0;
        let mut total_points = 0;
        for p in &paths {
            let (ann, result) = import_cpce_cpc(p, None);
            let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            if !result.success {
                warnings.push(format!("{name}: {}", result.message));
                continue;
            }
            if let Some(ann) = ann {
                total_points += ann.points.len();
                self.project.stations[target_idx].annotations.push(ann);
                success += 1;
            }
            warnings.extend(result.warnings.iter().map(|w| format!("{name}: {w}")));
        }
        if success == 0 {
            self.pending_error = Some(format!("No files were imported.\n\n{}", warnings.join("\n")));
        } else {
            let summary = format!("Imported {success} of {} .cpc file(s) - {total_points} total points - into station '{}'.", paths.len(), self.project.stations[target_idx].name);
            self.import_result = Some(ImportResultDialog::new("Import CPCe .cpc", summary, warnings));
        }
    }

    fn start_import_cpce_excel(&mut self) {
        let Some(path) = rfd::FileDialog::new().add_filter("Excel", &["xlsx", "xls"]).pick_file() else { return };
        let (project, result) = import_cpce_excel(&path);
        if !result.success {
            self.pending_error = Some(result.message);
            return;
        }
        let Some(project) = project else { return };
        let n_st = project.stations.len();
        let n_img: usize = project.stations.iter().map(|s| s.annotations.len()).sum();
        let n_pts: usize = project.annotations().iter().map(|a| a.points.len()).sum();
        self.cpce_import = Some((project, CpceImportDialog::new(n_st, n_img, n_pts, true)));
        self.import_result = Some(ImportResultDialog::new("Import from CPCe Excel", result.message, result.warnings));
    }

    fn on_ai_results(&mut self, results: Vec<LabelResult>) {
        let mut label_map: HashMap<String, HashMap<i64, String>> = HashMap::new();
        for r in results {
            if let Some(code) = r.mapped_code {
                label_map.entry(r.annotation_path).or_default().insert(r.point_index, code);
            }
        }
        let mut labeled_count = 0;
        for station in &mut self.project.stations {
            for ann in &mut station.annotations {
                if let Some(updates) = label_map.get(&ann.image_path) {
                    for p in &mut ann.points {
                        if let Some(code) = updates.get(&p.index) {
                            p.label = Some(code.clone());
                            labeled_count += 1;
                        }
                    }
                }
            }
        }
        self.status = format!("AI auto-label complete: {labeled_count} point(s) labeled.");
    }

    fn show_dialogs(&mut self, ctx: &Context) {
        if let Some(dlg) = &mut self.calibration_dialog {
            if let Some(result) = dlg.show(ctx) {
                if let (Ok(r), Some(idx)) = (result, self.current_ann) {
                    let station = &mut self.project.stations[self.current_station];
                    let targets: Vec<usize> = if r.apply_to_all { (0..station.annotations.len()).collect() } else { vec![idx] };
                    for i in targets {
                        station.annotations[i].scale_factor = r.scale_factor;
                        station.annotations[i].scale_unit = r.unit.clone();
                    }
                    self.status = format!("Scale set: {:.3} px/{}", r.scale_factor, r.unit);
                }
                self.calibration_dialog = None;
            }
        }

        if let Some(dlg) = &mut self.ai_label_dialog {
            if let Some(run_config) = dlg.show(ctx) {
                let annotations: Vec<(String, ImageAnnotation)> = match run_config.scope {
                    "station" => self.station().annotations.iter().map(|a| (a.image_path.clone(), a.clone())).collect(),
                    "project" => self.project.annotations().into_iter().map(|a| (a.image_path.clone(), a.clone())).collect(),
                    _ => self.current_ann.map(|idx| vec![(self.station().annotations[idx].image_path.clone(), self.station().annotations[idx].clone())]).unwrap_or_default(),
                };
                let annotations: Vec<(String, ImageAnnotation)> = annotations.into_iter().filter(|(_, a)| !a.points.is_empty()).collect();
                if annotations.is_empty() {
                    self.pending_info = Some(("AI Auto-Label".into(), "No points found in the selected scope.".into()));
                } else {
                    self.ai_progress_dialog = Some(AiProgressDialog::spawn(
                        run_config.labeler,
                        annotations,
                        run_config.class_mapping,
                        run_config.conf_threshold,
                        run_config.crop_size,
                        run_config.overwrite_labeled,
                    ));
                }
                self.ai_label_dialog = None;
            }
        }

        if let Some(dlg) = &mut self.ai_progress_dialog {
            dlg.show(ctx);
            if dlg.is_finished() {
                if let Some(dlg) = self.ai_progress_dialog.take() {
                    self.on_ai_results(dlg.take_result());
                }
            }
        }

        if let Some(dlg) = &mut self.analysis_dialog {
            if let Some(result) = dlg.show(ctx, &self.project) {
                match result {
                    Ok(path) => self.pending_info = Some(("Selesai".into(), format!("Hasil disimpan ke:\n{}", path.display()))),
                    Err(e) if e != "cancelled" => self.pending_error = Some(e),
                    _ => {}
                }
                self.analysis_dialog = None;
            }
        }

        if let Some(edit) = &mut self.add_code_dialog {
            let mut close = false;
            egui::Window::new("Add Coral Code").collapsible(false).show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Code:");
                    ui.text_edit_singleline(&mut edit.code);
                });
                ui.horizontal(|ui| {
                    ui.label("Description:");
                    ui.text_edit_singleline(&mut edit.desc);
                });
                ui.horizontal(|ui| {
                    if ui.button("OK").clicked() && !edit.code.trim().is_empty() {
                        self.project.coral_codes.insert(edit.code.trim().to_uppercase(), edit.desc.clone());
                        close = true;
                    }
                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                });
            });
            if close {
                self.add_code_dialog = None;
            }
        }

        if let Some(edit) = &mut self.manage_groups_dialog {
            let mut close = false;
            let mut save = false;
            egui::Window::new("Manage Code Groups").collapsible(false).show(ctx, |ui| {
                let mut remove_idx = None;
                for (i, (name, codes)) in edit.rows.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        ui.text_edit_singleline(name);
                        ui.text_edit_singleline(codes);
                        if ui.button("x").clicked() {
                            remove_idx = Some(i);
                        }
                    });
                }
                if let Some(i) = remove_idx {
                    edit.rows.remove(i);
                }
                if ui.button("+ Add Group").clicked() {
                    edit.rows.push(("New Group".to_string(), String::new()));
                }
                let all_codes: std::collections::HashSet<&String> = self.project.coral_codes.keys().collect();
                let mut used: std::collections::HashSet<String> = std::collections::HashSet::new();
                for (_, codes) in &edit.rows {
                    used.extend(codes.split(',').map(|c| c.trim().to_uppercase()).filter(|c| !c.is_empty()));
                }
                let ungrouped: Vec<&&String> = all_codes.iter().filter(|c| !used.contains(c.as_str())).collect();
                if !ungrouped.is_empty() {
                    ui.weak(format!("Ungrouped codes: {}", ungrouped.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")));
                }
                ui.horizontal(|ui| {
                    if ui.button("OK").clicked() {
                        save = true;
                        close = true;
                    }
                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                });
            });
            if save {
                self.project.coral_groups = edit
                    .rows
                    .iter()
                    .filter(|(name, _)| !name.trim().is_empty())
                    .map(|(name, codes)| CoralGroup {
                        name: name.trim().to_string(),
                        codes: codes.split(',').map(|c| c.trim().to_uppercase()).filter(|c| !c.is_empty()).collect(),
                        color: None,
                    })
                    .collect();
            }
            if close {
                self.manage_groups_dialog = None;
            }
        }

        if let Some(edit) = &mut self.station_meta_dialog {
            let mut close = false;
            let mut save = false;
            egui::Window::new("Edit Station").collapsible(false).show(ctx, |ui| {
                egui::Grid::new("station_edit_grid").show(ui, |ui| {
                    ui.label("Name:");
                    ui.text_edit_singleline(&mut edit.name);
                    ui.end_row();
                    ui.label("Depth (m):");
                    ui.text_edit_singleline(&mut edit.depth);
                    ui.end_row();
                    ui.label("Date:");
                    ui.text_edit_singleline(&mut edit.date);
                    ui.end_row();
                    ui.label("GPS Lat:");
                    ui.text_edit_singleline(&mut edit.lat);
                    ui.end_row();
                    ui.label("GPS Lon:");
                    ui.text_edit_singleline(&mut edit.lon);
                    ui.end_row();
                    ui.label("Notes:");
                    ui.text_edit_multiline(&mut edit.notes);
                    ui.end_row();
                });
                ui.horizontal(|ui| {
                    if ui.button("OK").clicked() {
                        save = true;
                        close = true;
                    }
                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                });
            });
            if save {
                let st = &mut self.project.stations[edit.station_idx];
                st.name = if edit.name.trim().is_empty() { "Station".to_string() } else { edit.name.trim().to_string() };
                st.depth_m = edit.depth.trim().parse().ok();
                st.date = if edit.date.trim().is_empty() { None } else { Some(edit.date.trim().to_string()) };
                st.gps_lat = edit.lat.trim().parse().ok();
                st.gps_lon = edit.lon.trim().parse().ok();
                st.notes = edit.notes.clone();
            }
            if close {
                self.station_meta_dialog = None;
            }
        }

        if let Some((codes, groups, dlg)) = &mut self.coral_codes_merge {
            if let Some(outcome) = dlg.show(ctx) {
                if let MergeOutcome::Confirmed = outcome {
                    if dlg.merge {
                        self.project.coral_codes.extend(codes.clone());
                    } else {
                        self.project.coral_codes = codes.clone();
                    }
                    if dlg.import_groups && !groups.is_empty() {
                        self.project.coral_groups = groups
                            .iter()
                            .map(|g| CoralGroup { name: g.name.clone(), codes: g.codes.clone(), color: g.color.clone() })
                            .collect();
                    }
                }
                self.coral_codes_merge = None;
            }
        }

        if let Some((incoming, dlg)) = &mut self.station_merge {
            if let Some(outcome) = dlg.show(ctx) {
                if let MergeOutcome::Confirmed = outcome {
                    let mut added = 0;
                    let mut updated = 0;
                    for meta in incoming.iter() {
                        if let Some(st) = self.project.stations.iter_mut().find(|s| s.name == meta.name) {
                            if dlg.update_existing {
                                if meta.depth_m.is_some() {
                                    st.depth_m = meta.depth_m;
                                }
                                if meta.date.is_some() {
                                    st.date = meta.date.clone();
                                }
                                if meta.gps_lat.is_some() {
                                    st.gps_lat = meta.gps_lat;
                                }
                                if meta.gps_lon.is_some() {
                                    st.gps_lon = meta.gps_lon;
                                }
                                if !meta.notes.is_empty() {
                                    st.notes = meta.notes.clone();
                                }
                                updated += 1;
                            }
                        } else {
                            self.project.stations.push(Station {
                                name: meta.name.clone(),
                                depth_m: meta.depth_m,
                                date: meta.date.clone(),
                                gps_lat: meta.gps_lat,
                                gps_lon: meta.gps_lon,
                                notes: meta.notes.clone(),
                                annotations: Vec::new(),
                            });
                            added += 1;
                        }
                    }
                    self.status = format!("Added {added} station(s), updated {updated} station(s).");
                }
                self.station_merge = None;
            }
        }

        if let Some((project, dlg)) = &mut self.cpce_import {
            if let Some(outcome) = dlg.show(ctx) {
                if let MergeOutcome::Confirmed = outcome {
                    if dlg.open_as_new {
                        self.project = project.clone();
                        self.current_ann = None;
                        self.current_station = 0;
                    } else {
                        self.project.stations.extend(project.stations.clone());
                    }
                }
                self.cpce_import = None;
            }
        }

        if let Some(dlg) = &self.import_result {
            if dlg.show(ctx) {
                self.import_result = None;
            }
        }

        if self.stats_open {
            self.show_stats_window(ctx);
        }

        if let Some(confirm) = &self.confirm {
            let mut result = None;
            egui::Window::new(&confirm.title).collapsible(false).show(ctx, |ui| {
                ui.label(&confirm.message);
                ui.horizontal(|ui| {
                    if ui.button("Yes").clicked() {
                        result = Some(true);
                    }
                    if ui.button("Cancel").clicked() {
                        result = Some(false);
                    }
                });
            });
            if let Some(yes) = result {
                if yes {
                    match &confirm.on_yes {
                        ConfirmAction::DeleteStation(si) => {
                            self.project.stations.remove(*si);
                            if self.current_station >= self.project.stations.len() {
                                self.current_station = self.project.stations.len().saturating_sub(1);
                            }
                            self.current_ann = None;
                        }
                        ConfirmAction::RemoveImage(si, ai) => {
                            self.project.stations[*si].annotations.remove(*ai);
                            if self.current_station == *si && self.current_ann == Some(*ai) {
                                self.current_ann = None;
                            }
                        }
                    }
                }
                self.confirm = None;
            }
        }

        if let Some(msg) = self.pending_error.clone() {
            egui::Window::new("Error").collapsible(false).show(ctx, |ui| {
                ui.label(&msg);
                if ui.button("OK").clicked() {
                    self.pending_error = None;
                }
            });
        }
        if let Some((title, msg)) = self.pending_info.clone() {
            egui::Window::new(title).collapsible(false).show(ctx, |ui| {
                ui.label(&msg);
                if ui.button("OK").clicked() {
                    self.pending_info = None;
                }
            });
        }
    }

    fn show_stats_window(&mut self, ctx: &Context) {
        let mut open = self.stats_open;
        egui::Window::new("Project Statistics").open(&mut open).default_size([520.0, 420.0]).show(ctx, |ui| {
            let Some(summary) = project_summary(&self.project) else {
                ui.label("No labeled points yet.");
                return;
            };
            egui::CollapsingHeader::new("Overview").default_open(true).show(ui, |ui| {
                ui.label(format!("Project: {}", self.project.name));
                ui.label(format!("Total points: {}", summary.total_points));
                ui.label(format!("Labeled points: {}", summary.labeled_points));
                let calibrated: Vec<&ImageAnnotation> = self.project.annotations().into_iter().filter(|a| a.scale_factor > 1.0).collect();
                if !calibrated.is_empty() {
                    let unit = calibrated[0].scale_unit.clone();
                    let total_area: f64 = calibrated.iter().filter_map(|a| photo_area(a)).sum();
                    ui.label(format!("Calibrated images: {} / {}", calibrated.len(), self.project.annotations().len()));
                    ui.label(format!("Total photo area surveyed: {total_area:.2} {unit}\u{b2}"));
                }
            });
            egui::CollapsingHeader::new("Diversity").show(ui, |ui| {
                egui::Grid::new("diversity_grid").show(ui, |ui| {
                    for (name, val) in [
                        ("Species richness (S)", summary.species_richness.to_string()),
                        ("Shannon diversity (H')", summary.shannon_diversity.to_string()),
                        ("Simpson diversity (1-D)", summary.simpson_diversity.to_string()),
                        ("Pielou evenness (J')", summary.pielou_evenness.to_string()),
                        ("Margalef richness (d)", summary.margalef_richness.to_string()),
                        ("Fisher alpha", summary.fisher_alpha.to_string()),
                    ] {
                        ui.label(name);
                        ui.label(val);
                        ui.end_row();
                    }
                });
            });
            egui::CollapsingHeader::new("% Cover + CI").show(ui, |ui| {
                egui::Grid::new("cover_grid").striped(true).show(ui, |ui| {
                    ui.strong("Code");
                    ui.strong("% Cover");
                    ui.strong("95% CI Low");
                    ui.strong("95% CI High");
                    ui.end_row();
                    for (code, (pct, lo, hi)) in &summary.coverage_ci {
                        ui.label(code);
                        ui.label(format!("{pct}%"));
                        ui.label(format!("{lo}%"));
                        ui.label(format!("{hi}%"));
                        ui.end_row();
                    }
                });
            });
            egui::CollapsingHeader::new("Per Station").show(ui, |ui| {
                let rows = per_station_table(&self.project);
                egui::ScrollArea::horizontal().show(ui, |ui| {
                    egui::Grid::new("per_station_grid").striped(true).show(ui, |ui| {
                        if let Some(first) = rows.first() {
                            for (col, _) in first {
                                ui.strong(col);
                            }
                            ui.end_row();
                            for row in &rows {
                                for (_, val) in row {
                                    ui.label(val.to_string());
                                }
                                ui.end_row();
                            }
                        }
                    });
                });
            });
        });
        self.stats_open = open;
    }
}

fn load_default_codes(project: &mut Project) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../data/coral_codes_default.json");
    let Ok(text) = std::fs::read_to_string(&path) else { return };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else { return };
    if let Some(obj) = value.as_object() {
        if let Some(codes) = obj.get("codes").and_then(|c| c.as_object()) {
            project.coral_codes = codes.iter().filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string()))).collect();
            if let Some(groups) = obj.get("groups").and_then(|g| g.as_array()) {
                project.coral_groups = groups
                    .iter()
                    .filter_map(|g| {
                        let name = g.get("name")?.as_str()?.to_string();
                        let codes = g.get("codes")?.as_array()?.iter().filter_map(|c| c.as_str().map(String::from)).collect();
                        Some(CoralGroup { name, codes, color: None })
                    })
                    .collect();
            }
        } else {
            project.coral_codes = obj.iter().filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string()))).collect();
        }
    }
}
