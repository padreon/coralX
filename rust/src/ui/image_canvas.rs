//! Interactive image canvas: point-count point display/labeling, border
//! drawing, and the four measurement drawing modes (line/polyline/polygon/
//! magic wand). The single largest, most interactive piece of the UI port.
//!
//! Ports Qt signals to a `Vec<CanvasEvent>` returned from [`ImageCanvas::show`]
//! each frame, since egui has no signal/slot mechanism — the caller
//! (measurement_window / main_window) drains and reacts to them.

use std::time::Instant;

use egui::{Color32, Context, Pos2, Rect, Vec2};

use crate::core::measurement_tools::{self, Mask};
use crate::models::{Measurement, Point};

const POINT_RADIUS: f32 = 8.0;
const COLOR_UNLABELED: Color32 = Color32::from_rgba_premultiplied(200, 63, 63, 220);
const COLOR_LABELED: Color32 = Color32::from_rgba_premultiplied(63, 173, 63, 220);
const COLOR_SELECTED: Color32 = Color32::from_rgb(255, 220, 0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BorderMode {
    TwoPoint,
    FourPoint,
    Polygon,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeasureMode {
    Line,
    Polyline,
    Polygon,
    Magic,
}

impl MeasureMode {
    /// Status-bar hint shown when this mode is entered (the caller displays
    /// it after calling `ImageCanvas::start_measurement`).
    pub fn hint(self) -> &'static str {
        match self {
            MeasureMode::Line => "Click 2 points to measure length (ESC to cancel)",
            MeasureMode::Polyline => "Click points along coral, double-click to finish (ESC to cancel)",
            MeasureMode::Polygon => "Click to outline coral area, double-click to finish (ESC to cancel)",
            MeasureMode::Magic => "Click on coral to auto-select area (ESC to cancel)",
        }
    }
}

pub struct PreviewInfo {
    pub unit: String,
    pub area: Option<f64>,
    pub perimeter: Option<f64>,
    pub length: Option<f64>,
    pub width: f64,
    pub height: f64,
    pub angle: Option<f64>,
}

pub enum CanvasEvent {
    PointLabeled(i64, String),
    PointSelected(i64),
    BorderDefined(i64, i64, i64, i64),
    BorderPolygonDefined(Vec<(f64, f64)>),
    StatusMessage(String),
    MeasurementDrawn(Measurement),
    PreviewUpdated(Option<PreviewInfo>),
}

struct LabelMenu {
    point_index: i64,
    pos: Pos2,
    /// Skips the dismiss-on-outside-click check for the frame the menu was
    /// opened on — otherwise the click that opens it also reads as "outside".
    just_opened: bool,
}

/// Minimal read access to the annotation the canvas draws/edits, avoiding a
/// hard dependency on the full `ImageAnnotation` type so this module can be
/// unit-tested and reused independently.
pub struct AnnotationView<'a> {
    pub points: &'a mut Vec<Point>,
    pub scale_factor: f64,
    pub scale_unit: &'a str,
}

pub struct ImageCanvas {
    texture: Option<egui::TextureHandle>,
    image_path: String,
    img_size: Vec2,
    zoom: f32,
    offset: Vec2,
    fitted: bool,
    selected_index: Option<i64>,
    coral_codes: Vec<(String, String)>, // (code, description), display order

    border: i64,
    border_rect: Option<(i64, i64, i64, i64)>,
    drawn_polygon: Option<Vec<(f64, f64)>>,
    border_mode: Option<BorderMode>,
    border_clicks: Vec<(f64, f64)>,

    key_buffer: String,
    key_buffer_started: Option<Instant>,

    measure_mode: Option<MeasureMode>,
    measure_clicks: Vec<(f64, f64)>,
    measure_tolerance: i64,
    measure_smoothing: i64,
    measure_ready: bool,
    measurements: Vec<Measurement>,

    magic_preview: Option<Vec<(f64, f64)>>,
    magic_regions: Vec<(Vec<(f64, f64)>, Vec<Vec<(f64, f64)>>)>,
    magic_mask: Option<Mask>,
    magic_history: Vec<(Option<Mask>, Vec<(f64, f64)>)>,
    eraser_active: bool,
    eraser_radius: i64,
    erasing: bool,
    eraser_cursor: Option<(f64, f64)>,

    label_menu: Option<LabelMenu>,
}

impl Default for ImageCanvas {
    fn default() -> Self {
        ImageCanvas {
            texture: None,
            image_path: String::new(),
            img_size: Vec2::ZERO,
            zoom: 1.0,
            offset: Vec2::ZERO,
            fitted: false,
            selected_index: None,
            coral_codes: Vec::new(),
            border: 0,
            border_rect: None,
            drawn_polygon: None,
            border_mode: None,
            border_clicks: Vec::new(),
            key_buffer: String::new(),
            key_buffer_started: None,
            measure_mode: None,
            measure_clicks: Vec::new(),
            measure_tolerance: 32,
            measure_smoothing: 0,
            measure_ready: false,
            measurements: Vec::new(),
            magic_preview: None,
            magic_regions: Vec::new(),
            magic_mask: None,
            magic_history: Vec::new(),
            eraser_active: false,
            eraser_radius: 24,
            erasing: false,
            eraser_cursor: None,
            label_menu: None,
        }
    }
}

impl ImageCanvas {
    pub fn load_image(&mut self, ctx: &Context, image_path: &str, coral_codes: Vec<(String, String)>) -> Result<(), String> {
        let img = image::open(image_path).map_err(|e| e.to_string())?.to_rgba8();
        let (w, h) = (img.width() as usize, img.height() as usize);
        let color_image = egui::ColorImage::from_rgba_unmultiplied([w, h], img.as_raw());
        self.texture = Some(ctx.load_texture(format!("canvas:{image_path}"), color_image, egui::TextureOptions::LINEAR));
        self.img_size = Vec2::new(w as f32, h as f32);
        self.image_path = image_path.to_string();
        self.coral_codes = coral_codes;
        self.zoom = 1.0;
        self.offset = Vec2::ZERO;
        self.fitted = false;
        self.selected_index = None;
        Ok(())
    }

    pub fn set_measurements(&mut self, measurements: Vec<Measurement>) {
        self.measurements = measurements;
    }

    pub fn set_selected_index(&mut self, index: Option<i64>) {
        self.selected_index = index;
    }

    pub fn set_border(&mut self, border: i64) {
        self.border = border;
    }

    pub fn set_border_rect(&mut self, rect: Option<(i64, i64, i64, i64)>) {
        self.border_rect = rect;
        if rect.is_some() {
            self.drawn_polygon = None;
        }
    }

    pub fn set_border_polygon(&mut self, polygon: Option<Vec<(f64, f64)>>) {
        if polygon.is_some() {
            self.border_rect = None;
        }
        self.drawn_polygon = polygon;
    }

    pub fn start_border_drawing(&mut self, mode: BorderMode) {
        self.border_mode = Some(mode);
        self.border_clicks.clear();
    }

    pub fn cancel_border_drawing(&mut self) {
        self.border_mode = None;
        self.border_clicks.clear();
    }

    pub fn start_measurement(&mut self, mode: MeasureMode) {
        self.measure_mode = Some(mode);
        self.measure_clicks.clear();
        self.magic_history.clear();
        self.erasing = false;
        self.eraser_cursor = None;
        self.border_mode = None;
        self.border_clicks.clear();
        self.measure_ready = false;
    }

    pub fn cancel_measurement(&mut self) {
        self.measure_mode = None;
        self.measure_clicks.clear();
        self.magic_preview = None;
        self.magic_regions.clear();
        self.magic_mask = None;
        self.magic_history.clear();
        self.erasing = false;
        self.eraser_cursor = None;
        self.measure_ready = false;
    }

    pub fn set_measure_tolerance(&mut self, v: i64) {
        self.measure_tolerance = v;
    }
    pub fn set_measure_smoothing(&mut self, v: i64) {
        self.measure_smoothing = v;
    }
    pub fn set_eraser_active(&mut self, on: bool) {
        self.eraser_active = on;
        self.erasing = false;
        self.eraser_cursor = None;
    }
    pub fn set_eraser_radius(&mut self, px: i64) {
        self.eraser_radius = px.max(1);
    }

    pub fn zoom_in(&mut self) {
        self.zoom = (self.zoom * 1.25).clamp(0.1, 10.0);
    }
    pub fn zoom_out(&mut self) {
        self.zoom = (self.zoom / 1.25).clamp(0.1, 10.0);
    }
    pub fn zoom_fit(&mut self, rect: Rect) {
        if self.img_size.x > 0.0 {
            let wr = rect.width() / self.img_size.x;
            let hr = rect.height() / self.img_size.y;
            self.zoom = (wr.min(hr) * 0.95).clamp(0.1, 10.0);
            self.offset = Vec2::ZERO;
        }
    }

    /// Undo the last in-progress drawing step. Returns `true` if it did
    /// something (so the caller can fall back to undoing a committed
    /// measurement when nothing is in progress).
    pub fn undo_last(&mut self) -> bool {
        let Some(mode) = self.measure_mode else { return false };

        if mode == MeasureMode::Magic {
            let Some((mask, clicks)) = self.magic_history.pop() else { return false };
            self.measure_clicks = clicks;
            self.magic_mask = mask.clone();
            match mask {
                Some(m) if m.data.iter().any(|&v| v) => {
                    let regions = self.contours_from_mask(&m);
                    self.magic_regions = regions.clone();
                    self.magic_preview = regions.into_iter().max_by_key(|r| r.0.len()).map(|r| r.0);
                }
                _ => {
                    self.magic_mask = None;
                    self.magic_preview = None;
                    self.magic_regions.clear();
                }
            }
            return true;
        }

        if !self.measure_clicks.is_empty() {
            self.measure_clicks.pop();
            let min_pts = if mode == MeasureMode::Polygon { 3 } else { 2 };
            if self.measure_clicks.len() < min_pts {
                self.measure_ready = false;
            }
            return true;
        }
        false
    }

    fn image_to_screen(&self, rect: Rect, p: (f64, f64)) -> Pos2 {
        let img_center = self.img_size / 2.0;
        rect.center() + self.offset + (Vec2::new(p.0 as f32, p.1 as f32) - img_center) * self.zoom
    }

    fn screen_to_image(&self, rect: Rect, p: Pos2) -> (f64, f64) {
        let img_center = self.img_size / 2.0;
        let rel = (p - rect.center() - self.offset) / self.zoom;
        ((rel.x + img_center.x) as f64, (rel.y + img_center.y) as f64)
    }

    fn adaptive_point_r(&self, n_points: usize) -> f32 {
        let mut r = (POINT_RADIUS * self.zoom.sqrt()).clamp(2.5, POINT_RADIUS);
        if n_points > 1 && self.img_size.x > 0.0 {
            let area = self.img_size.x * self.img_size.y;
            let avg_spacing = (area / n_points as f32).sqrt() * self.zoom;
            r = r.min((avg_spacing * 0.35).max(2.5));
        }
        r
    }

    fn hit_point(&self, points: &[Point], img_pos: (f64, f64)) -> Option<i64> {
        let threshold = (self.adaptive_point_r(points.len()) + 2.0) as f64 / self.zoom as f64;
        points.iter().find(|p| (p.x - img_pos.0).abs() <= threshold && (p.y - img_pos.1).abs() <= threshold).map(|p| p.index)
    }

    /// Draws the canvas and handles all mouse/keyboard interaction for one
    /// frame. Returns events the caller should react to (labels applied,
    /// measurements drawn, status text, etc).
    pub fn show(&mut self, ui: &mut egui::Ui, ann: &mut AnnotationView) -> Vec<CanvasEvent> {
        let mut events = Vec::new();
        let (rect, response) = ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());

        if !self.fitted && self.img_size.x > 0.0 {
            self.zoom_fit(rect);
            self.fitted = true;
        }

        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, Color32::from_rgb(30, 30, 30));

        let Some(texture) = &self.texture else {
            painter.text(rect.center(), egui::Align2::CENTER_CENTER, "Open an image to begin", egui::FontId::default(), Color32::from_gray(150));
            return events;
        };

        // ---- pan / zoom ----
        if response.dragged_by(egui::PointerButton::Middle) {
            self.offset += response.drag_delta();
        }
        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                let factor = if scroll > 0.0 { 1.15 } else { 0.87 };
                let new_zoom = (self.zoom * factor).clamp(0.1, 10.0);
                if let Some(cursor) = ui.input(|i| i.pointer.hover_pos()) {
                    let img = self.screen_to_image(rect, cursor);
                    let img_center = self.img_size / 2.0;
                    self.offset = (cursor - rect.center()) - (Vec2::new(img.0 as f32, img.1 as f32) - img_center) * new_zoom;
                }
                self.zoom = new_zoom;
            }
        }

        // ---- image + overlays ----
        let img_min = self.image_to_screen(rect, (0.0, 0.0));
        let img_max = self.image_to_screen(rect, (self.img_size.x as f64, self.img_size.y as f64));
        painter.image(texture.id(), Rect::from_min_max(img_min, img_max), Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);

        self.draw_border(&painter, rect);
        for m in &self.measurements.clone() {
            self.draw_measurement(&painter, rect, m);
        }
        if let Some(preview) = self.magic_preview.clone() {
            self.draw_magic_preview(&painter, rect, &preview, ann);
        }
        if !self.measure_clicks.is_empty() && self.magic_preview.is_none() {
            self.draw_measure_preview(&painter, rect, ann);
        }
        if self.measure_mode == Some(MeasureMode::Magic) && self.eraser_active {
            if let Some(cursor) = self.eraser_cursor {
                let sp = self.image_to_screen(rect, cursor);
                let r = self.eraser_radius as f32 * self.zoom;
                painter.circle(sp, r, Color32::from_rgba_unmultiplied(255, 60, 60, 40), egui::Stroke::new(1.5, Color32::from_rgb(255, 70, 70)));
            }
        }
        self.draw_points(&painter, rect, ann.points);

        // ---- interaction: click handling ----
        self.handle_clicks(ui, &response, rect, ann, &mut events);
        self.handle_keys(ui, ann, &mut events);

        if let Some(menu) = &self.label_menu {
            let idx = menu.point_index;
            let pos = menu.pos;
            let just_opened = menu.just_opened;
            let mut close = false;
            let mut chosen: Option<Option<String>> = None; // Some(None) = clear
            egui::Area::new(egui::Id::new("label_menu")).fixed_pos(pos).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.label(format!("Point #{}", idx + 1));
                    ui.separator();
                    if self.coral_codes.is_empty() {
                        ui.weak("(No coral codes loaded)");
                    }
                    egui::ScrollArea::vertical().max_height(240.0).show(ui, |ui| {
                        for (code, desc) in self.coral_codes.clone() {
                            if ui.button(format!("{code} - {desc}")).clicked() {
                                chosen = Some(Some(code));
                            }
                        }
                    });
                    ui.separator();
                    if ui.button("Clear label").clicked() {
                        chosen = Some(None);
                    }
                });
            });
            // Skip the dismiss check on the frame the menu opened — the click
            // that opened it would otherwise also register as "outside click".
            if !just_opened && chosen.is_none() && ui.input(|i| i.pointer.any_click()) {
                close = true;
            }
            if let Some(menu) = &mut self.label_menu {
                menu.just_opened = false;
            }
            if let Some(value) = chosen {
                if let Some(p) = ann.points.iter_mut().find(|p| p.index == idx) {
                    match value {
                        Some(code) => {
                            p.label = Some(code.clone());
                            p.category = None;
                            events.push(CanvasEvent::PointLabeled(idx, code));
                        }
                        None => {
                            p.label = None;
                            p.category = None;
                        }
                    }
                }
                close = true;
            }
            if close {
                self.label_menu = None;
            }
        }

        events
    }

    fn handle_clicks(&mut self, ui: &mut egui::Ui, response: &egui::Response, rect: Rect, ann: &mut AnnotationView, events: &mut Vec<CanvasEvent>) {
        let hover_pos = response.hover_pos();

        // Double-click: finish polyline/polygon or reset magic-wand selection.
        if response.double_clicked() {
            match self.measure_mode {
                Some(MeasureMode::Magic) => {
                    self.magic_preview = None;
                    self.magic_mask = None;
                    self.magic_regions.clear();
                    self.measure_clicks.clear();
                    self.magic_history.clear();
                    events.push(CanvasEvent::PreviewUpdated(None));
                    events.push(CanvasEvent::StatusMessage("Selection reset - click to start again (ESC to cancel)".into()));
                }
                Some(mode @ (MeasureMode::Polyline | MeasureMode::Polygon)) => {
                    self.measure_clicks.pop();
                    let min_pts = if mode == MeasureMode::Polyline { 2 } else { 3 };
                    if self.measure_clicks.len() >= min_pts {
                        self.measure_ready = true;
                        self.emit_preview_stats(ann, events);
                        let label = if mode == MeasureMode::Polyline { "Polyline" } else { "Polygon" };
                        events.push(CanvasEvent::StatusMessage(format!("{label} preview - Enter to confirm / ESC to cancel")));
                    }
                }
                _ => {}
            }
            return;
        }

        if response.clicked_by(egui::PointerButton::Primary) {
            if let Some(pos) = response.interact_pointer_pos() {
                let img_pos = self.screen_to_image(rect, pos);

                if let Some(mode) = self.measure_mode {
                    match mode {
                        MeasureMode::Magic => {
                            if self.eraser_active {
                                if self.magic_mask.is_some() {
                                    self.push_magic_history();
                                    self.erasing = true;
                                    self.erase_at(img_pos, ann, events);
                                }
                            } else {
                                self.push_magic_history();
                                self.measure_clicks.push(img_pos);
                                self.run_magic_preview(img_pos, ann, events);
                            }
                        }
                        MeasureMode::Line => {
                            if !self.measure_ready {
                                self.measure_clicks.push(img_pos);
                                if self.measure_clicks.len() == 2 {
                                    self.measure_ready = true;
                                    self.emit_preview_stats(ann, events);
                                    events.push(CanvasEvent::StatusMessage("Line preview - Enter to confirm / ESC to cancel".into()));
                                } else {
                                    events.push(CanvasEvent::StatusMessage("Click a second point (ESC to cancel)".into()));
                                }
                            }
                        }
                        MeasureMode::Polyline | MeasureMode::Polygon => {
                            if !self.measure_ready {
                                self.measure_clicks.push(img_pos);
                                let remaining = if mode == MeasureMode::Polyline { "line" } else { "polygon" };
                                events.push(CanvasEvent::StatusMessage(format!("{} point(s) - double-click to finish {remaining} (ESC to cancel)", self.measure_clicks.len())));
                            }
                        }
                    }
                } else if let Some(border_mode) = self.border_mode {
                    self.border_clicks.push(img_pos);
                    match border_mode {
                        BorderMode::Polygon => {
                            let remaining = 4 - self.border_clicks.len() as i64;
                            if remaining > 0 {
                                events.push(CanvasEvent::StatusMessage(format!("Polygon: {remaining} more click(s) remaining (ESC to cancel)")));
                            } else {
                                self.finish_polygon(events);
                            }
                        }
                        _ => {
                            let required = if border_mode == BorderMode::TwoPoint { 2 } else { 4 };
                            let remaining = required - self.border_clicks.len() as i64;
                            if remaining > 0 {
                                events.push(CanvasEvent::StatusMessage(format!("Border: {remaining} more click(s) (ESC to cancel)")));
                            } else {
                                self.finish_border_drawing(events);
                            }
                        }
                    }
                } else if let Some(idx) = self.hit_point(ann.points, img_pos) {
                    self.selected_index = Some(idx);
                    events.push(CanvasEvent::PointSelected(idx));
                    self.label_menu = Some(LabelMenu { point_index: idx, pos, just_opened: true });
                }
            }
        } else if response.clicked_by(egui::PointerButton::Secondary) {
            if let Some(pos) = response.interact_pointer_pos() {
                let img_pos = self.screen_to_image(rect, pos);
                if self.measure_mode == Some(MeasureMode::Magic) {
                    self.push_magic_history();
                    self.run_magic_subtract(img_pos, ann, events);
                } else if let Some(idx) = self.hit_point(ann.points, img_pos) {
                    self.selected_index = Some(idx);
                    events.push(CanvasEvent::PointSelected(idx));
                    self.label_menu = Some(LabelMenu { point_index: idx, pos, just_opened: true });
                }
            }
        }

        // Eraser drag + hover status.
        if self.measure_mode == Some(MeasureMode::Magic) && self.eraser_active {
            if let Some(pos) = hover_pos {
                let img_pos = self.screen_to_image(rect, pos);
                self.eraser_cursor = Some(img_pos);
                if self.erasing && ui.input(|i| i.pointer.button_down(egui::PointerButton::Primary)) {
                    self.erase_at(img_pos, ann, events);
                }
            }
        }
        if response.drag_stopped_by(egui::PointerButton::Primary) && self.erasing {
            self.erasing = false;
            if self.magic_mask.as_ref().is_none_or(|m| !m.data.iter().any(|&v| v)) {
                self.magic_preview = None;
                self.magic_regions.clear();
                events.push(CanvasEvent::PreviewUpdated(None));
            }
            events.push(CanvasEvent::StatusMessage("Erased - drag to remove more / toggle eraser off to add (Ctrl+Z to undo / Enter to confirm)".into()));
        }

        if let Some(pos) = hover_pos {
            let img_pos = self.screen_to_image(rect, pos);
            if self.measure_mode.is_none() && self.border_mode.is_none() {
                match self.hit_point(ann.points, img_pos) {
                    Some(idx) => {
                        let p = ann.points.iter().find(|p| p.index == idx).unwrap();
                        let label_info = p.label.clone().map(|l| format!(" - {l}")).unwrap_or(" - unlabeled".to_string());
                        events.push(CanvasEvent::StatusMessage(format!("Point #{}{label_info}  |  x={}, y={}", idx + 1, img_pos.0 as i64, img_pos.1 as i64)));
                    }
                    None => events.push(CanvasEvent::StatusMessage(format!("x={}, y={}", img_pos.0 as i64, img_pos.1 as i64))),
                }
            }
        }
    }

    fn handle_keys(&mut self, ui: &mut egui::Ui, ann: &mut AnnotationView, events: &mut Vec<CanvasEvent>) {
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            if self.measure_mode.is_some() {
                self.cancel_measurement();
                events.push(CanvasEvent::PreviewUpdated(None));
            } else if self.border_mode.is_some() {
                self.cancel_border_drawing();
            }
            self.key_buffer.clear();
            return;
        }

        if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            if self.measure_mode == Some(MeasureMode::Magic) && self.magic_preview.is_some() {
                self.finish_measurement(ann, events);
                return;
            }
            if self.measure_ready {
                self.finish_measurement(ann, events);
                return;
            }
        }

        if ann.points.is_empty() {
            if ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                self.zoom_in();
            }
            if ui.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                self.zoom_out();
            }
            return;
        }

        let n = ann.points.len() as i64;
        if ui.input(|i| i.key_pressed(egui::Key::ArrowRight)) {
            self.key_buffer.clear();
            let idx = self.selected_index.map(|i| (i + 1).rem_euclid(n)).unwrap_or(0);
            self.selected_index = Some(idx);
            events.push(CanvasEvent::PointSelected(idx));
        } else if ui.input(|i| i.key_pressed(egui::Key::ArrowLeft)) {
            self.key_buffer.clear();
            let idx = self.selected_index.map(|i| (i - 1).rem_euclid(n)).unwrap_or(n - 1);
            self.selected_index = Some(idx);
            events.push(CanvasEvent::PointSelected(idx));
        } else if ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
            self.zoom_in();
        } else if ui.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
            self.zoom_out();
        } else {
            let text = ui.input(|i| i.events.iter().find_map(|e| if let egui::Event::Text(t) = e { Some(t.clone()) } else { None }));
            if let Some(text) = text {
                let text = text.to_uppercase();
                if !self.coral_codes.is_empty() && self.selected_index.is_some() {
                    if self.key_buffer_started.is_none_or(|t| t.elapsed().as_millis() > 700) {
                        self.key_buffer.clear();
                    }
                    self.key_buffer.push_str(&text);
                    self.key_buffer_started = Some(Instant::now());
                    self.try_shortcut_label(ann, events);
                }
            }
        }
    }

    fn try_shortcut_label(&mut self, ann: &mut AnnotationView, events: &mut Vec<CanvasEvent>) {
        let Some(idx) = self.selected_index else {
            self.key_buffer.clear();
            return;
        };
        let matches: Vec<&String> = self.coral_codes.iter().map(|(c, _)| c).filter(|c| c.starts_with(&self.key_buffer)).collect();
        if matches.len() == 1 {
            let code = matches[0].clone();
            if let Some(p) = ann.points.iter_mut().find(|p| p.index == idx) {
                p.label = Some(code.clone());
            }
            self.key_buffer.clear();
            events.push(CanvasEvent::PointLabeled(idx, code));
        } else if matches.is_empty() {
            self.key_buffer.clear();
        }
    }

    fn finish_border_drawing(&mut self, events: &mut Vec<CanvasEvent>) {
        let xs: Vec<f64> = self.border_clicks.iter().map(|p| p.0).collect();
        let ys: Vec<f64> = self.border_clicks.iter().map(|p| p.1).collect();
        let (w, h) = (self.img_size.x as f64, self.img_size.y as f64);
        let x_min = xs.iter().cloned().fold(f64::INFINITY, f64::min).max(0.0) as i64;
        let y_min = ys.iter().cloned().fold(f64::INFINITY, f64::min).max(0.0) as i64;
        let x_max = (xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max)).min(w) as i64;
        let y_max = (ys.iter().cloned().fold(f64::NEG_INFINITY, f64::max)).min(h) as i64;
        self.border_rect = Some((x_min, y_min, x_max, y_max));
        self.drawn_polygon = None;
        self.border_mode = None;
        self.border_clicks.clear();
        events.push(CanvasEvent::BorderDefined(x_min, y_min, x_max, y_max));
    }

    fn finish_polygon(&mut self, events: &mut Vec<CanvasEvent>) {
        let poly = self.border_clicks.clone();
        self.drawn_polygon = Some(poly.clone());
        self.border_rect = None;
        self.border_mode = None;
        self.border_clicks.clear();
        events.push(CanvasEvent::BorderPolygonDefined(poly));
    }

    fn contours_from_mask(&self, mask: &Mask) -> Vec<(Vec<(f64, f64)>, Vec<Vec<(f64, f64)>>)> {
        // The Rust Mask has no OpenCV-style hierarchy/holes support (RETR_CCOMP);
        // approximate with the single outer contour measurement_tools produces.
        // Holes within a magic-wand selection will render solid rather than
        // punched-through until contour hierarchy is ported.
        let region_img = image::GrayImage::from_fn(mask.width as u32, mask.height as u32, |x, y| {
            image::Luma([if mask.data[y as usize * mask.width + x as usize] { 255 } else { 0 }])
        });
        let contours = imageproc::contours::find_contours::<i32>(&region_img);
        contours
            .into_iter()
            .filter(|c| c.border_type == imageproc::contours::BorderType::Outer)
            .filter_map(|c| {
                let perimeter: f64 = c
                    .points
                    .windows(2)
                    .map(|w| (((w[1].x - w[0].x).pow(2) + (w[1].y - w[0].y).pow(2)) as f64).sqrt())
                    .sum();
                let eps = 0.5 + (self.measure_smoothing as f64 / 100.0) * 0.04 * perimeter;
                let line: geo::LineString<f64> = geo::LineString::from(c.points.iter().map(|p| (p.x as f64, p.y as f64)).collect::<Vec<_>>());
                use geo::Simplify;
                let simplified = line.simplify(eps);
                let pts: Vec<(f64, f64)> = simplified.0.iter().map(|c| (c.x, c.y)).collect();
                if pts.len() < 3 {
                    None
                } else {
                    Some((pts, Vec::new()))
                }
            })
            .collect()
    }

    fn push_magic_history(&mut self) {
        self.magic_history.push((self.magic_mask.clone(), self.measure_clicks.clone()));
    }

    fn run_magic_preview(&mut self, seed: (f64, f64), ann: &AnnotationView, events: &mut Vec<CanvasEvent>) {
        events.push(CanvasEvent::StatusMessage("Running magic wand...".into()));
        let (contour, mask) =
            measurement_tools::magic_wand_select(&self.image_path, seed.0 as i64, seed.1 as i64, self.measure_tolerance, self.measure_smoothing, self.magic_mask.as_ref());
        if let (Some(contour), Some(mask)) = (&contour, &mask) {
            if contour.len() >= 3 {
                self.magic_mask = Some(mask.clone());
                let regions = self.contours_from_mask(mask);
                self.magic_regions = if regions.is_empty() { vec![(contour.clone(), Vec::new())] } else { regions.clone() };
                self.magic_preview = Some(regions.into_iter().max_by_key(|r| r.0.len()).map(|r| r.0).unwrap_or_else(|| contour.clone()));
                let scale = if ann.scale_factor > 1.0 { ann.scale_factor } else { 1.0 };
                let area = measurement_tools::polygon_area(contour, scale);
                let unit_str = if ann.scale_factor > 1.0 { format!("{}\u{b2}", ann.scale_unit) } else { "px\u{b2}".to_string() };
                let n = self.measure_clicks.len();
                events.push(CanvasEvent::StatusMessage(format!(
                    "Preview ({n} click{}): {area:.2} {unit_str} - click to expand / right-click to remove area / double-click to reset / Enter to confirm / ESC to cancel",
                    if n != 1 { "s" } else { "" }
                )));
                self.emit_preview_stats(ann, events);
                return;
            }
        }
        self.magic_mask = mask;
        if self.magic_preview.is_none() {
            events.push(CanvasEvent::StatusMessage("No region found - try adjusting tolerance or click elsewhere (ESC to cancel)".into()));
        }
    }

    fn run_magic_subtract(&mut self, seed: (f64, f64), ann: &AnnotationView, events: &mut Vec<CanvasEvent>) {
        let Some(mask) = &self.magic_mask else { return };
        events.push(CanvasEvent::StatusMessage("Subtracting area...".into()));
        let Some(new_mask) = measurement_tools::magic_wand_subtract(&self.image_path, seed.0 as i64, seed.1 as i64, self.measure_tolerance, mask) else { return };
        self.magic_mask = Some(new_mask.clone());
        let regions = self.contours_from_mask(&new_mask);
        if regions.is_empty() {
            self.magic_preview = None;
            self.magic_regions.clear();
            events.push(CanvasEvent::PreviewUpdated(None));
            events.push(CanvasEvent::StatusMessage("Selection empty - click to add / double-click to reset (ESC to cancel)".into()));
            return;
        }
        self.magic_regions = regions.clone();
        self.magic_preview = regions.into_iter().max_by_key(|r| r.0.len()).map(|r| r.0);
        self.emit_preview_stats(ann, events);
        events.push(CanvasEvent::StatusMessage("Area removed - click to expand / right-click to remove more / double-click to reset / Enter to confirm / ESC to cancel".into()));
    }

    fn erase_at(&mut self, pt: (f64, f64), ann: &AnnotationView, events: &mut Vec<CanvasEvent>) {
        let Some(mask) = &mut self.magic_mask else { return };
        let (cx, cy) = (pt.0.round() as i64, pt.1.round() as i64);
        let r = self.eraser_radius.max(1);
        for dy in -r..=r {
            for dx in -r..=r {
                if dx * dx + dy * dy > r * r {
                    continue;
                }
                let (x, y) = (cx + dx, cy + dy);
                if x >= 0 && y >= 0 && (x as usize) < mask.width && (y as usize) < mask.height {
                    mask.data[y as usize * mask.width + x as usize] = false;
                }
            }
        }
        let mask = mask.clone();
        let regions = self.contours_from_mask(&mask);
        self.magic_regions = regions.clone();
        self.magic_preview = regions.into_iter().max_by_key(|r| r.0.len()).map(|r| r.0);
        if self.magic_preview.is_some() {
            self.emit_preview_stats(ann, events);
        } else {
            events.push(CanvasEvent::PreviewUpdated(None));
        }
    }

    fn emit_preview_stats(&self, ann: &AnnotationView, events: &mut Vec<CanvasEvent>) {
        let scale = if ann.scale_factor > 1.0 { ann.scale_factor } else { 1.0 };
        let unit = ann.scale_unit.to_string();

        let pts: &[(f64, f64)] = if self.measure_mode == Some(MeasureMode::Magic) {
            match &self.magic_preview {
                Some(p) => p,
                None => {
                    events.push(CanvasEvent::PreviewUpdated(None));
                    return;
                }
            }
        } else if self.measure_ready && !self.measure_clicks.is_empty() {
            &self.measure_clicks
        } else {
            events.push(CanvasEvent::PreviewUpdated(None));
            return;
        };

        let mut info = PreviewInfo { unit, area: None, perimeter: None, length: None, width: 0.0, height: 0.0, angle: None };

        match self.measure_mode {
            Some(MeasureMode::Polygon) | Some(MeasureMode::Magic) => {
                info.area = Some(if self.measure_mode == Some(MeasureMode::Magic) {
                    self.magic_mask.as_ref().map(|m| measurement_tools::mask_area(m, scale)).unwrap_or(0.0)
                } else {
                    measurement_tools::polygon_area(pts, scale)
                });
                info.perimeter = Some(measurement_tools::contour_perimeter(pts, scale));
                let (w, h, ang) = measurement_tools::oriented_extent(pts, scale);
                info.width = w;
                info.height = h;
                info.angle = Some(ang);
            }
            Some(MeasureMode::Line) => {
                info.length = Some(measurement_tools::line_length(pts[0], pts[1], scale));
                let (w, h) = measurement_tools::bounding_box(pts, scale);
                info.width = w;
                info.height = h;
            }
            Some(MeasureMode::Polyline) => {
                info.length = Some(measurement_tools::polyline_length(pts, scale));
                let (w, h) = measurement_tools::bounding_box(pts, scale);
                info.width = w;
                info.height = h;
            }
            None => {}
        }
        events.push(CanvasEvent::PreviewUpdated(Some(info)));
    }

    fn finish_measurement(&mut self, ann: &AnnotationView, events: &mut Vec<CanvasEvent>) {
        let Some(mut mode) = self.measure_mode else { return };
        let clicks = self.measure_clicks.clone();
        let scale = ann.scale_factor;
        let unit = ann.scale_unit.to_string();

        let (value, points, mut area, mut perim, mut auto_w, mut auto_h, mut angle);
        area = 0.0;
        perim = 0.0;
        auto_w = 0.0;
        auto_h = 0.0;
        angle = 0.0;

        match mode {
            MeasureMode::Line if clicks.len() >= 2 => {
                value = measurement_tools::line_length(clicks[0], clicks[1], scale);
                points = clicks[..2].to_vec();
                (auto_w, auto_h) = measurement_tools::bounding_box(&points, scale);
            }
            MeasureMode::Polyline if clicks.len() >= 2 => {
                value = measurement_tools::polyline_length(&clicks, scale);
                points = clicks.clone();
                (auto_w, auto_h) = measurement_tools::bounding_box(&points, scale);
            }
            MeasureMode::Polygon if clicks.len() >= 3 => {
                area = measurement_tools::polygon_area(&clicks, scale);
                value = area;
                points = clicks.clone();
                perim = measurement_tools::contour_perimeter(&points, scale);
                (auto_w, auto_h, angle) = measurement_tools::oriented_extent(&points, scale);
            }
            MeasureMode::Magic if self.magic_preview.is_some() => {
                let contour = self.magic_preview.clone().unwrap();
                area = self.magic_mask.as_ref().map(|m| measurement_tools::mask_area(m, scale)).unwrap_or_else(|| measurement_tools::polygon_area(&contour, scale));
                value = area;
                points = contour;
                mode = MeasureMode::Polygon;
                perim = measurement_tools::contour_perimeter(&points, scale);
                (auto_w, auto_h, angle) = measurement_tools::oriented_extent(&points, scale);
                self.magic_preview = None;
                self.magic_regions.clear();
                self.magic_mask = None;
            }
            _ => return,
        }

        let kind = match mode {
            MeasureMode::Line => "line",
            MeasureMode::Polyline => "polyline",
            MeasureMode::Polygon => "polygon",
            MeasureMode::Magic => "polygon",
        };
        let m = Measurement::new(kind, "", points, value, unit, "", auto_w, auto_h, area, perim, angle);

        self.measure_mode = None;
        self.measure_clicks.clear();
        self.magic_preview = None;
        self.magic_mask = None;
        self.magic_history.clear();
        self.measure_ready = false;
        events.push(CanvasEvent::PreviewUpdated(None));
        events.push(CanvasEvent::MeasurementDrawn(m));
    }

    // ------------------------------------------------------------ drawing

    fn draw_points(&self, painter: &egui::Painter, rect: Rect, points: &[Point]) {
        let r_screen = self.adaptive_point_r(points.len());
        let show_label = r_screen >= 6.0;
        for p in points {
            let is_selected = Some(p.index) == self.selected_index;
            let color = if is_selected { COLOR_SELECTED } else if p.label.is_some() { COLOR_LABELED } else { COLOR_UNLABELED };
            let sp = self.image_to_screen(rect, (p.x, p.y));
            painter.circle(sp, r_screen, color, egui::Stroke::new(1.5, Color32::from_rgba_unmultiplied(0, 0, 0, 180)));
            if let Some(label) = &p.label {
                if show_label {
                    painter.text(sp + Vec2::new(r_screen + 2.0, 0.0), egui::Align2::LEFT_CENTER, &label[..label.len().min(6)], egui::FontId::proportional(11.0), Color32::BLACK);
                }
            }
        }
    }

    fn draw_border(&self, painter: &egui::Painter, rect: Rect) {
        let stroke = egui::Stroke::new(2.0, Color32::from_rgba_unmultiplied(255, 200, 0, 200));
        if let Some(poly) = &self.drawn_polygon {
            if poly.len() >= 3 {
                let pts: Vec<Pos2> = poly.iter().map(|&p| self.image_to_screen(rect, p)).collect();
                painter.add(egui::Shape::convex_polygon(pts, Color32::from_rgba_unmultiplied(255, 200, 0, 25), stroke));
            }
        } else if let Some((x0, y0, x1, y1)) = self.border_rect {
            let a = self.image_to_screen(rect, (x0 as f64, y0 as f64));
            let b = self.image_to_screen(rect, (x1 as f64, y1 as f64));
            painter.rect_stroke(Rect::from_two_pos(a, b), 0.0, stroke, egui::StrokeKind::Inside);
        } else if self.border > 0 {
            let b = self.border as f64;
            let a1 = self.image_to_screen(rect, (b, b));
            let b1 = self.image_to_screen(rect, (self.img_size.x as f64 - b, self.img_size.y as f64 - b));
            painter.rect_stroke(Rect::from_two_pos(a1, b1), 0.0, stroke, egui::StrokeKind::Inside);
        }

        if !self.border_clicks.is_empty() {
            let r = 6.0f32.max(6.0);
            for &pt in &self.border_clicks {
                let sp = self.image_to_screen(rect, pt);
                painter.circle(sp, r, Color32::from_rgba_unmultiplied(0, 200, 255, 220), egui::Stroke::new(1.0, Color32::from_rgba_unmultiplied(0, 0, 0, 180)));
            }
            if self.border_mode == Some(BorderMode::Polygon) && self.border_clicks.len() >= 2 {
                let pts: Vec<Pos2> = self.border_clicks.iter().map(|&p| self.image_to_screen(rect, p)).collect();
                painter.add(egui::Shape::line(pts, egui::Stroke::new(2.0, Color32::from_rgba_unmultiplied(0, 200, 255, 200))));
            } else if self.border_clicks.len() >= 2 {
                let xs: Vec<f64> = self.border_clicks.iter().map(|p| p.0).collect();
                let ys: Vec<f64> = self.border_clicks.iter().map(|p| p.1).collect();
                let a = self.image_to_screen(rect, (xs.iter().cloned().fold(f64::INFINITY, f64::min), ys.iter().cloned().fold(f64::INFINITY, f64::min)));
                let b = self.image_to_screen(rect, (xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max), ys.iter().cloned().fold(f64::NEG_INFINITY, f64::max)));
                painter.rect(Rect::from_two_pos(a, b), 0.0, Color32::from_rgba_unmultiplied(0, 200, 255, 30), egui::Stroke::new(2.0, Color32::from_rgba_unmultiplied(0, 200, 255, 200)), egui::StrokeKind::Inside);
            }
        }
    }

    fn draw_measurement(&self, painter: &egui::Painter, rect: Rect, m: &Measurement) {
        if m.points.is_empty() {
            return;
        }
        let pts: Vec<Pos2> = m.points.iter().map(|&p| self.image_to_screen(rect, p)).collect();
        let stroke = egui::Stroke::new(2.0, Color32::from_rgba_unmultiplied(0, 220, 255, 230));

        match m.kind.as_str() {
            "line" if pts.len() >= 2 => {
                painter.line_segment([pts[0], pts[1]], stroke);
                let mid = ((pts[0].to_vec2() + pts[1].to_vec2()) / 2.0).to_pos2();
                painter.text(mid, egui::Align2::LEFT_CENTER, format!("{}: {:.2} {}", m.label, m.value, m.unit), egui::FontId::proportional(12.0), Color32::WHITE);
            }
            "polyline" if pts.len() >= 2 => {
                painter.add(egui::Shape::line(pts.clone(), stroke));
                let mid = pts.iter().fold(Vec2::ZERO, |acc, p| acc + p.to_vec2()) / pts.len() as f32;
                painter.text(mid.to_pos2(), egui::Align2::LEFT_CENTER, format!("{}: {:.2} {}", m.label, m.value, m.unit), egui::FontId::proportional(12.0), Color32::WHITE);
            }
            "polygon" if pts.len() >= 3 => {
                painter.add(egui::Shape::convex_polygon(pts.clone(), Color32::from_rgba_unmultiplied(0, 220, 255, 35), stroke));
                let mid = pts.iter().fold(Vec2::ZERO, |acc, p| acc + p.to_vec2()) / pts.len() as f32;
                painter.text(mid.to_pos2(), egui::Align2::LEFT_CENTER, format!("{}: {:.2} {}\u{b2}", m.label, m.value, m.unit), egui::FontId::proportional(12.0), Color32::WHITE);
            }
            _ => {}
        }
    }

    fn draw_measure_preview(&self, painter: &egui::Painter, rect: Rect, ann: &AnnotationView) {
        let pts: Vec<Pos2> = self.measure_clicks.iter().map(|&p| self.image_to_screen(rect, p)).collect();
        for &p in &pts {
            painter.circle(p, 5.0, Color32::from_rgba_unmultiplied(0, 220, 255, 200), egui::Stroke::new(1.0, Color32::from_rgba_unmultiplied(0, 0, 0, 160)));
        }
        if pts.len() >= 2 {
            painter.add(egui::Shape::line(pts, egui::Stroke::new(2.0, Color32::from_rgba_unmultiplied(0, 220, 255, 200))));
        }

        if self.measure_ready && self.measure_clicks.len() >= 2 {
            let scale = if ann.scale_factor > 1.0 { ann.scale_factor } else { 1.0 };
            if self.measure_mode == Some(MeasureMode::Polygon) {
                self.draw_oriented_bbox(painter, rect, &self.measure_clicks, scale, ann.scale_unit);
            }
        }
    }

    fn draw_magic_preview(&self, painter: &egui::Painter, rect: Rect, preview: &[(f64, f64)], ann: &AnnotationView) {
        for (outer, holes) in &self.magic_regions {
            if outer.len() < 3 {
                continue;
            }
            let pts: Vec<Pos2> = outer.iter().map(|&p| self.image_to_screen(rect, p)).collect();
            painter.add(egui::Shape::convex_polygon(pts, Color32::from_rgba_unmultiplied(255, 180, 0, 70), egui::Stroke::new(2.5, Color32::from_rgba_unmultiplied(255, 200, 0, 240))));
            for hole in holes {
                if hole.len() >= 3 {
                    let hpts: Vec<Pos2> = hole.iter().map(|&p| self.image_to_screen(rect, p)).collect();
                    painter.add(egui::Shape::convex_polygon(hpts, Color32::from_rgb(30, 30, 30), egui::Stroke::new(1.5, Color32::from_rgba_unmultiplied(255, 200, 0, 200))));
                }
            }
        }
        let scale = if ann.scale_factor > 1.0 { ann.scale_factor } else { 1.0 };
        self.draw_oriented_bbox(painter, rect, preview, scale, ann.scale_unit);
    }

    fn draw_oriented_bbox(&self, painter: &egui::Painter, rect: Rect, pts: &[(f64, f64)], scale: f64, unit: &str) {
        let Some(axes) = measurement_tools::oriented_axes(pts) else { return };
        let (w_real, h_real, angle) = measurement_tools::oriented_extent(pts, scale);
        let unit_str = if scale > 1.0 { unit } else { "px" };

        let corners: Vec<Pos2> = axes.corners.iter().map(|&p| self.image_to_screen(rect, p)).collect();
        painter.add(egui::Shape::closed_line(corners, egui::Stroke::new(1.5, Color32::from_rgba_unmultiplied(255, 255, 255, 90))));

        let (ha, hb) = axes.height_line;
        let (wa, wb) = axes.width_line;
        let angle_txt = if angle.abs() > 1e-6 { format!("  ({angle:+.0}\u{b0})") } else { String::new() };
        self.draw_axis(painter, rect, ha, hb, Color32::from_rgb(100, 255, 140), &format!("H: {h_real:.1} {unit_str}{angle_txt}"));
        self.draw_axis(painter, rect, wa, wb, Color32::from_rgb(255, 220, 0), &format!("W: {w_real:.1} {unit_str}"));
    }

    fn draw_axis(&self, painter: &egui::Painter, rect: Rect, a: (f64, f64), b: (f64, f64), color: Color32, label: &str) {
        let p1 = self.image_to_screen(rect, a);
        let p2 = self.image_to_screen(rect, b);
        painter.line_segment([p1, p2], egui::Stroke::new(1.5, color));
        let mid = ((p1.to_vec2() + p2.to_vec2()) / 2.0).to_pos2();
        painter.text(mid + Vec2::new(6.0, -6.0), egui::Align2::LEFT_BOTTOM, label, egui::FontId::proportional(12.0), color);
    }
}
