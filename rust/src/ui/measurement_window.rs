//! Fragment Measurement mode window: wires ImageCanvas + CalibrationDialog +
//! MeasurementLabelDialog together with an image list, tool panel, and
//! measurement table.

use egui::Context;

use crate::core::measurement_exporter::export_measurements_excel;
use crate::models::{ImageAnnotation, Measurement, Project, Station};
use crate::ui::calibration_dialog::CalibrationDialog;
use crate::ui::image_canvas::{AnnotationView, CanvasEvent, ImageCanvas, MeasureMode};
use crate::ui::measurement_label_dialog::{DialogOutcome, MeasurementLabelDialog};

pub struct MeasurementScreen {
    project: Project,
    current_ann: Option<usize>,
    redo_stack: Vec<Measurement>,
    canvas: ImageCanvas,

    tol_display: i64,
    smooth_display: i64,
    eraser_on: bool,
    eraser_radius: i64,

    status: String,
    preview: Option<crate::ui::image_canvas::PreviewInfo>,
    selected_row: Option<usize>,

    calibration_dialog: Option<CalibrationDialog>,
    label_dialog: Option<MeasurementLabelDialog>,
    pending_error: Option<String>,
    pending_info: Option<(String, String)>,
}

impl MeasurementScreen {
    pub fn new(project: Option<Project>) -> Self {
        let mut project = project.unwrap_or_else(|| Project::new("Untitled Measurement"));
        if project.stations.is_empty() {
            project.stations.push(Station::new("Station 1"));
        }
        MeasurementScreen {
            project,
            current_ann: None,
            redo_stack: Vec::new(),
            canvas: ImageCanvas::default(),
            tol_display: 50,
            smooth_display: 1,
            eraser_on: false,
            eraser_radius: 24,
            status: "Ready  |  Select a mode to begin measuring".to_string(),
            preview: None,
            selected_row: None,
            calibration_dialog: None,
            label_dialog: None,
            pending_error: None,
            pending_info: None,
        }
    }

    fn tolerance_internal(display: i64) -> i64 {
        ((display as f64 * 35.0 / 100.0).round() as i64).max(1)
    }
    fn smoothing_internal(display: i64) -> i64 {
        ((display - 1) as f64 * 5.0 / 9.0).round() as i64
    }

    fn station(&self) -> &Station {
        &self.project.stations[0]
    }
    fn station_mut(&mut self) -> &mut Station {
        &mut self.project.stations[0]
    }

    fn load_current_image(&mut self, ctx: &Context) {
        let Some(idx) = self.current_ann else { return };
        let path = self.station().annotations[idx].image_path.clone();
        self.redo_stack.clear();
        self.canvas.cancel_measurement();
        if let Err(e) = self.canvas.load_image(ctx, &path, Vec::new()) {
            self.status = format!("Cannot load: {path} ({e})");
            return;
        }
        let measurements = self.station().annotations[idx].measurements.clone();
        self.canvas.set_measurements(measurements);
        self.canvas.set_measure_tolerance(Self::tolerance_internal(self.tol_display));
        self.canvas.set_measure_smoothing(Self::smoothing_internal(self.smooth_display));
        self.status = format!("Loaded: {path}");
    }

    fn update_calib_label(&self) -> String {
        let Some(idx) = self.current_ann else { return "\u{1F4CF} Calibrate: -".to_string() };
        let ann = &self.station().annotations[idx];
        if ann.scale_factor > 1.0 {
            format!("\u{1F4CF} Calibrate: 1 {} = {:.1} px", ann.scale_unit, ann.scale_factor)
        } else {
            "\u{1F4CF} Calibrate: not calibrated".to_string()
        }
    }

    fn start_measure(&mut self, mode: MeasureMode) {
        let Some(idx) = self.current_ann else {
            self.pending_info = Some(("No Image".into(), "Select an image first.".into()));
            return;
        };
        if self.station().annotations[idx].scale_factor <= 1.0 {
            self.status = "Not calibrated - measuring in pixels".to_string();
        }
        self.eraser_on = false;
        self.canvas.set_eraser_active(false);
        self.canvas.start_measurement(mode);
        self.status = mode.hint().to_string();
        let _ = &mode;
    }

    fn add_images(&mut self) {
        let Some(paths) = rfd::FileDialog::new().add_filter("Images", &["jpg", "jpeg", "png", "tif", "tiff", "bmp"]).pick_files() else { return };
        let was_empty = self.station().annotations.is_empty();
        let existing: std::collections::HashSet<String> = self.station().annotations.iter().map(|a| a.image_path.clone()).collect();
        for p in paths {
            let p = p.to_string_lossy().into_owned();
            if !existing.contains(&p) {
                self.station_mut().annotations.push(ImageAnnotation::new(p));
            }
        }
        if was_empty && !self.station().annotations.is_empty() {
            self.current_ann = Some(0);
        }
    }

    fn handle_canvas_events(&mut self, events: Vec<CanvasEvent>) {
        for ev in events {
            match ev {
                CanvasEvent::StatusMessage(msg) => self.status = msg,
                CanvasEvent::PreviewUpdated(info) => self.preview = info,
                CanvasEvent::MeasurementDrawn(m) => {
                    let species_list = std::mem::take(&mut self.project.species_list);
                    self.project.species_list = species_list;
                    self.label_dialog = Some(MeasurementLabelDialog::new(m));
                }
                CanvasEvent::PointLabeled(..) | CanvasEvent::PointSelected(..) | CanvasEvent::BorderDefined(..) | CanvasEvent::BorderPolygonDefined(..) => {
                    // Point-count-only events; unused in Fragment Measurement mode.
                }
            }
        }
    }

    fn undo(&mut self) {
        if self.canvas.undo_last() {
            return;
        }
        let Some(idx) = self.current_ann else {
            self.status = "Nothing to undo".to_string();
            return;
        };
        let Some(m) = self.station_mut().annotations[idx].measurements.pop() else {
            self.status = "Nothing to undo".to_string();
            return;
        };
        self.status = format!("Undid: {}", if m.label.is_empty() { m.kind.clone() } else { m.label.clone() });
        self.redo_stack.push(m);
        let measurements = self.station().annotations[idx].measurements.clone();
        self.canvas.set_measurements(measurements);
    }

    fn redo(&mut self) {
        let Some(idx) = self.current_ann else { return };
        let Some(m) = self.redo_stack.pop() else {
            self.status = "Nothing to redo".to_string();
            return;
        };
        self.status = format!("Redid: {}", if m.label.is_empty() { m.kind.clone() } else { m.label.clone() });
        self.station_mut().annotations[idx].measurements.push(m);
        let measurements = self.station().annotations[idx].measurements.clone();
        self.canvas.set_measurements(measurements);
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        egui::Panel::top("measure_menu").show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("New Project").clicked() {
                        self.project = Project::new("Untitled Measurement");
                        self.project.stations.push(Station::new("Station 1"));
                        self.current_ann = None;
                        ui.close();
                    }
                    if ui.button("Open Project...").clicked() {
                        if let Some(path) = rfd::FileDialog::new().add_filter("coralX Project", &["cpce"]).pick_file() {
                            match Project::load(&path) {
                                Ok(mut p) => {
                                    if p.stations.is_empty() {
                                        p.stations.push(Station::new("Station 1"));
                                    }
                                    self.project = p;
                                    self.current_ann = None;
                                }
                                Err(e) => self.pending_error = Some(format!("Could not open project:\n{e}")),
                            }
                        }
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Save Project").clicked() {
                        self.save_project(None);
                        ui.close();
                    }
                    if ui.button("Save As...").clicked() {
                        self.save_project(rfd::FileDialog::new().add_filter("coralX Project", &["cpce"]).save_file());
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Export Excel...").clicked() {
                        self.export_excel();
                        ui.close();
                    }
                });
                ui.menu_button("Edit", |ui| {
                    if ui.button("Undo (Ctrl+Z)").clicked() {
                        self.undo();
                        ui.close();
                    }
                    if ui.button("Redo (Ctrl+Shift+Z)").clicked() {
                        self.redo();
                        ui.close();
                    }
                });
                ui.menu_button("Image", |ui| {
                    if ui.button("Add Images...").clicked() {
                        self.add_images();
                        ui.close();
                    }
                    if ui.button("Calibrate Scale...").clicked() {
                        self.open_calibration(&ctx);
                        ui.close();
                    }
                });
            });
        });

        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::Z) && i.modifiers.shift) {
            self.redo();
        } else if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::Z)) {
            self.undo();
        }

        egui::Panel::top("measure_toolbar").show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.button("New").clicked() {
                    self.project = Project::new("Untitled Measurement");
                    self.project.stations.push(Station::new("Station 1"));
                    self.current_ann = None;
                }
                if ui.button("Open").clicked() {
                    if let Some(path) = rfd::FileDialog::new().add_filter("coralX Project", &["cpce"]).pick_file() {
                        if let Ok(mut p) = Project::load(&path) {
                            if p.stations.is_empty() {
                                p.stations.push(Station::new("Station 1"));
                            }
                            self.project = p;
                            self.current_ann = None;
                        }
                    }
                }
                if ui.button("Save").clicked() {
                    self.save_project(None);
                }
                ui.separator();
                if ui.button("+ Add Images").clicked() {
                    self.add_images();
                }
                if ui.button(self.update_calib_label()).clicked() {
                    self.open_calibration(&ctx);
                }
                ui.separator();
                if ui.button("\u{1F50D} Zoom In").clicked() {
                    self.canvas.zoom_in();
                }
                if ui.button("\u{1F50D} Zoom Out").clicked() {
                    self.canvas.zoom_out();
                }
                if ui.button("Fit").clicked() {
                    self.canvas.zoom_fit(ui.max_rect());
                }
                ui.separator();
                if ui.button("Export Excel").clicked() {
                    self.export_excel();
                }
            });
        });

        egui::Panel::bottom("measure_status").show(ui, |ui| {
            ui.label(&self.status);
        });

        egui::Panel::left("measure_left").exact_size(210.0).show(ui, |ui| {
            if ui.add(egui::Button::new("+ Add Images").min_size(egui::vec2(ui.available_width(), 28.0))).clicked() {
                self.add_images();
            }
            ui.label("Images:");
            let mut clicked_row = None;
            egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
                for (i, ann) in self.station().annotations.iter().enumerate() {
                    let name = std::path::Path::new(&ann.image_path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    if ui.selectable_label(self.current_ann == Some(i), name).clicked() {
                        clicked_row = Some(i);
                    }
                }
            });
            if let Some(row) = clicked_row {
                self.current_ann = Some(row);
                self.load_current_image(&ctx);
            }

            ui.separator();
            ui.group(|ui| {
                ui.label("Measure");
                if ui.add(egui::Button::new("\u{1F4CF} Straight Line").min_size(egui::vec2(ui.available_width(), 0.0))).clicked() {
                    self.start_measure(MeasureMode::Line);
                }
                if ui.add(egui::Button::new("\u{3030} Polyline").min_size(egui::vec2(ui.available_width(), 0.0))).clicked() {
                    self.start_measure(MeasureMode::Polyline);
                }
                if ui.add(egui::Button::new("\u{2B20} Polygon (manual)").min_size(egui::vec2(ui.available_width(), 0.0))).clicked() {
                    self.start_measure(MeasureMode::Polygon);
                }
                if ui.add(egui::Button::new("\u{1FA84} Magic Wand").min_size(egui::vec2(ui.available_width(), 0.0))).clicked() {
                    self.start_measure(MeasureMode::Magic);
                }

                ui.label("Magic wand tolerance:");
                if ui.add(egui::Slider::new(&mut self.tol_display, 1..=100)).changed() {
                    self.canvas.set_measure_tolerance(Self::tolerance_internal(self.tol_display));
                }
                ui.label("Contour detail (1=detail, 10=smooth):");
                if ui.add(egui::Slider::new(&mut self.smooth_display, 1..=10)).changed() {
                    self.canvas.set_measure_smoothing(Self::smoothing_internal(self.smooth_display));
                }

                if ui.selectable_label(self.eraser_on, "\u{1F9FD} Eraser").clicked() {
                    self.eraser_on = !self.eraser_on;
                    self.canvas.set_eraser_active(self.eraser_on);
                }
                ui.label("Eraser brush size:");
                if ui.add(egui::Slider::new(&mut self.eraser_radius, 4..=120)).changed() {
                    self.canvas.set_eraser_radius(self.eraser_radius);
                }
            });
        });

        egui::Panel::right("measure_right").exact_size(260.0).show(ui, |ui| {
            ui.label("Measurements (current image):");
            let measurements = self.current_ann.map(|i| self.station().annotations[i].measurements.clone()).unwrap_or_default();
            egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                egui::Grid::new("meas_table").striped(true).show(ui, |ui| {
                    ui.strong("Label");
                    ui.strong("Species/Genus");
                    ui.strong("Type");
                    ui.strong("W");
                    ui.strong("H");
                    ui.strong("Area/Len");
                    ui.strong("Unit");
                    ui.end_row();
                    for (i, m) in measurements.iter().enumerate() {
                        let unit_str = if m.kind == "polygon" { format!("{}\u{b2}", m.unit) } else { m.unit.clone() };
                        let selected = self.selected_row == Some(i);
                        if ui.selectable_label(selected, &m.label).clicked() {
                            self.selected_row = Some(i);
                        }
                        ui.label(&m.species);
                        ui.label(&m.kind);
                        ui.label(if m.auto_width != 0.0 { format!("{:.2}", m.auto_width) } else { "-".to_string() });
                        ui.label(if m.auto_height != 0.0 { format!("{:.2}", m.auto_height) } else { "-".to_string() });
                        ui.label(format!("{:.3}", m.value));
                        ui.label(unit_str);
                        ui.end_row();
                    }
                });
            });
            if ui.button("Delete Selected").clicked() {
                if let (Some(ann_idx), Some(row)) = (self.current_ann, self.selected_row) {
                    let anns = &mut self.project.stations[0].annotations[ann_idx];
                    if row < anns.measurements.len() {
                        anns.measurements.remove(row);
                        let measurements = anns.measurements.clone();
                        self.redo_stack.clear();
                        self.canvas.set_measurements(measurements);
                    }
                }
            }

            ui.separator();
            ui.group(|ui| {
                ui.label("Selection");
                let unit = self.preview.as_ref().map(|p| p.unit.clone()).unwrap_or_else(|| "cm".to_string());
                let text = |label: &str, v: Option<f64>, suffix: &str| match v {
                    Some(v) => format!("{label}: {v:.3} {unit}{suffix}"),
                    None => format!("{label}: -"),
                };
                ui.label(text("Area", self.preview.as_ref().and_then(|p| p.area), "\u{b2}"));
                ui.label(text("Perimeter", self.preview.as_ref().and_then(|p| p.perimeter), ""));
                ui.label(match &self.preview {
                    Some(p) => format!("Width: {:.2} {}", p.width, p.unit),
                    None => "Width: -".to_string(),
                });
                ui.label(match &self.preview {
                    Some(p) => format!("Height: {:.2} {}", p.height, p.unit),
                    None => "Height: -".to_string(),
                });
                ui.label(text("Length", self.preview.as_ref().and_then(|p| p.length), ""));
            });
        });

        egui::CentralPanel::default().show(ui, |ui| {
            if let Some(idx) = self.current_ann {
                let ann = &mut self.project.stations[0].annotations[idx];
                let mut view = AnnotationView { points: &mut ann.points, scale_factor: ann.scale_factor, scale_unit: &ann.scale_unit };
                let events = self.canvas.show(ui, &mut view);
                self.handle_canvas_events(events);
            } else {
                ui.centered_and_justified(|ui| ui.label("Open an image to begin"));
            }
        });

        if let Some(idx) = self.current_ann {
            if self.calibration_dialog.is_some() {
                let ann = self.station().annotations[idx].clone();
                if let Some(dlg) = &mut self.calibration_dialog {
                    if let Some(result) = dlg.show(&ctx) {
                        match result {
                            Ok(r) => {
                                let scope: Vec<usize> = if r.apply_to_all {
                                    (0..self.station().annotations.len()).collect()
                                } else {
                                    vec![idx]
                                };
                                for i in scope {
                                    self.station_mut().annotations[i].scale_factor = r.scale_factor;
                                    self.station_mut().annotations[i].scale_unit = r.unit.clone();
                                }
                            }
                            Err(()) => {}
                        }
                        self.calibration_dialog = None;
                    }
                }
                let _ = ann;
            }
        }

        if let Some(dlg) = &mut self.label_dialog {
            if let Some(outcome) = dlg.show(&ctx, &mut self.project.species_list) {
                if let (DialogOutcome::Saved(m), Some(idx)) = (outcome, self.current_ann) {
                    let unit_str = if m.kind == "polygon" { format!("{}\u{b2}", m.unit) } else { m.unit.clone() };
                    self.status = format!("Saved: {} - {:.3} {unit_str}", m.label, m.value);
                    self.station_mut().annotations[idx].measurements.push(m);
                    self.redo_stack.clear();
                    let measurements = self.station().annotations[idx].measurements.clone();
                    self.canvas.set_measurements(measurements);
                }
                self.label_dialog = None;
            }
        }

        if let Some(msg) = self.pending_error.clone() {
            egui::Window::new("Error").collapsible(false).show(&ctx, |ui| {
                ui.label(&msg);
                if ui.button("OK").clicked() {
                    self.pending_error = None;
                }
            });
        }
        if let Some((title, msg)) = self.pending_info.clone() {
            egui::Window::new(title).collapsible(false).show(&ctx, |ui| {
                ui.label(&msg);
                if ui.button("OK").clicked() {
                    self.pending_info = None;
                }
            });
        }
    }

    fn open_calibration(&mut self, ctx: &Context) {
        let Some(idx) = self.current_ann else {
            self.pending_info = Some(("No Image".into(), "Select an image first.".into()));
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

    fn save_project(&mut self, path: Option<std::path::PathBuf>) {
        let path = path.or_else(|| self.project.save_path.clone().map(std::path::PathBuf::from));
        let Some(path) = path else {
            if let Some(p) = rfd::FileDialog::new().add_filter("coralX Project", &["cpce"]).save_file() {
                self.save_project(Some(p));
            }
            return;
        };
        match self.project.save(&path) {
            Ok(()) => self.status = format!("Saved: {}", path.display()),
            Err(e) => self.pending_error = Some(format!("Could not save:\n{e}")),
        }
    }

    fn export_excel(&mut self) {
        let Some(path) = rfd::FileDialog::new().add_filter("Excel", &["xlsx"]).save_file() else { return };
        match export_measurements_excel(&self.project, &path) {
            Ok(()) => {
                self.status = format!("Exported: {}", path.display());
                self.pending_info = Some(("Export Done".into(), format!("Saved to:\n{}", path.display())));
            }
            Err(e) => self.pending_error = Some(e.to_string()),
        }
    }
}
