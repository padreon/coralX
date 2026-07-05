//! AI auto-labeling via a YOLOv8 classification or detection model, exported
//! to ONNX and run through the `ort` crate (chosen over `tch`/libtorch for a
//! much smaller runtime footprint — see project history).
//!
//! Convert the existing `.pt` checkpoint once with
//! `tools/export_ai_model_onnx.py`, which also verifies it exports at the
//! model's actual training resolution (baked into `model.model.transforms`,
//! not necessarily a "round" number like 224 — the shipped `data-training.pt`
//! is trained at 64px, for instance).
//!
//! ## Verified pipeline (classify)
//! Pipeline: crop -> resize shortest edge to the model's native size
//! (bilinear) -> center-crop to a square -> `[0,1]` float NCHW, RGB, no
//! mean/std normalisation -> the ONNX graph's output is already softmax'd.
//! Cross-checked against the original PyTorch model on a real photo (see
//! `examples/verify_ai_labeler.rs`): identical top-1 class in every case,
//! and confidence matches almost exactly — a full-image decode without
//! `crop_around` matches PyTorch's own confidence bit-for-bit; the point-crop
//! path is within ~2%, traced by direct pixel comparison to Rust's and
//! OpenCV's JPEG decoders disagreeing by +-2/255 on a couple of pixels, not
//! a logic error.
//!
//! ## Detect path
//! Implemented against the documented Ultralytics export contract (letterbox
//! resize, `[1, 4+nc, num_anchors]` output, greedy NMS) but **not validated**
//! against a real detect-task model — none exists in this repo. Test before
//! relying on it.

use std::collections::HashMap;
use std::path::Path;
use std::sync::mpsc::{Receiver, Sender};

use anyhow::{Context, Result};
use image::{imageops::FilterType, Rgb, RgbImage};
use ort::session::Session;
use ort::value::TensorRef;

use crate::models::ImageAnnotation;

pub struct LabelResult {
    pub annotation_path: String,
    pub point_index: i64,
    pub predicted_class: String,
    pub mapped_code: Option<String>,
    pub confidence: f32,
}

pub struct AiLabeler {
    session: Session,
    class_names: Vec<String>,
    task: String,
    input_size: u32,
}

impl AiLabeler {
    /// Load a YOLOv8 classify/detect model exported to ONNX by
    /// `tools/export_ai_model_onnx.py`. Reads class names, task, and input
    /// size from the metadata Ultralytics embeds in the ONNX file itself.
    pub fn load(model_path: impl AsRef<Path>) -> Result<Self> {
        let session = Session::builder()?.commit_from_file(model_path.as_ref())?;
        let (task, class_names, input_size) = {
            let metadata = session.metadata()?;
            let task = metadata.custom("task").unwrap_or_else(|| "classify".to_string());
            let class_names = parse_names_dict(&metadata.custom("names").unwrap_or_default());
            let input_size = parse_first_int(&metadata.custom("imgsz").unwrap_or_default()).unwrap_or(224);
            (task, class_names, input_size)
        };

        Ok(AiLabeler { session, class_names, task, input_size })
    }

    pub fn task(&self) -> &str {
        &self.task
    }

    pub fn class_names(&self) -> &[String] {
        &self.class_names
    }

    /// Return `(class_name, confidence)` for the image region around `(x, y)`.
    pub fn predict_point(&mut self, image: &RgbImage, x: f64, y: f64, crop_size: u32) -> Result<(String, f32)> {
        if self.task == "classify" {
            self.predict_classify(image, x, y, crop_size)
        } else {
            self.predict_detect(image, x, y, crop_size)
        }
    }

    fn predict_classify(&mut self, image: &RgbImage, x: f64, y: f64, crop_size: u32) -> Result<(String, f32)> {
        let crop = crop_around(image, x, y, crop_size);
        let resized = resize_shortest_edge_then_center_crop(&crop, self.input_size);
        let input = rgb_to_nchw(&resized);

        let outputs = self.session.run(ort::inputs![TensorRef::from_array_view(&input)?])?;
        let (_, data) = outputs[0].try_extract_tensor::<f32>()?;

        let (best_idx, &best_score) =
            data.iter().enumerate().max_by(|a, b| a.1.partial_cmp(b.1).unwrap()).context("empty model output")?;
        let name = self.class_names.get(best_idx).cloned().unwrap_or_else(|| best_idx.to_string());
        Ok((name, best_score))
    }

    fn predict_detect(&mut self, image: &RgbImage, x: f64, y: f64, crop_size: u32) -> Result<(String, f32)> {
        let detect_size = (crop_size * 3).max(224);
        let crop = crop_around(image, x, y, detect_size);
        let (letterboxed, scale, pad_x, pad_y) = letterbox(&crop, self.input_size);
        let input = rgb_to_nchw(&letterboxed);

        let outputs = self.session.run(ort::inputs![TensorRef::from_array_view(&input)?])?;
        let (shape, data) = outputs[0].try_extract_tensor::<f32>()?;
        // Expected shape [1, 4+nc, num_anchors] (Ultralytics' transposed detect export).
        let channels = shape[1] as usize;
        let num_anchors = shape[2] as usize;
        let nc = channels - 4;

        let get = |c: usize, a: usize| data[c * num_anchors + a];

        const CONF_THRESH: f32 = 0.25;
        const IOU_THRESH: f32 = 0.45;
        let mut candidates: Vec<(f32, f32, f32, f32, usize, f32)> = Vec::new(); // cx,cy,w,h,class,score
        for a in 0..num_anchors {
            let (mut best_c, mut best_s) = (0usize, f32::MIN);
            for c in 0..nc {
                let s = get(4 + c, a);
                if s > best_s {
                    best_s = s;
                    best_c = c;
                }
            }
            if best_s >= CONF_THRESH {
                candidates.push((get(0, a), get(1, a), get(2, a), get(3, a), best_c, best_s));
            }
        }

        let kept = greedy_nms(candidates, IOU_THRESH);
        if kept.is_empty() {
            return Ok(("(no detection)".to_string(), 0.0));
        }

        // Undo letterbox to get boxes back in crop-space, then pick the one
        // whose center is closest to the crop's own center.
        let center = (detect_size as f32 / 2.0, detect_size as f32 / 2.0);
        let mut best: Option<(f32, usize, f32)> = None; // dist, class, score
        for (cx, cy, _w, _h, class, score) in &kept {
            let px = (cx - pad_x) / scale;
            let py = (cy - pad_y) / scale;
            let dist = ((px - center.0).powi(2) + (py - center.1).powi(2)).sqrt();
            if best.is_none() || dist < best.unwrap().0 {
                best = Some((dist, *class, *score));
            }
        }
        let (_, class, score) = best.unwrap();
        let _ = (x, y); // seed point only informs the crop; detection is over the whole crop
        let name = self.class_names.get(class).cloned().unwrap_or_else(|| class.to_string());
        Ok((name, score))
    }

    /// Best-guess `{class_name: coral_code}` mapping based on name matching.
    pub fn suggest_mapping(class_names: &[String], coral_codes: &HashMap<String, String>) -> HashMap<String, Option<String>> {
        let mut mapping = HashMap::new();
        for cls in class_names {
            let cls_lower = cls.to_lowercase().replace('_', " ");
            let matched = coral_codes
                .iter()
                .find(|(code, desc)| cls_lower == code.to_lowercase() || desc.to_lowercase().contains(&cls_lower))
                .map(|(code, _)| code.clone());
            mapping.insert(cls.clone(), matched);
        }
        mapping
    }
}

fn crop_around(image: &RgbImage, x: f64, y: f64, crop_size: u32) -> RgbImage {
    let half = (crop_size / 2) as i64;
    let (w, h) = (image.width() as i64, image.height() as i64);
    let cx = (x as i64).clamp(0, w - 1);
    let cy = (y as i64).clamp(0, h - 1);

    let mut out = RgbImage::from_pixel(crop_size, crop_size, Rgb([0, 0, 0]));
    let (x0, x1) = ((cx - half).max(0), (cx + half).min(w));
    let (y0, y1) = ((cy - half).max(0), (cy + half).min(h));
    let (dst_x0, dst_y0) = ((x0 - (cx - half)) as u32, (y0 - (cy - half)) as u32);

    for sy in y0..y1 {
        for sx in x0..x1 {
            let px = *image.get_pixel(sx as u32, sy as u32);
            let (dx, dy) = (dst_x0 + (sx - x0) as u32, dst_y0 + (sy - y0) as u32);
            if dx < crop_size && dy < crop_size {
                out.put_pixel(dx, dy, px);
            }
        }
    }
    out
}

/// Matches `torchvision.transforms.Resize(size)` (shortest edge, bilinear)
/// followed by `CenterCrop(size)`.
fn resize_shortest_edge_then_center_crop(img: &RgbImage, size: u32) -> RgbImage {
    let (w, h) = (img.width(), img.height());
    let (new_w, new_h) = if w < h {
        (size, (size as f64 * h as f64 / w as f64).round() as u32)
    } else {
        ((size as f64 * w as f64 / h as f64).round() as u32, size)
    };
    let resized = image::imageops::resize(img, new_w.max(1), new_h.max(1), FilterType::Triangle);
    let left = (new_w.saturating_sub(size)) / 2;
    let top = (new_h.saturating_sub(size)) / 2;
    image::imageops::crop_imm(&resized, left, top, size, size).to_image()
}

/// Standard YOLO letterbox: resize preserving aspect ratio to fit within
/// `size x size`, pad the remainder with grey (114). Returns
/// `(image, scale, pad_x, pad_y)` so boxes can be mapped back.
fn letterbox(img: &RgbImage, size: u32) -> (RgbImage, f32, f32, f32) {
    let (w, h) = (img.width() as f32, img.height() as f32);
    let scale = (size as f32 / w).min(size as f32 / h);
    let (new_w, new_h) = ((w * scale).round() as u32, (h * scale).round() as u32);
    let resized = image::imageops::resize(img, new_w.max(1), new_h.max(1), FilterType::Triangle);

    let mut out = RgbImage::from_pixel(size, size, Rgb([114, 114, 114]));
    let pad_x = ((size - new_w) / 2) as i64;
    let pad_y = ((size - new_h) / 2) as i64;
    image::imageops::overlay(&mut out, &resized, pad_x, pad_y);
    (out, scale, pad_x as f32, pad_y as f32)
}

fn rgb_to_nchw(img: &RgbImage) -> ndarray::Array4<f32> {
    let (w, h) = (img.width() as usize, img.height() as usize);
    ndarray::Array4::from_shape_fn((1, 3, h, w), |(_, c, y, x)| img.get_pixel(x as u32, y as u32).0[c] as f32 / 255.0)
}

fn greedy_nms(mut boxes: Vec<(f32, f32, f32, f32, usize, f32)>, iou_thresh: f32) -> Vec<(f32, f32, f32, f32, usize, f32)> {
    boxes.sort_by(|a, b| b.5.partial_cmp(&a.5).unwrap());
    let mut kept: Vec<(f32, f32, f32, f32, usize, f32)> = Vec::new();
    'outer: for b in boxes {
        for k in &kept {
            if k.4 == b.4 && iou(b, *k) > iou_thresh {
                continue 'outer;
            }
        }
        kept.push(b);
    }
    kept
}

fn iou(a: (f32, f32, f32, f32, usize, f32), b: (f32, f32, f32, f32, usize, f32)) -> f32 {
    let (ax0, ay0, ax1, ay1) = (a.0 - a.2 / 2.0, a.1 - a.3 / 2.0, a.0 + a.2 / 2.0, a.1 + a.3 / 2.0);
    let (bx0, by0, bx1, by1) = (b.0 - b.2 / 2.0, b.1 - b.3 / 2.0, b.0 + b.2 / 2.0, b.1 + b.3 / 2.0);
    let ix0 = ax0.max(bx0);
    let iy0 = ay0.max(by0);
    let ix1 = ax1.min(bx1);
    let iy1 = ay1.min(by1);
    let inter = (ix1 - ix0).max(0.0) * (iy1 - iy0).max(0.0);
    let area_a = (ax1 - ax0) * (ay1 - ay0);
    let area_b = (bx1 - bx0) * (by1 - by0);
    let union = area_a + area_b - inter;
    if union > 0.0 {
        inter / union
    } else {
        0.0
    }
}

/// Parse Ultralytics' embedded `names` metadata, a Python dict literal like
/// `{0: 'branching', 1: 'encrusting', ...}`. Keys are contiguous from 0 in
/// class-index order, so this just returns the quoted values in order.
fn parse_names_dict(raw: &str) -> Vec<String> {
    raw.split('\'').skip(1).step_by(2).map(String::from).collect()
}

fn parse_first_int(raw: &str) -> Option<u32> {
    raw.chars().skip_while(|c| !c.is_ascii_digit()).take_while(|c| c.is_ascii_digit()).collect::<String>().parse().ok()
}

/// Messages emitted by [`run_label_worker`] — the `std::thread` + channel
/// equivalent of the Python version's `QThread` + `pyqtSignal`s. A future
/// egui frontend polls `Receiver::try_recv` once per frame.
pub enum WorkerEvent {
    Progress { done: usize, total: usize, message: String },
    Error(String),
    Finished(Vec<LabelResult>),
}

/// Run `AiLabeler` over every point in `annotations`, sending progress/result
/// events through `tx`. `cancel` is polled between points so the caller can
/// stop the job from another thread.
pub fn run_label_worker(
    mut labeler: AiLabeler,
    annotations: Vec<(String, ImageAnnotation)>, // (owning key, snapshot) since threads can't hold &Project
    class_mapping: HashMap<String, Option<String>>,
    conf_threshold: f32,
    crop_size: u32,
    overwrite_labeled: bool,
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    tx: Sender<WorkerEvent>,
) {
    use std::sync::atomic::Ordering;

    let total: usize = annotations
        .iter()
        .map(|(_, a)| a.points.iter().filter(|p| overwrite_labeled || p.label.is_none()).count())
        .sum();
    let mut done = 0usize;
    let mut results = Vec::new();

    'outer: for (_key, ann) in &annotations {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let img_name = Path::new(&ann.image_path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();

        let img = match image::open(&ann.image_path) {
            Ok(i) => i.to_rgb8(),
            Err(_) => {
                let _ = tx.send(WorkerEvent::Progress { done, total, message: format!("WARNING: could not read {img_name} — skipping") });
                continue;
            }
        };

        for p in &ann.points {
            if cancel.load(Ordering::Relaxed) {
                break 'outer;
            }
            if !overwrite_labeled && p.label.is_some() {
                continue;
            }

            let status = match labeler.predict_point(&img, p.x, p.y, crop_size) {
                Ok((predicted_class, confidence)) => {
                    let mapped_code = if confidence < conf_threshold {
                        None
                    } else {
                        class_mapping.get(&predicted_class).cloned().flatten()
                    };
                    let status = format!(
                        "{img_name} — Point #{}: {predicted_class} -> {} ({:.1}%)",
                        p.index + 1,
                        mapped_code.clone().unwrap_or_else(|| "(skip)".to_string()),
                        confidence * 100.0
                    );
                    results.push(LabelResult {
                        annotation_path: ann.image_path.clone(),
                        point_index: p.index,
                        predicted_class,
                        mapped_code,
                        confidence,
                    });
                    status
                }
                Err(e) => {
                    let _ = tx.send(WorkerEvent::Error(format!("{img_name} — Point #{}: {e}", p.index + 1)));
                    format!("{img_name} — Point #{}: (error)", p.index + 1)
                }
            };

            done += 1;
            let _ = tx.send(WorkerEvent::Progress { done, total, message: status });
        }
    }

    let _ = tx.send(WorkerEvent::Finished(results));
}

/// Spawn [`run_label_worker`] on a background thread, returning the receiver
/// and a cancellation flag.
pub fn spawn_label_worker(
    labeler: AiLabeler,
    annotations: Vec<(String, ImageAnnotation)>,
    class_mapping: HashMap<String, Option<String>>,
    conf_threshold: f32,
    crop_size: u32,
    overwrite_labeled: bool,
) -> (Receiver<WorkerEvent>, std::sync::Arc<std::sync::atomic::AtomicBool>) {
    let (tx, rx) = std::sync::mpsc::channel();
    let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let cancel_clone = cancel.clone();
    std::thread::spawn(move || {
        run_label_worker(labeler, annotations, class_mapping, conf_threshold, crop_size, overwrite_labeled, cancel_clone, tx);
    });
    (rx, cancel)
}
