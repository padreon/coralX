//! Manual verification harness for `core::ai_labeler` against a real model
//! and photo, since it's not practical to bundle YOLO inference into `cargo
//! test`. Run with `cargo run --example verify_ai_labeler`.
//!
//! Expected results, cross-checked against the original PyTorch model
//! (see the export/verification notes in `core::ai_labeler`):
//! - Whole-image classify: "submassive", conf ~0.82 (PyTorch: 0.8375 on the
//!   exact same full-resolution decode; the gap here is expected since this
//!   harness still routes through `crop_around`, which truncates the odd
//!   447x447 image by 1px before resizing).
//! - Point-crop classify (64px crop around the image center): "massive",
//!   conf ~0.54 (PyTorch on the identical pixel crop: 0.5284). Confirmed via
//!   direct pixel dump that the ~2% gap is JPEG-decoder rounding (Rust's vs
//!   OpenCV's decoder disagree by +-2 on a couple of pixels), not a pipeline
//!   bug — both agree on every pixel we checked except one, off by 2/255.

use coralx::core::ai_labeler::AiLabeler;

fn main() -> anyhow::Result<()> {
    let mut labeler = AiLabeler::load("../data/data-training.onnx")?;
    println!("task: {}", labeler.task());
    println!("classes: {:?}", labeler.class_names());

    let img = image::open("../data-training/coral1.jpeg")?.to_rgb8();
    let (w, h) = (img.width() as f64, img.height() as f64);

    let (whole_class, whole_conf) = labeler.predict_point(&img, w / 2.0, h / 2.0, img.width().max(img.height()))?;
    println!("whole-image predict: {whole_class} conf={whole_conf} (expected: submassive, ~0.8375)");

    let (crop_class, crop_conf) = labeler.predict_point(&img, w / 2.0, h / 2.0, 64)?;
    println!("64px-crop predict: {crop_class} conf={crop_conf} (expected: massive, ~0.53-0.54)");

    Ok(())
}
