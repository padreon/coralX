//! Dialog for calibrating image scale: the user clicks two points and enters
//! the real-world distance between them.

use egui::{Color32, Context, Pos2, Rect, Vec2};

use crate::models::ImageAnnotation;

const POINT_RADIUS: f32 = 6.0;

/// Zoom/pan state and click handling shared with the eventual `image_canvas`
/// (same transform model as the Python `_CalibCanvas` / `ImageCanvas`).
struct CalibCanvas {
    texture: egui::TextureHandle,
    img_size: Vec2,
    zoom: f32,
    offset: Vec2,
    clicks: Vec<(f32, f32)>,
    fitted: bool,
}

impl CalibCanvas {
    fn image_to_screen(&self, rect: Rect, p: (f32, f32)) -> Pos2 {
        let img_center = self.img_size / 2.0;
        let widget_center = rect.center();
        widget_center + self.offset + (Vec2::new(p.0, p.1) - img_center) * self.zoom
    }

    fn screen_to_image(&self, rect: Rect, p: Pos2) -> (f32, f32) {
        let img_center = self.img_size / 2.0;
        let widget_center = rect.center();
        let rel = (p - widget_center - self.offset) / self.zoom;
        (rel.x + img_center.x, rel.y + img_center.y)
    }

    fn zoom_fit(&mut self, rect: Rect) {
        let wr = rect.width() / self.img_size.x;
        let hr = rect.height() / self.img_size.y;
        self.zoom = wr.min(hr) * 0.95;
        self.offset = Vec2::ZERO;
    }

    fn apply_zoom(&mut self, rect: Rect, new_zoom: f32, anchor: Option<Pos2>) {
        let new_zoom = new_zoom.clamp(0.1, 20.0);
        if let Some(anchor) = anchor {
            let img = self.screen_to_image(rect, anchor);
            let img_center = self.img_size / 2.0;
            self.offset = (anchor - rect.center()) - (Vec2::new(img.0, img.1) - img_center) * new_zoom;
        }
        self.zoom = new_zoom;
    }

    fn reset_points(&mut self) {
        self.clicks.clear();
    }

    fn pixel_distance(&self) -> Option<f32> {
        if self.clicks.len() < 2 {
            return None;
        }
        let (a, b) = (self.clicks[0], self.clicks[1]);
        Some(((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt())
    }

    fn show(&mut self, ui: &mut egui::Ui) {
        let (rect, response) = ui.allocate_exact_size(ui.available_size().max(Vec2::splat(320.0)), egui::Sense::click_and_drag());

        if !self.fitted {
            self.zoom_fit(rect);
            self.fitted = true;
        }

        // Middle-drag to pan.
        if response.dragged_by(egui::PointerButton::Middle) {
            self.offset += response.drag_delta();
        }
        // Left click to place/replace calibration points.
        if response.clicked_by(egui::PointerButton::Primary) {
            if let Some(pos) = response.interact_pointer_pos() {
                if self.clicks.len() >= 2 {
                    self.clicks.clear();
                }
                let (ix, iy) = self.screen_to_image(rect, pos);
                let ix = ix.clamp(0.0, self.img_size.x - 1.0);
                let iy = iy.clamp(0.0, self.img_size.y - 1.0);
                self.clicks.push((ix, iy));
            }
        }
        // Scroll to zoom, anchored at the cursor.
        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                let factor = if scroll > 0.0 { 1.15 } else { 0.87 };
                self.apply_zoom(rect, self.zoom * factor, ui.input(|i| i.pointer.hover_pos()));
            }
            let up = ui.input(|i| i.key_pressed(egui::Key::ArrowUp));
            let down = ui.input(|i| i.key_pressed(egui::Key::ArrowDown));
            if up {
                self.apply_zoom(rect, self.zoom * 1.25, None);
            }
            if down {
                self.apply_zoom(rect, self.zoom / 1.25, None);
            }
        }

        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, Color32::from_rgb(30, 30, 30));

        let img_min = self.image_to_screen(rect, (0.0, 0.0));
        let img_max = self.image_to_screen(rect, (self.img_size.x, self.img_size.y));
        painter.image(self.texture.id(), Rect::from_min_max(img_min, img_max), Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);

        if self.clicks.len() == 2 {
            let a = self.image_to_screen(rect, self.clicks[0]);
            let b = self.image_to_screen(rect, self.clicks[1]);
            painter.line_segment([a, b], egui::Stroke::new(2.0, Color32::from_rgb(255, 220, 0)));
        }
        let r = (POINT_RADIUS * self.zoom).max(2.0);
        for (i, &p) in self.clicks.iter().enumerate() {
            let sp = self.image_to_screen(rect, p);
            painter.circle(sp, r, Color32::from_rgba_unmultiplied(255, 80, 80, 180), egui::Stroke::new(1.5, Color32::from_rgb(255, 80, 80)));
            painter.text(sp + Vec2::new(r + 1.0, 0.0), egui::Align2::LEFT_CENTER, (i + 1).to_string(), egui::FontId::proportional(12.0), Color32::WHITE);
        }
    }
}

pub struct CalibrationDialog {
    canvas: CalibCanvas,
    image_width: i64,
    image_height: i64,
    current_scale_factor: f64,
    current_scale_unit: String,
    real_dist: f64,
    unit_is_cm: bool,
    apply_all: bool,
}

pub struct CalibrationResult {
    pub scale_factor: f64,
    pub unit: String,
    pub apply_to_all: bool,
}

impl CalibrationDialog {
    /// `rgba` must be the decoded image pixels (`image_width * image_height * 4` bytes).
    pub fn new(ctx: &Context, annotation: &ImageAnnotation, rgba: &[u8]) -> Self {
        let (w, h) = (annotation.image_width.max(1) as usize, annotation.image_height.max(1) as usize);
        let color_image = egui::ColorImage::from_rgba_unmultiplied([w, h], rgba);
        let texture = ctx.load_texture("calibration_image", color_image, egui::TextureOptions::LINEAR);

        CalibrationDialog {
            canvas: CalibCanvas {
                texture,
                img_size: Vec2::new(w as f32, h as f32),
                zoom: 1.0,
                offset: Vec2::ZERO,
                clicks: Vec::new(),
                fitted: false,
            },
            image_width: annotation.image_width,
            image_height: annotation.image_height,
            current_scale_factor: annotation.scale_factor,
            current_scale_unit: annotation.scale_unit.clone(),
            real_dist: 50.0,
            unit_is_cm: annotation.scale_unit == "cm",
            apply_all: false,
        }
    }

    fn unit(&self) -> &'static str {
        if self.unit_is_cm { "cm" } else { "m" }
    }

    /// Returns `Some(Ok(result))` on Apply, `Some(Err(()))` on Cancel, `None` while open.
    pub fn show(&mut self, ctx: &Context) -> Option<Result<CalibrationResult, ()>> {
        let mut outcome = None;

        egui::Window::new("Calibrate Image Scale").collapsible(false).default_size([700.0, 780.0]).min_width(500.0).min_height(500.0).show(ctx, |ui| {
            ui.label(
                "Click two points on a known feature (e.g. scale bar or ruler), then enter the real-world distance below.\nScroll or use up/down to zoom. Middle-drag to pan.",
            );
            ui.horizontal(|ui| {
                if ui.button("\u{2191} Zoom In").clicked() {
                    let rect = ui.max_rect();
                    self.canvas.apply_zoom(rect, self.canvas.zoom * 1.25, None);
                }
                if ui.button("\u{2193} Zoom Out").clicked() {
                    let rect = ui.max_rect();
                    self.canvas.apply_zoom(rect, self.canvas.zoom / 1.25, None);
                }
                if ui.button("Fit").clicked() {
                    let rect = ui.max_rect();
                    self.canvas.zoom_fit(rect);
                }
            });

            egui::Frame::new().show(ui, |ui| {
                ui.set_min_height(400.0);
                self.canvas.show(ui);
            });

            ui.separator();
            let px_dist = self.canvas.pixel_distance();
            ui.label(match px_dist {
                Some(d) => format!("Pixel distance: {d:.1} px"),
                None => "Pixel distance: -".to_string(),
            });

            ui.horizontal(|ui| {
                ui.label("Real-world distance:");
                ui.add(egui::DragValue::new(&mut self.real_dist).range(0.01..=100000.0).speed(0.5));
                egui::ComboBox::from_id_salt("calib_unit").selected_text(self.unit()).show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.unit_is_cm, true, "cm");
                    ui.selectable_value(&mut self.unit_is_cm, false, "m");
                });
            });

            let sf = px_dist.filter(|&d| d > 0.0).map(|d| d as f64 / self.real_dist);
            match sf {
                Some(sf) => {
                    ui.label(format!("Scale factor: {sf:.3} px/{}", self.unit()));
                    if self.image_width > 0 && self.image_height > 0 {
                        let (w, h) = (self.image_width as f64 / sf, self.image_height as f64 / sf);
                        ui.label(format!("Photo area: {:.2} {}\u{b2} ({w:.1} x {h:.1} {})", w * h, self.unit(), self.unit()));
                    }
                }
                None => {
                    ui.label("Scale factor: -");
                    ui.label("Photo area: -");
                }
            }
            if self.current_scale_factor > 1.0 {
                ui.label(format!("Current: {:.2} px/{}", self.current_scale_factor, self.current_scale_unit));
            }

            ui.separator();
            if ui.button("Reset points").clicked() {
                self.canvas.reset_points();
            }
            ui.checkbox(&mut self.apply_all, "Apply this scale to all images in this station");

            ui.separator();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let can_apply = sf.is_some();
                if ui.add_enabled(can_apply, egui::Button::new("Apply Calibration")).clicked() {
                    if let Some(sf) = sf {
                        outcome = Some(Ok(CalibrationResult { scale_factor: sf, unit: self.unit().to_string(), apply_to_all: self.apply_all }));
                    }
                }
                if ui.button("Cancel").clicked() {
                    outcome = Some(Err(()));
                }
            });
        });

        outcome
    }
}
