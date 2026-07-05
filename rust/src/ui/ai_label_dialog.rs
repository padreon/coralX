//! AI auto-label configuration dialog and run-progress dialog.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::Arc;

use egui::{Color32, Context};

use crate::core::ai_labeler::{self, AiLabeler, LabelResult, WorkerEvent};
use crate::models::ImageAnnotation;
use crate::ui::progress_dialog::{spawn_worker, WorkerEvent as SimpleEvent};
use crate::ui::settings::AiSettings;

#[derive(PartialEq, Clone, Copy)]
pub enum Scope {
    Image,
    Station,
    Project,
}

impl Scope {
    fn as_str(self) -> &'static str {
        match self {
            Scope::Image => "image",
            Scope::Station => "station",
            Scope::Project => "project",
        }
    }
    fn from_str(s: &str) -> Self {
        match s {
            "station" => Scope::Station,
            "project" => Scope::Project,
            _ => Scope::Image,
        }
    }
}

enum ModelLoad {
    None,
    Loading(Receiver<SimpleEvent<(AiLabeler, HashMap<String, Option<String>>)>>),
    Loaded,
}

pub struct AiLabelDialog {
    model_path: String,
    conf_threshold: f64,
    crop_size: i64,
    scope: Scope,
    /// Checkbox state: "Label only unlabeled points" — the inverse of the
    /// `overwrite_labeled` flag the labeling job actually wants.
    only_unlabeled: bool,
    coral_codes: HashMap<String, String>,
    class_mapping: Vec<(String, Option<String>)>,
    labeler: Option<AiLabeler>,
    load: ModelLoad,
    load_error: Option<String>,
    info_message: Option<String>,
}

pub struct RunConfig {
    pub labeler: AiLabeler,
    pub class_mapping: HashMap<String, Option<String>>,
    pub conf_threshold: f32,
    pub crop_size: u32,
    pub overwrite_labeled: bool,
    pub scope: &'static str,
}

impl AiLabelDialog {
    pub fn new(coral_codes: HashMap<String, String>) -> Self {
        let s = AiSettings::load();
        AiLabelDialog {
            model_path: s.model_path,
            conf_threshold: s.conf_threshold,
            crop_size: s.crop_size,
            scope: Scope::from_str(&s.scope),
            only_unlabeled: s.only_unlabeled,
            coral_codes,
            class_mapping: Vec::new(),
            labeler: None,
            load: ModelLoad::None,
            load_error: None,
            info_message: None,
        }
    }

    fn save_settings(&self) {
        AiSettings {
            model_path: self.model_path.clone(),
            conf_threshold: self.conf_threshold,
            crop_size: self.crop_size,
            scope: self.scope.as_str().to_string(),
            only_unlabeled: self.only_unlabeled,
        }
        .save();
    }

    fn start_load(&mut self) {
        let path = self.model_path.trim().to_string();
        if path.is_empty() {
            self.load_error = Some("Please select a .pt/.onnx model file first.".to_string());
            return;
        }
        let coral_codes = self.coral_codes.clone();
        let rx = spawn_worker(move |_cb| {
            let labeler = AiLabeler::load(&path)?;
            let suggestions = AiLabeler::suggest_mapping(labeler.class_names(), &coral_codes);
            Ok((labeler, suggestions))
        });
        self.load = ModelLoad::Loading(rx);
    }

    /// Draws the dialog; returns `Some(config)` once the user clicks Run with
    /// a model loaded.
    pub fn show(&mut self, ctx: &Context) -> Option<RunConfig> {
        // Poll the background model-load job, if any.
        if let ModelLoad::Loading(rx) = &self.load {
            match rx.try_recv() {
                Ok(SimpleEvent::Succeeded((labeler, suggestions))) => {
                    let mut options: Vec<String> = self.coral_codes.keys().cloned().collect();
                    options.sort();
                    self.class_mapping = suggestions
                        .into_iter()
                        .map(|(cls, suggested)| (cls, suggested.filter(|s| options.contains(s))))
                        .collect();
                    self.info_message = Some(format!(
                        "Model loaded successfully.\nType: {}\nClasses ({}): {}",
                        if labeler.task() == "detect" { "detection" } else { "classification" },
                        labeler.class_names().len(),
                        labeler.class_names().join(", ")
                    ));
                    self.labeler = Some(labeler);
                    self.load = ModelLoad::Loaded;
                }
                Ok(SimpleEvent::Failed(msg)) => {
                    self.load_error = Some(msg);
                    self.load = ModelLoad::None;
                }
                Ok(SimpleEvent::Progress { .. }) | Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.load = ModelLoad::None,
            }
        }

        let mut run_config = None;

        egui::Window::new("AI Auto-Label").collapsible(false).min_width(500.0).show(ctx, |ui| {
            ui.group(|ui| {
                ui.label("Model");
                ui.horizontal(|ui| {
                    ui.label("Model file (.pt/.onnx):");
                    ui.text_edit_singleline(&mut self.model_path);
                    if ui.button("Browse...").clicked() {
                        if let Some(path) = rfd::FileDialog::new().add_filter("YOLO model", &["pt", "onnx"]).pick_file() {
                            self.model_path = path.to_string_lossy().into_owned();
                            self.start_load();
                        }
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Confidence threshold:");
                    ui.add(egui::Slider::new(&mut self.conf_threshold, 0.0..=1.0).step_by(0.05));
                });
                ui.horizontal(|ui| {
                    ui.label("Crop size:");
                    ui.add(egui::Slider::new(&mut self.crop_size, 32..=512).step_by(32.0).suffix(" px"));
                });
            });

            ui.group(|ui| {
                ui.label("Scope");
                ui.radio_value(&mut self.scope, Scope::Image, "This image only");
                ui.radio_value(&mut self.scope, Scope::Station, "This station");
                ui.radio_value(&mut self.scope, Scope::Project, "Entire project");
            });

            ui.checkbox(&mut self.only_unlabeled, "Label only unlabeled points (uncheck to overwrite all)");

            ui.group(|ui| {
                ui.label("Class Mapping");
                if matches!(self.load, ModelLoad::Loading(_)) {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Loading model...");
                    });
                } else if self.class_mapping.is_empty() {
                    ui.colored_label(Color32::GRAY, "Select a model file above to populate this table.");
                } else {
                    let mut options: Vec<String> = self.coral_codes.keys().cloned().collect();
                    options.sort();
                    egui::Grid::new("class_mapping_grid").striped(true).show(ui, |ui| {
                        ui.label("Model Class");
                        ui.label("Coral Code");
                        ui.end_row();
                        for (cls, selected) in &mut self.class_mapping {
                            ui.label(cls.as_str());
                            let text = selected.clone().unwrap_or_else(|| "(skip)".to_string());
                            egui::ComboBox::from_id_salt(format!("map_{cls}")).selected_text(text).show_ui(ui, |ui| {
                                if ui.selectable_label(selected.is_none(), "(skip)").clicked() {
                                    *selected = None;
                                }
                                for code in &options {
                                    if ui.selectable_label(selected.as_deref() == Some(code.as_str()), code).clicked() {
                                        *selected = Some(code.clone());
                                    }
                                }
                            });
                            ui.end_row();
                        }
                    });
                }
                if ui.button("Load Model").clicked() {
                    self.start_load();
                }
            });

            if let Some(err) = &self.load_error {
                ui.colored_label(Color32::from_rgb(0xd6, 0x27, 0x28), err);
            }
            if let Some(info) = self.info_message.clone() {
                ui.colored_label(Color32::from_rgb(0x6a, 0xcc, 0x65), &info);
            }

            ui.separator();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let can_run = self.labeler.is_some();
                if ui.add_enabled(can_run, egui::Button::new("Run")).clicked() {
                    self.save_settings();
                    if let Some(labeler) = self.labeler.take() {
                        let class_mapping: HashMap<String, Option<String>> = self.class_mapping.iter().cloned().collect();
                        run_config = Some(RunConfig {
                            labeler,
                            class_mapping,
                            conf_threshold: self.conf_threshold as f32,
                            crop_size: self.crop_size as u32,
                            overwrite_labeled: !self.only_unlabeled,
                            scope: self.scope.as_str(),
                        });
                    }
                }
                if ui.button("Cancel").clicked() {
                    self.save_settings();
                }
            });
        });

        run_config
    }
}

/// Live progress + scrolling log while an AI labeling job runs in the background.
pub struct AiProgressDialog {
    rx: Receiver<WorkerEvent>,
    cancel: Arc<AtomicBool>,
    done: usize,
    total: usize,
    status: String,
    log: Vec<String>,
    finished: bool,
    result: Vec<LabelResult>,
}

impl AiProgressDialog {
    pub fn spawn(
        labeler: AiLabeler,
        annotations: Vec<(String, ImageAnnotation)>,
        class_mapping: HashMap<String, Option<String>>,
        conf_threshold: f32,
        crop_size: u32,
        overwrite_labeled: bool,
    ) -> Self {
        let total: usize = annotations
            .iter()
            .map(|(_, a)| a.points.iter().filter(|p| overwrite_labeled || p.label.is_none()).count())
            .sum();
        let (rx, cancel) = ai_labeler::spawn_label_worker(labeler, annotations, class_mapping, conf_threshold, crop_size, overwrite_labeled);
        AiProgressDialog { rx, cancel, done: 0, total, status: "Starting...".to_string(), log: Vec::new(), finished: false, result: Vec::new() }
    }

    pub fn is_finished(&self) -> bool {
        self.finished
    }

    pub fn take_result(self) -> Vec<LabelResult> {
        self.result
    }

    pub fn show(&mut self, ctx: &Context) {
        loop {
            match self.rx.try_recv() {
                Ok(WorkerEvent::Progress { done, total, message }) => {
                    self.done = done;
                    self.total = total;
                    self.status = message.clone();
                    self.log.push(message);
                }
                Ok(WorkerEvent::Error(msg)) => {
                    self.log.push(format!("ERROR: {msg}"));
                }
                Ok(WorkerEvent::Finished(results)) => {
                    self.result = results;
                    self.finished = true;
                    self.status = "Done.".to_string();
                    break;
                }
                Err(_) => break,
            }
        }

        egui::Window::new("AI Auto-Label — Running").collapsible(false).min_width(480.0).min_height(300.0).show(ctx, |ui| {
            ui.label("Running AI inference...");
            ui.label(&self.status);
            ui.add(egui::ProgressBar::new(self.done as f32 / self.total.max(1) as f32).show_percentage());

            egui::ScrollArea::vertical().max_height(150.0).stick_to_bottom(true).show(ui, |ui| {
                for line in &self.log {
                    ui.monospace(line);
                }
            });

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if self.finished {
                    if ui.button("Close").clicked() {
                        // caller checks is_finished() + closes
                    }
                } else {
                    let cancelling = self.cancel.load(Ordering::Relaxed);
                    if ui.add_enabled(!cancelling, egui::Button::new(if cancelling { "Cancelling..." } else { "Cancel" })).clicked() {
                        self.cancel.store(true, Ordering::Relaxed);
                        self.status = "Cancelling...".to_string();
                    }
                }
            });
        });
        ctx.request_repaint();
    }
}
