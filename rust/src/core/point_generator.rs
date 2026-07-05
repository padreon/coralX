use anyhow::{bail, Result};
use rand::RngExt;

use crate::models::Point;

/// Generate points on an image.
///
/// * `distribution` - "random", "stratified", or "uniform"
/// * `border` - uniform pixel border to exclude (ignored when `border_rect` or
///   `border_polygon` is set)
/// * `border_rect` - `[x_min, y_min, x_max, y_max]` from a manual click; overrides `border`
/// * `border_polygon` - `[[x, y], ...]` from manual drawing; overrides `border_rect`
#[allow(clippy::too_many_arguments)]
pub fn generate_points(
    image_width: f64,
    image_height: f64,
    count: usize,
    distribution: &str,
    border: f64,
    border_rect: Option<&[f64]>,
    border_polygon: Option<&[Vec<f64>]>,
) -> Result<Vec<Point>> {
    if let Some(polygon) = border_polygon {
        if polygon.len() >= 3 {
            let coords = generate_in_polygon(polygon, count, distribution);
            return Ok(coords
                .into_iter()
                .enumerate()
                .map(|(i, (x, y))| Point { x, y, index: i as i64, label: None, category: None })
                .collect());
        }
    }

    let (x_min, y_min, x_max, y_max) = if let Some(rect) = border_rect {
        (rect[0], rect[1], rect[2], rect[3])
    } else {
        (border, border, image_width - border, image_height - border)
    };

    if x_min >= x_max || y_min >= y_max {
        bail!("Border exclusion too large for image size.");
    }

    let coords = match distribution {
        "random" => random_points(x_min, x_max, y_min, y_max, count),
        "stratified" => stratified_points(x_min, x_max, y_min, y_max, count),
        "uniform" => uniform_grid_points(x_min, x_max, y_min, y_max, count),
        other => bail!("Unknown distribution: {other}"),
    };

    Ok(coords
        .into_iter()
        .enumerate()
        .map(|(i, (x, y))| Point { x, y, index: i as i64, label: None, category: None })
        .collect())
}

/// Ray-casting point-in-polygon test.
fn point_in_polygon(x: f64, y: f64, polygon: &[Vec<f64>]) -> bool {
    let n = polygon.len();
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = (polygon[i][0], polygon[i][1]);
        let (xj, yj) = (polygon[j][0], polygon[j][1]);
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn generate_in_polygon(polygon: &[Vec<f64>], count: usize, distribution: &str) -> Vec<(f64, f64)> {
    let x_min = polygon.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min);
    let x_max = polygon.iter().map(|p| p[0]).fold(f64::NEG_INFINITY, f64::max);
    let y_min = polygon.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min);
    let y_max = polygon.iter().map(|p| p[1]).fold(f64::NEG_INFINITY, f64::max);

    if distribution == "uniform" {
        let density = (count * 9).max(900);
        let cols = (density as f64).sqrt().ceil() as usize;
        let rows = (density as f64 / cols as f64).ceil() as usize;
        let xs = linspace(x_min, x_max, cols);
        let ys = linspace(y_min, y_max, rows);

        let mut inside: Vec<(f64, f64)> = Vec::new();
        for &y in &ys {
            for &x in &xs {
                if point_in_polygon(x, y, polygon) {
                    inside.push((x, y));
                }
            }
        }

        if inside.len() >= count && count > 0 {
            let step = (inside.len() / count).max(1);
            return inside.into_iter().step_by(step).take(count).collect();
        }
        return inside;
    }

    // random / stratified: rejection sampling (matches the Python implementation,
    // which does not actually stratify this branch either).
    let mut coords: Vec<(f64, f64)> = Vec::new();
    let batch = (count * 4).max(200);
    let mut rng = rand::rng();
    for _ in 0..50 {
        if coords.len() >= count {
            break;
        }
        for _ in 0..batch {
            if coords.len() >= count {
                break;
            }
            let x = rng.random_range(x_min..=x_max);
            let y = rng.random_range(y_min..=y_max);
            if point_in_polygon(x, y, polygon) {
                coords.push((x, y));
            }
        }
    }
    coords.truncate(count);
    coords
}

fn random_points(x_min: f64, x_max: f64, y_min: f64, y_max: f64, count: usize) -> Vec<(f64, f64)> {
    let mut rng = rand::rng();
    (0..count)
        .map(|_| (rng.random_range(x_min..=x_max), rng.random_range(y_min..=y_max)))
        .collect()
}

/// Stratified random sampling — divides the image into grid cells, one point per cell.
fn stratified_points(x_min: f64, x_max: f64, y_min: f64, y_max: f64, count: usize) -> Vec<(f64, f64)> {
    if count == 0 {
        return Vec::new();
    }
    let cols = (count as f64).sqrt().ceil() as usize;
    let rows = (count as f64 / cols as f64).ceil() as usize;

    let cell_w = (x_max - x_min) / cols as f64;
    let cell_h = (y_max - y_min) / rows as f64;

    let mut rng = rand::rng();
    let mut coords = Vec::with_capacity(count);
    'outer: for row in 0..rows {
        for col in 0..cols {
            if coords.len() >= count {
                break 'outer;
            }
            let x = (x_min + col as f64 * cell_w + rng.random_range(0.0..=cell_w)).min(x_max);
            let y = (y_min + row as f64 * cell_h + rng.random_range(0.0..=cell_h)).min(y_max);
            coords.push((x, y));
        }
    }
    coords.truncate(count);
    coords
}

/// Uniform grid — evenly spaced points across the image.
fn uniform_grid_points(x_min: f64, x_max: f64, y_min: f64, y_max: f64, count: usize) -> Vec<(f64, f64)> {
    if count == 0 {
        return Vec::new();
    }
    let cols = (count as f64).sqrt().ceil() as usize;
    let rows = (count as f64 / cols as f64).ceil() as usize;

    let xs = linspace(x_min, x_max, cols);
    let ys = linspace(y_min, y_max, rows);

    let mut coords = Vec::with_capacity(rows * cols);
    for &y in &ys {
        for &x in &xs {
            coords.push((x, y));
        }
    }
    coords.truncate(count);
    coords
}

/// Mirrors `numpy.linspace(start, stop, num)`: `num` evenly spaced points
/// over `[start, stop]` inclusive, with the last point forced to `stop`.
fn linspace(start: f64, stop: f64, num: usize) -> Vec<f64> {
    if num == 0 {
        return Vec::new();
    }
    if num == 1 {
        return vec![start];
    }
    let step = (stop - start) / (num - 1) as f64;
    let mut values: Vec<f64> = (0..num).map(|i| start + i as f64 * step).collect();
    values[num - 1] = stop;
    values
}
