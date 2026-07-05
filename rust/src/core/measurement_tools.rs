//! Pure-Rust magic-wand selection, contour/orientation geometry, and
//! measurement math for the Fragment Measurement workflow.
//!
//! Reimplements (rather than binds to) OpenCV's flood fill, bilateral
//! filter, connected components, contour tracing, and min-area-rect, to
//! avoid a native OpenCV runtime dependency. See the project history for
//! the tradeoff discussion. Validate against the Python/OpenCV version's
//! output on real photos before relying on this for published measurements.

use std::sync::Mutex;
use std::time::SystemTime;

use geo::{Coord, LineString, MinimumRotatedRect, Polygon, Simplify};
use image::{GrayImage, Luma, RgbImage};
use imageproc::contours::BorderType;
use imageproc::filter::bilateral::{bilateral_filter, GaussianEuclideanColorDistance};
use imageproc::distance_transform::Norm;
use imageproc::morphology::{close, dilate};

// --- Magic-wand tuning -------------------------------------------------------
// Underwater lighting shades the same coral very differently, so the Lightness
// (L) channel is allowed to drift further than the chroma (a, b) channels: the
// same material at a different brightness should still be captured, while a
// genuinely different colour should not. Tolerance is the user's slider value.
const MW_L_WEIGHT: f64 = 2.0; // L tolerance multiplier (permissive on brightness)
const MW_AB_WEIGHT: f64 = 1.0; // a/b tolerance multiplier (strict on hue)
const MW_MEDIAN_KSIZE: u32 = 5; // median blur radius to kill coral speckle; 0 disables
const MW_DENOISE_RADIUS: u8 = 5; // bilateral-filter radius; 0 disables denoising
const MW_CLOSE_FRAC: f64 = 0.006; // close-kernel size as fraction of min(h, w)
const MW_CLOSE_MAX: u32 = 25; // clamp the adaptive close kernel (px)
const MW_MIN_SPECK_FRAC: f64 = 0.002; // drop foreground blobs smaller than this x fill area
const MW_HOLE_FILL_FRAC: f64 = 0.05; // fill interior holes smaller than this x fill area
const MW_MIN_AREA_ABS: usize = 16; // absolute pixel floor for speck/hole removal
const MW_FOCUS_GATE: bool = true; // block the fill from bleeding into blurry background
const MW_FOCUS_WIN_FRAC: f64 = 0.012; // local-sharpness window as fraction of min(h, w)

// A fragment only gets a TILTED measurement box when it genuinely leans like a
// stick: it must be BOTH clearly elongated (long/short >= ORIENT_MIN_ASPECT) AND
// leaning by more than ORIENT_MIN_ANGLE degrees. Default is upright.
const ORIENT_MIN_ASPECT: f64 = 2.2;
const ORIENT_MIN_ANGLE: f64 = 8.0;

/// A selection mask, one bool per pixel, row-major, unpadded (the Python
/// version pads by 1px to satisfy `cv2.floodFill`'s mask convention; that
/// convention doesn't apply here since flood fill is hand-rolled).
#[derive(Debug, Clone)]
pub struct Mask {
    pub width: usize,
    pub height: usize,
    pub data: Vec<bool>,
}

impl Mask {
    fn new(width: usize, height: usize) -> Self {
        Mask { width, height, data: vec![false; width * height] }
    }
    #[inline]
    fn get(&self, x: usize, y: usize) -> bool {
        self.data[y * self.width + x]
    }
    #[inline]
    fn set(&mut self, x: usize, y: usize, v: bool) {
        self.data[y * self.width + x] = v;
    }
    fn count(&self) -> usize {
        self.data.iter().filter(|&&v| v).count()
    }
}

/// Pixel-accurate area from a magic-wand mask, in real units^2.
pub fn mask_area(mask: &Mask, scale_factor: f64) -> f64 {
    mask.count() as f64 / scale_factor.powi(2)
}

/// Straight-line distance between two image-space points in real units.
pub fn line_length(p1: (f64, f64), p2: (f64, f64), scale_factor: f64) -> f64 {
    (p2.0 - p1.0).hypot(p2.1 - p1.1) / scale_factor
}

/// Sum of segment lengths along a polyline in real units.
pub fn polyline_length(points: &[(f64, f64)], scale_factor: f64) -> f64 {
    let total: f64 = points.windows(2).map(|w| (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1)).sum();
    total / scale_factor
}

/// Polygon area via the shoelace formula, in real units squared.
pub fn polygon_area(points: &[(f64, f64)], scale_factor: f64) -> f64 {
    let n = points.len();
    if n < 3 {
        return 0.0;
    }
    let mut area_px2 = 0.0;
    for i in 0..n {
        let (x0, y0) = points[i];
        let (x1, y1) = points[(i + 1) % n];
        area_px2 += x0 * y1 - x1 * y0;
    }
    (area_px2.abs() / 2.0) / scale_factor.powi(2)
}

/// Perimeter of a closed polygon in real units.
pub fn contour_perimeter(points: &[(f64, f64)], scale_factor: f64) -> f64 {
    let n = points.len();
    if n < 2 {
        return 0.0;
    }
    let total: f64 = (0..n)
        .map(|i| {
            let a = points[i];
            let b = points[(i + 1) % n];
            (b.0 - a.0).hypot(b.1 - a.1)
        })
        .sum();
    total / scale_factor
}

/// `(width_real, height_real)` from the axis-aligned bounding box of `points`.
pub fn bounding_box(points: &[(f64, f64)], scale_factor: f64) -> (f64, f64) {
    if points.is_empty() {
        return (0.0, 0.0);
    }
    let xs = points.iter().map(|p| p.0);
    let ys = points.iter().map(|p| p.1);
    let (x_min, x_max) = min_max(xs);
    let (y_min, y_max) = min_max(ys);
    ((x_max - x_min) / scale_factor, (y_max - y_min) / scale_factor)
}

fn min_max(it: impl Iterator<Item = f64>) -> (f64, f64) {
    it.fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| (lo.min(v), hi.max(v)))
}

/// `minimum_rotated_rect` of the points -> `(long_side, short_side, height_angle_deg)`.
///
/// `height_angle` is the lean of the LONG (height) axis from horizontal,
/// normalised to `(-90, 90]`.
fn rotated_box(points: &[(f64, f64)]) -> Option<(f64, f64, f64)> {
    if points.len() < 2 {
        return None;
    }
    let coords: Vec<Coord<f64>> = points.iter().map(|&(x, y)| Coord { x, y }).collect();
    let poly = if points.len() < 3 {
        // Degenerate (a line) — MinimumRotatedRect needs an area-bearing geometry;
        // fall back to treating the two extreme points as a zero-width rect.
        let (x_min, x_max) = min_max(points.iter().map(|p| p.0));
        let (y_min, y_max) = min_max(points.iter().map(|p| p.1));
        Polygon::new(
            LineString::from(vec![
                (x_min, y_min),
                (x_max, y_min),
                (x_max, y_max),
                (x_min, y_max),
                (x_min, y_min),
            ]),
            vec![],
        )
    } else {
        Polygon::new(LineString::new(coords), vec![])
    };

    let rect = poly.minimum_rotated_rect()?;
    let corners = rect_corners(&rect)?;
    Some(rotated_box_from_corners(&corners))
}

fn rect_corners(rect: &Polygon<f64>) -> Option<[(f64, f64); 4]> {
    let ext = rect.exterior();
    if ext.0.len() < 4 {
        return None;
    }
    Some([
        (ext.0[0].x, ext.0[0].y),
        (ext.0[1].x, ext.0[1].y),
        (ext.0[2].x, ext.0[2].y),
        (ext.0[3].x, ext.0[3].y),
    ])
}

fn rotated_box_from_corners(c: &[(f64, f64); 4]) -> (f64, f64, f64) {
    let edge_01 = dist(c[0], c[1]);
    let edge_12 = dist(c[1], c[2]);
    let (long_side, short_side, dir) =
        if edge_01 >= edge_12 { (edge_01, edge_12, (c[1].0 - c[0].0, c[1].1 - c[0].1)) } else { (edge_12, edge_01, (c[2].0 - c[1].0, c[2].1 - c[1].1)) };
    let mut angle = dir.1.atan2(dir.0).to_degrees();
    while angle <= -90.0 {
        angle += 180.0;
    }
    while angle > 90.0 {
        angle -= 180.0;
    }
    (long_side, short_side, angle)
}

fn dist(a: (f64, f64), b: (f64, f64)) -> f64 {
    (b.0 - a.0).hypot(b.1 - a.1)
}

/// True only for a stick-like fragment that genuinely leans: needs BOTH a high
/// elongation (long/short >= ORIENT_MIN_ASPECT) AND a lean past ORIENT_MIN_ANGLE
/// away from upright/flat.
fn should_tilt(long_side: f64, short_side: f64, height_angle: f64) -> bool {
    if short_side <= 1e-6 || long_side / short_side < ORIENT_MIN_ASPECT {
        return false;
    }
    let lean = height_angle.abs().min((height_angle.abs() - 90.0).abs());
    lean >= ORIENT_MIN_ANGLE
}

/// `(width_real, height_real, angle_deg)` from the minimum-area rotated box.
///
/// Only a stick-like fragment that genuinely leans gets a tilted box;
/// anything else falls back to the upright axis-aligned box with angle 0.
pub fn oriented_extent(points: &[(f64, f64)], scale_factor: f64) -> (f64, f64, f64) {
    let Some((long_side, short_side, height_angle)) = rotated_box(points) else {
        return (0.0, 0.0, 0.0);
    };
    if !should_tilt(long_side, short_side, height_angle) {
        let (w, h) = bounding_box(points, scale_factor);
        return (w, h, 0.0);
    }
    (short_side / scale_factor, long_side / scale_factor, height_angle)
}

pub struct OrientedAxes {
    pub corners: [(f64, f64); 4],
    pub height_line: ((f64, f64), (f64, f64)),
    pub width_line: ((f64, f64), (f64, f64)),
}

/// Geometry (image-space px) for drawing the tilt-aligned box and H/W axes.
pub fn oriented_axes(points: &[(f64, f64)]) -> Option<OrientedAxes> {
    let (long_side, short_side, height_angle) = rotated_box(points)?;

    if !should_tilt(long_side, short_side, height_angle) {
        let (x_min, x_max) = min_max(points.iter().map(|p| p.0));
        let (y_min, y_max) = min_max(points.iter().map(|p| p.1));
        let (cx, cy) = ((x_min + x_max) / 2.0, (y_min + y_max) / 2.0);
        return Some(OrientedAxes {
            corners: [(x_min, y_min), (x_max, y_min), (x_max, y_max), (x_min, y_max)],
            height_line: ((cx, y_min), (cx, y_max)),
            width_line: ((x_min, cy), (x_max, cy)),
        });
    }

    let coords: Vec<Coord<f64>> = points.iter().map(|&(x, y)| Coord { x, y }).collect();
    let poly = Polygon::new(LineString::new(coords), vec![]);
    let rect = poly.minimum_rotated_rect()?;
    let c = rect_corners(&rect)?;
    let mid = |a: usize, b: usize| ((c[a].0 + c[b].0) / 2.0, (c[a].1 + c[b].1) / 2.0);

    let len_a = dist(c[0], c[1]);
    let len_b = dist(c[1], c[2]);
    // c0-c1 the long edge => height spans the short-edge midpoints, and vice versa.
    let (height_line, width_line) = if len_a >= len_b {
        ((mid(1, 2), mid(3, 0)), (mid(0, 1), mid(2, 3)))
    } else {
        ((mid(0, 1), mid(2, 3)), (mid(1, 2), mid(3, 0)))
    };
    Some(OrientedAxes { corners: c, height_line, width_line })
}

// ---------------------------------------------------------------------------
// LAB colour conversion (matches OpenCV's 8-bit BGR2LAB scaling: L*255/100,
// a/b shifted by +128, so the tuned tolerance constants above still apply).
// ---------------------------------------------------------------------------

fn srgb_to_linear(c: f64) -> f64 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

fn lab_f(t: f64) -> f64 {
    const DELTA: f64 = 6.0 / 29.0;
    if t > DELTA.powi(3) {
        t.cbrt()
    } else {
        t / (3.0 * DELTA * DELTA) + 4.0 / 29.0
    }
}

/// One pixel's OpenCV-scaled Lab value: L in `[0,255]`, a/b in `[0,255]` (128 = neutral).
fn rgb_to_lab_u8(r: u8, g: u8, b: u8) -> [i16; 3] {
    let (r, g, b) = (srgb_to_linear(r as f64 / 255.0), srgb_to_linear(g as f64 / 255.0), srgb_to_linear(b as f64 / 255.0));
    let x = 0.4124564 * r + 0.3575761 * g + 0.1804375 * b;
    let y = 0.2126729 * r + 0.7151522 * g + 0.0721750 * b;
    let z = 0.0193339 * r + 0.1191920 * g + 0.9503041 * b;
    let (xr, yr, zr) = (x / 0.95047, y / 1.0, z / 1.08883);
    let (fx, fy, fz) = (lab_f(xr), lab_f(yr), lab_f(zr));
    let l = 116.0 * fy - 16.0;
    let a = 500.0 * (fx - fy);
    let bb = 200.0 * (fy - fz);
    let l255 = (l * 255.0 / 100.0).round().clamp(0.0, 255.0);
    let a255 = (a + 128.0).round().clamp(0.0, 255.0);
    let b255 = (bb + 128.0).round().clamp(0.0, 255.0);
    [l255 as i16, a255 as i16, b255 as i16]
}

struct LabImage {
    width: usize,
    height: usize,
    data: Vec<[i16; 3]>,
}

impl LabImage {
    fn from_rgb(img: &RgbImage) -> Self {
        let (w, h) = img.dimensions();
        let mut data = Vec::with_capacity((w * h) as usize);
        for p in img.pixels() {
            data.push(rgb_to_lab_u8(p[0], p[1], p[2]));
        }
        LabImage { width: w as usize, height: h as usize, data }
    }
    #[inline]
    fn get(&self, x: usize, y: usize) -> [i16; 3] {
        self.data[y * self.width + x]
    }
}

// ---------------------------------------------------------------------------
// Focus gating
// ---------------------------------------------------------------------------

/// Boolean in-focus map, or `None` if focus/blur gating doesn't cleanly apply
/// (a uniformly-focused photo is never wrongly gated).
fn focus_region(img: &RgbImage) -> Option<Vec<bool>> {
    let (w, h) = img.dimensions();
    let gray: GrayImage = image::DynamicImage::ImageRgb8(img.clone()).to_luma8();

    let lap = imageproc::filter::laplacian_filter(&gray);
    let win = (((w.min(h) as f64) * MW_FOCUS_WIN_FRAC).round() as u32).max(9) | 1;
    let radius = win / 2;

    // Energy = box-filtered squared Laplacian response.
    let lap_sq: GrayImage = GrayImage::from_fn(w, h, |x, y| {
        let v = lap.get_pixel(x, y).0[0] as f64;
        Luma([((v * v).sqrt().min(255.0)) as u8])
    });
    let energy = imageproc::filter::box_filter(&lap_sq, radius, radius);

    let level = imageproc::contrast::otsu_level(&energy);
    let mut sharp = imageproc::contrast::threshold(&energy, level, imageproc::contrast::ThresholdType::Binary);

    // Close to merge sharp texture into solid in-focus regions.
    let k = radius.max(1) as u8;
    sharp = close(&sharp, Norm::LInf, k);

    let focus: Vec<bool> = sharp.pixels().map(|p| p.0[0] > 0).collect();
    let frac = focus.iter().filter(|&&v| v).count() as f64 / focus.len() as f64;
    if !(0.05..=0.95).contains(&frac) {
        return None;
    }

    let focus_img = GrayImage::from_fn(w, h, |x, y| Luma([if focus[(y * w + x) as usize] { 255 } else { 0 }]));
    let dilated = dilate(&focus_img, Norm::LInf, k);
    Some(dilated.pixels().map(|p| p.0[0] > 0).collect())
}

/// Cached (per image path + mtime) speckle-suppressed Lab image + focus map,
/// so repeated union/subtract clicks on the same image reuse it.
struct PreparedImage {
    key: (String, Option<SystemTime>),
    lab: std::sync::Arc<LabImage>,
    focus: Option<std::sync::Arc<Vec<bool>>>,
}

static FLOOD_CACHE: Mutex<Option<PreparedImage>> = Mutex::new(None);

fn prepare(image_path: &str) -> Option<(std::sync::Arc<LabImage>, Option<std::sync::Arc<Vec<bool>>>)> {
    let mtime = std::fs::metadata(image_path).and_then(|m| m.modified()).ok();
    let key = (image_path.to_string(), mtime);

    {
        let cache = FLOOD_CACHE.lock().unwrap();
        if let Some(entry) = cache.as_ref() {
            if entry.key == key {
                return Some((entry.lab.clone(), entry.focus.clone()));
            }
        }
    }

    let mut img = image::open(image_path).ok()?.to_rgb8();
    let focus = if MW_FOCUS_GATE { focus_region(&img).map(std::sync::Arc::new) } else { None };

    if MW_MEDIAN_KSIZE > 0 {
        img = imageproc::filter::median_filter(&img, MW_MEDIAN_KSIZE, MW_MEDIAN_KSIZE);
    }
    if MW_DENOISE_RADIUS > 0 {
        img = bilateral_filter(&img, MW_DENOISE_RADIUS, 50.0, GaussianEuclideanColorDistance::new(50.0));
    }
    let lab = std::sync::Arc::new(LabImage::from_rgb(&img));

    *FLOOD_CACHE.lock().unwrap() =
        Some(PreparedImage { key, lab: lab.clone(), focus: focus.clone() });
    Some((lab, focus))
}

// ---------------------------------------------------------------------------
// Connected-component helpers (8-connected) for speck removal / hole filling
// ---------------------------------------------------------------------------

struct Component {
    area: usize,
    touches_border: bool,
    pixels: Vec<usize>,
}

fn label_components(grid: &[bool], w: usize, h: usize, target: bool) -> Vec<Component> {
    let mut visited = vec![false; grid.len()];
    let mut components = Vec::new();
    let mut stack = Vec::new();

    for start in 0..grid.len() {
        if visited[start] || grid[start] != target {
            continue;
        }
        visited[start] = true;
        stack.push(start);
        let mut pixels = Vec::new();
        let mut touches_border = false;

        while let Some(idx) = stack.pop() {
            pixels.push(idx);
            let (x, y) = (idx % w, idx / w);
            if x == 0 || y == 0 || x == w - 1 || y == h - 1 {
                touches_border = true;
            }
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    if dx == 0 && dy == 0 {
                        continue;
                    }
                    let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                    if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                        continue;
                    }
                    let nidx = ny as usize * w + nx as usize;
                    if !visited[nidx] && grid[nidx] == target {
                        visited[nidx] = true;
                        stack.push(nidx);
                    }
                }
            }
        }
        let area = pixels.len();
        components.push(Component { area, touches_border, pixels });
    }
    components
}

/// Fill enclosed background holes (not touching the image border) up to
/// `max_hole_area` px; genuine large holes (donut shapes) are preserved.
fn fill_small_holes(region: &mut [bool], w: usize, h: usize, max_hole_area: f64) {
    for comp in label_components(region, w, h, false) {
        if !comp.touches_border && (comp.area as f64) < max_hole_area {
            for idx in comp.pixels {
                region[idx] = true;
            }
        }
    }
}

/// Area-driven cleanup of a fresh magic-wand fill: drop foreground specks,
/// then fill small interior background holes. Preserves genuinely large holes.
fn clean_fill_region(region: &mut [bool], w: usize, h: usize) {
    let fg_area = region.iter().filter(|&&v| v).count();
    if fg_area == 0 {
        return;
    }

    let speck_thr = (fg_area as f64 * MW_MIN_SPECK_FRAC).max(MW_MIN_AREA_ABS as f64);
    for comp in label_components(region, w, h, true) {
        if (comp.area as f64) < speck_thr {
            for idx in comp.pixels {
                region[idx] = false;
            }
        }
    }

    let hole_thr = (fg_area as f64 * MW_HOLE_FILL_FRAC).max(MW_MIN_AREA_ABS as f64);
    fill_small_holes(region, w, h, hole_thr);
}

// ---------------------------------------------------------------------------
// Flood fill (FIXED_RANGE: every candidate pixel is compared to the seed
// colour, not its neighbours — true magic-wand behaviour), 4-connected.
// ---------------------------------------------------------------------------

fn flood_fill(
    lab: &LabImage,
    seed_x: usize,
    seed_y: usize,
    tol: [i16; 3],
    barrier: &[bool],
) -> Vec<bool> {
    let (w, h) = (lab.width, lab.height);
    let seed = lab.get(seed_x, seed_y);
    let mut filled = vec![false; w * h];
    let start = seed_y * w + seed_x;
    if barrier[start] {
        return filled;
    }
    filled[start] = true;
    let mut stack = vec![start];
    while let Some(idx) = stack.pop() {
        let (x, y) = (idx % w, idx / w);
        for (dx, dy) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)] {
            let (nx, ny) = (x as i32 + dx, y as i32 + dy);
            if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                continue;
            }
            let nidx = ny as usize * w + nx as usize;
            if filled[nidx] || barrier[nidx] {
                continue;
            }
            let p = lab.get(nx as usize, ny as usize);
            if (p[0] - seed[0]).abs() <= tol[0] && (p[1] - seed[1]).abs() <= tol[1] && (p[2] - seed[2]).abs() <= tol[2] {
                filled[nidx] = true;
                stack.push(nidx);
            }
        }
    }
    filled
}

/// Select pixels similar in colour to the seed point using flood fill.
///
/// If `existing_mask` is provided, the cleaned new fill is OR-ed into it so
/// successive clicks expand the selection (union); cleanup only touches the
/// new fill so earlier regions are never eroded. Returns `(contour, mask)`.
pub fn magic_wand_select(
    image_path: &str,
    seed_px: i64,
    seed_py: i64,
    tolerance: i64,
    smoothing: i64,
    existing_mask: Option<&Mask>,
) -> (Option<Vec<(f64, f64)>>, Option<Mask>) {
    let Some((lab, focus)) = prepare(image_path) else { return (None, None) };
    let (w, h) = (lab.width, lab.height);
    let seed_x = seed_px.clamp(0, w as i64 - 1) as usize;
    let seed_y = seed_py.clamp(0, h as i64 - 1) as usize;

    let tol = tolerance.max(1) as f64;
    let l_tol = ((tol * MW_L_WEIGHT).round() as i16).max(1);
    let ab_tol = ((tol * MW_AB_WEIGHT).round() as i16).max(1);

    let mut barrier = vec![false; w * h];
    if let Some(focus) = &focus {
        if focus[seed_y * w + seed_x] {
            for i in 0..w * h {
                barrier[i] = !focus[i];
            }
        }
    }

    let mut new_fill = flood_fill(&lab, seed_x, seed_y, [l_tol, ab_tol, ab_tol], &barrier);
    clean_fill_region(&mut new_fill, w, h);

    let mut mask = match existing_mask {
        Some(m) if m.width == w && m.height == h => m.clone(),
        _ => Mask::new(w, h),
    };
    for i in 0..w * h {
        if new_fill[i] {
            mask.data[i] = true;
        }
    }

    let sel_area = mask.count();
    if sel_area > 0 {
        let hole_thr = (sel_area as f64 * MW_HOLE_FILL_FRAC).max(MW_MIN_AREA_ABS as f64);
        fill_small_holes(&mut mask.data, w, h, hole_thr);
    }

    let region_u8 = GrayImage::from_fn(w as u32, h as u32, |x, y| {
        Luma([if mask.get(x as usize, y as usize) { 255 } else { 0 }])
    });
    let contours = imageproc::contours::find_contours::<i32>(&region_u8);
    let Some(largest) = contours
        .iter()
        .filter(|c| c.border_type == BorderType::Outer)
        .max_by(|a, b| contour_shoelace_area(a).partial_cmp(&contour_shoelace_area(b)).unwrap())
    else {
        return (None, Some(mask));
    };
    if contour_shoelace_area(largest) < 9.0 {
        return (None, Some(mask));
    }

    let line: LineString<f64> =
        LineString::from(largest.points.iter().map(|p| (p.x as f64, p.y as f64)).collect::<Vec<_>>());
    let perimeter_px: f64 = largest
        .points
        .windows(2)
        .map(|w| (((w[1].x - w[0].x).pow(2) + (w[1].y - w[0].y).pow(2)) as f64).sqrt())
        .sum();
    // smoothing 0 = follow every corner (epsilon ~0.5px); 100 = aggressive (epsilon ~4% perimeter)
    let epsilon = 0.5 + (smoothing as f64 / 100.0) * 0.04 * perimeter_px;
    let simplified = line.simplify(epsilon);

    if simplified.0.len() < 3 {
        return (None, Some(mask));
    }
    let pts: Vec<(f64, f64)> = simplified.0.iter().map(|c| (c.x, c.y)).collect();
    (Some(pts), Some(mask))
}

fn contour_shoelace_area(c: &imageproc::contours::Contour<i32>) -> f64 {
    let pts = &c.points;
    let n = pts.len();
    if n < 3 {
        return 0.0;
    }
    let mut area = 0.0;
    for i in 0..n {
        let a = pts[i];
        let b = pts[(i + 1) % n];
        area += (a.x * b.y - b.x * a.y) as f64;
    }
    area.abs() / 2.0
}

/// Right-click subtract: remove only the natural image feature under the
/// seed (e.g. one hole in a plate), leaving spatially-separate regions of
/// similar colour (e.g. a second hole) intact.
///
/// Sweeps the subtract tolerance from tight to broad and stops just before
/// the filled area jumps sharply (a "break-through" past the feature's real
/// edge into the connected body), so the clicked feature is fully captured
/// without bleeding into neighbouring similarly-coloured regions. See the
/// Python implementation's docstring for the full rationale.
pub fn magic_wand_subtract(image_path: &str, seed_px: i64, seed_py: i64, tolerance: i64, existing_mask: &Mask) -> Option<Mask> {
    let img = image::open(image_path).ok()?.to_rgb8();
    let (w, h) = (img.dimensions().0 as usize, img.dimensions().1 as usize);
    if existing_mask.width != w || existing_mask.height != h {
        return None;
    }
    let seed_x = seed_px.clamp(0, w as i64 - 1) as usize;
    let seed_y = seed_py.clamp(0, h as i64 - 1) as usize;

    if !existing_mask.get(seed_x, seed_y) {
        return Some(existing_mask.clone());
    }

    let lab = LabImage::from_rgb(&img);
    // Barrier: every pixel NOT currently selected walls off the subtract fill,
    // so it can never grow outside the selection.
    let barrier: Vec<bool> = (0..w * h).map(|i| !existing_mask.data[i]).collect();

    let cap = tolerance.max(1);
    let step = (cap / 12).max(1);
    let mut tol_values: Vec<i64> = (1..=cap).step_by(step as usize).collect();
    if tol_values.last() != Some(&cap) {
        tol_values.push(cap);
    }

    let mut best_mask: Option<Vec<bool>> = None;
    let mut prev_mask: Option<Vec<bool>> = None;
    let mut prev_area = 0usize;
    for tol in tol_values {
        let t = tol as i16;
        let m = flood_fill(&lab, seed_x, seed_y, [t, t, t], &barrier);
        let area = m.iter().filter(|&&v| v).count();
        if area == 0 {
            continue;
        }
        if prev_area > 0 && area > prev_area * 3 && area - prev_area > 50 {
            best_mask = prev_mask;
            break;
        }
        best_mask = Some(m.clone());
        prev_mask = Some(m);
        prev_area = area;
    }

    let Some(best) = best_mask else { return Some(existing_mask.clone()) };
    let mut result = existing_mask.clone();
    for i in 0..w * h {
        if best[i] {
            result.data[i] = false;
        }
    }
    Some(result)
}
