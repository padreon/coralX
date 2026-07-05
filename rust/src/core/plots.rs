//! Chart generation for coralX, rendered to PNG via `plotters` (matplotlib's
//! role in the Python version). Every function returns `None` when the
//! underlying data is insufficient, mirroring the Python API.

use std::path::{Path, PathBuf};

use plotters::prelude::*;

use crate::core::multivariate::{LinkageResult, PcoaResult};
use crate::core::statistics::Summary;
use crate::core::table::{Cell, Row};
use crate::core::validation::can_run_multivariate;
use crate::models::Project;

fn health_color(category: &str) -> RGBColor {
    match category {
        "Poor" => RGBColor(0xd6, 0x27, 0x28),
        "Fair" => RGBColor(0xff, 0x7f, 0x0e),
        "Good" => RGBColor(0x2c, 0xa0, 0x2c),
        "Excellent" => RGBColor(0x1f, 0x77, 0xb4),
        _ => RGBColor(0xaa, 0xaa, 0xaa),
    }
}

const BAR_BLUE: RGBColor = RGBColor(0x48, 0x78, 0xcf);
const BAR_GREEN: RGBColor = RGBColor(0x6a, 0xcc, 0x65);
const BAR_RED: RGBColor = RGBColor(0xd6, 0x5f, 0x5f);

fn cell_f64(row: &Row, col: &str) -> Option<f64> {
    row.iter().find(|(k, _)| k == col).and_then(|(_, v)| v.as_f64())
}
fn cell_str(row: &Row, col: &str) -> Option<String> {
    row.iter().find(|(k, _)| k == col).and_then(|(_, v)| match v {
        Cell::Str(s) => Some(s.clone()),
        _ => None,
    })
}

/// Horizontal bar chart of benthic coverage % with 95% CI error bars.
pub fn plot_coverage_bar(summary: &Summary, output_path: impl AsRef<Path>) -> Option<PathBuf> {
    if summary.coverage_ci.is_empty() {
        return None;
    }
    let mut codes: Vec<&String> = summary.coverage_ci.keys().collect();
    codes.sort_by(|a, b| summary.coverage_ci[*a].0.partial_cmp(&summary.coverage_ci[*b].0).unwrap());

    let n = codes.len();
    let max_val = codes.iter().map(|c| summary.coverage_ci[*c].2).fold(0.0_f64, f64::max);
    let x_max = (max_val * 1.25 + 5.0).min(100.0).max(10.0);

    let path = output_path.as_ref().to_path_buf();
    let save_path = path.clone();
    let root = BitMapBackend::new(&save_path, (1000, (n as u32 * 45).max(400))).into_drawing_area();
    root.fill(&WHITE).ok()?;
    let mut chart = ChartBuilder::on(&root)
        .caption("Benthic Coverage (%)", ("sans-serif", 22))
        .margin(20)
        .x_label_area_size(35)
        .y_label_area_size(80)
        .build_cartesian_2d(0f64..x_max, 0i32..n as i32)
        .ok()?;
    chart
        .configure_mesh()
        .disable_y_mesh()
        .y_labels(n)
        .y_label_formatter(&|y| codes.get(*y as usize).map(|s| s.to_string()).unwrap_or_default())
        .x_desc("Coverage (%)")
        .draw()
        .ok()?;

    for (i, code) in codes.iter().enumerate() {
        let (pct, lo, hi) = summary.coverage_ci[*code];
        let y = i as i32;
        chart.draw_series(std::iter::once(Rectangle::new([(0.0, y), (pct, y + 1)], BAR_BLUE.mix(0.85).filled()))).ok()?;
        chart.draw_series(std::iter::once(ErrorBar::new_horizontal(y * 2 + 1, lo, pct, hi, BLACK, 6))).ok()?;
        chart
            .draw_series(std::iter::once(Text::new(format!("{pct:.1}%"), (pct + 0.5, y), ("sans-serif", 13))))
            .ok()?;
    }
    root.present().ok()?;
    Some(path)
}

/// Donut chart of benthic group composition.
pub fn plot_lifeform_pie(summary: &Summary, output_path: impl AsRef<Path>) -> Option<PathBuf> {
    if summary.group_coverage.is_empty() {
        return None;
    }
    let mut labels: Vec<&String> = summary.group_coverage.keys().collect();
    labels.sort();
    let sizes: Vec<f64> = labels.iter().map(|l| summary.group_coverage[*l]).collect();
    let palette = [
        BAR_BLUE, BAR_GREEN, BAR_RED,
        RGBColor(0xff, 0x7f, 0x0e), RGBColor(0x94, 0x67, 0xbd), RGBColor(0x8c, 0x56, 0x4b),
    ];
    let colors: Vec<RGBColor> = (0..labels.len()).map(|i| palette[i % palette.len()]).collect();
    let display_labels: Vec<String> = labels.iter().zip(&sizes).map(|(l, s)| format!("{l} ({s:.1}%)")).collect();

    let path = output_path.as_ref().to_path_buf();
    let save_path = path.clone();
    let root = BitMapBackend::new(&save_path, (900, 700)).into_drawing_area();
    root.fill(&WHITE).ok()?;
    root.titled("Life-form Composition", ("sans-serif", 22)).ok()?;
    let center = (350i32, 370i32);
    let radius = 260.0;
    let mut pie = Pie::new(&center, &radius, &sizes, &colors, &display_labels);
    pie.donut_hole(radius * 0.5);
    pie.percentages(("sans-serif", 14).into_font().color(&WHITE));
    root.draw(&pie).ok()?;
    root.present().ok()?;
    Some(path)
}

/// Grouped vertical bars for up to 3 series against a shared category axis.
/// `series` is `(label, color, values)`; `values[i]` corresponds to `categories[i]`.
fn grouped_bar_chart(
    path: &Path,
    title: &str,
    categories: &[String],
    series: &[(&str, RGBColor, Vec<f64>)],
    y_desc: &str,
) -> Option<()> {
    let n = categories.len();
    let y_max = series.iter().flat_map(|(_, _, v)| v.iter().copied()).fold(0.0_f64, f64::max) * 1.15 + 0.05;
    let width = 800.0 / n.max(1) as f64;

    let root = BitMapBackend::new(path, ((n as u32 * 140).max(700), 650)).into_drawing_area();
    root.fill(&WHITE).ok()?;
    let mut chart = ChartBuilder::on(&root)
        .caption(title, ("sans-serif", 22))
        .margin(20)
        .x_label_area_size(90)
        .y_label_area_size(60)
        .build_cartesian_2d(0f64..n as f64, 0f64..y_max)
        .ok()?;
    chart
        .configure_mesh()
        .disable_x_mesh()
        .x_labels(n)
        .x_label_formatter(&|x| categories.get(x.round() as usize).cloned().unwrap_or_default())
        .x_label_style(("sans-serif", 13).into_font().transform(FontTransform::Rotate90))
        .y_desc(y_desc)
        .draw()
        .ok()?;

    let n_series = series.len() as f64;
    let group_w = 0.8 / n_series;
    for (s_idx, (label, color, values)) in series.iter().enumerate() {
        chart
            .draw_series(values.iter().enumerate().map(|(i, &v)| {
                let x0 = i as f64 + 0.1 + s_idx as f64 * group_w;
                Rectangle::new([(x0, 0.0), (x0 + group_w, v)], color.mix(0.85).filled())
            }))
            .ok()?
            .label(*label)
            .legend(move |(x, y)| Rectangle::new([(x, y - 5), (x + 12, y + 5)], color.filled()));
    }
    let _ = width;
    chart.configure_series_labels().background_style(WHITE.mix(0.8)).border_style(BLACK).draw().ok()?;
    root.present().ok()?;
    Some(())
}

/// Grouped bar chart: Shannon H', Simpson 1-D, Hill q1 per station.
pub fn plot_diversity_bar(per_station_rows: &[Row], output_path: impl AsRef<Path>) -> Option<PathBuf> {
    let rows: Vec<&Row> = per_station_rows.iter().filter(|r| cell_f64(r, "shannon_H").is_some()).collect();
    if rows.is_empty() {
        return None;
    }
    let stations: Vec<String> = rows.iter().map(|r| cell_str(r, "station").unwrap_or_default()).collect();
    let shannon: Vec<f64> = rows.iter().map(|r| cell_f64(r, "shannon_H").unwrap_or(0.0)).collect();
    let simpson: Vec<f64> = rows.iter().map(|r| cell_f64(r, "simpson_1D").unwrap_or(0.0)).collect();
    let hill_q1: Vec<f64> = rows.iter().map(|r| cell_f64(r, "hill_q1").unwrap_or(0.0)).collect();

    let path = output_path.as_ref().to_path_buf();
    grouped_bar_chart(
        &path,
        "Diversity Indices per Station",
        &stations,
        &[("Shannon H'", BAR_BLUE, shannon), ("Simpson 1-D", BAR_GREEN, simpson), ("Hill q1", BAR_RED, hill_q1)],
        "Index Value",
    )?;
    Some(path)
}

/// Bar chart of Mortality Index per station; bars turn red above the 0.5
/// critical threshold, marked with a dashed line.
pub fn plot_mortality_bar(per_station_rows: &[Row], output_path: impl AsRef<Path>) -> Option<PathBuf> {
    let rows: Vec<&Row> = per_station_rows.iter().filter(|r| cell_f64(r, "mortality_index").is_some()).collect();
    if rows.is_empty() {
        return None;
    }
    let stations: Vec<String> = rows.iter().map(|r| cell_str(r, "station").unwrap_or_default()).collect();
    let mis: Vec<f64> = rows.iter().map(|r| cell_f64(r, "mortality_index").unwrap_or(0.0)).collect();

    let n = stations.len();
    let path = output_path.as_ref().to_path_buf();
    let save_path = path.clone();
    let root = BitMapBackend::new(&save_path, ((n as u32 * 130).max(700), 600)).into_drawing_area();
    root.fill(&WHITE).ok()?;
    let mut chart = ChartBuilder::on(&root)
        .caption("Mortality Index per Station", ("sans-serif", 22))
        .margin(20)
        .x_label_area_size(90)
        .y_label_area_size(50)
        .build_cartesian_2d(0f64..n as f64, 0f64..1.05)
        .ok()?;
    chart
        .configure_mesh()
        .disable_x_mesh()
        .x_labels(n)
        .x_label_formatter(&|x| stations.get(x.round() as usize).cloned().unwrap_or_default())
        .x_label_style(("sans-serif", 13).into_font().transform(FontTransform::Rotate90))
        .y_desc("Mortality Index (MI)")
        .draw()
        .ok()?;

    chart
        .draw_series(mis.iter().enumerate().map(|(i, &mi)| {
            let color = if mi > 0.5 { RGBColor(0xd6, 0x27, 0x28) } else { BAR_BLUE };
            Rectangle::new([(i as f64 + 0.1, 0.0), (i as f64 + 0.9, mi)], color.mix(0.85).filled())
        }))
        .ok()?;
    for (i, &mi) in mis.iter().enumerate() {
        chart
            .draw_series(std::iter::once(Text::new(format!("{mi:.2}"), (i as f64 + 0.5, mi + 0.02), ("sans-serif", 12))))
            .ok()?;
    }
    chart
        .draw_series(std::iter::once(PathElement::new(vec![(0.0, 0.5), (n as f64, 0.5)], RGBColor(0x55, 0x55, 0x55).stroke_width(2))))
        .ok()?
        .label("Critical threshold (0.5)")
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], RGBColor(0x55, 0x55, 0x55)));
    chart.configure_series_labels().background_style(WHITE.mix(0.8)).border_style(BLACK).draw().ok()?;
    root.present().ok()?;
    Some(path)
}

/// Horizontal bar of live coral % per station, colour-coded by Gomez & Yap
/// reef-health category, with background classification zones.
pub fn plot_reef_health(per_station_rows: &[Row], output_path: impl AsRef<Path>) -> Option<PathBuf> {
    let rows: Vec<&Row> = per_station_rows
        .iter()
        .filter(|r| cell_str(r, "reef_health_category").is_some() && cell_f64(r, "group_Hard Coral").is_some())
        .collect();
    if rows.is_empty() {
        return None;
    }
    let stations: Vec<String> = rows.iter().map(|r| cell_str(r, "station").unwrap_or_default()).collect();
    let live_pcts: Vec<f64> = rows.iter().map(|r| cell_f64(r, "group_Hard Coral").unwrap_or(0.0)).collect();
    let categories: Vec<String> = rows.iter().map(|r| cell_str(r, "reef_health_category").unwrap_or_default()).collect();

    let n = stations.len();
    let path = output_path.as_ref().to_path_buf();
    let save_path = path.clone();
    let root = BitMapBackend::new(&save_path, (1000, (n as u32 * 50).max(400))).into_drawing_area();
    root.fill(&WHITE).ok()?;
    let mut chart = ChartBuilder::on(&root)
        .caption("Reef Health by Station (Gomez & Yap 1988)", ("sans-serif", 22))
        .margin(20)
        .x_label_area_size(35)
        .y_label_area_size(100)
        .build_cartesian_2d(0f64..100f64, 0i32..n as i32)
        .ok()?;
    chart
        .configure_mesh()
        .disable_y_mesh()
        .y_labels(n)
        .y_label_formatter(&|y| stations.get(*y as usize).cloned().unwrap_or_default())
        .x_desc("Live Hard Coral Cover (%)")
        .draw()
        .ok()?;

    let zones: [(f64, f64, RGBColor); 4] = [
        (0.0, 25.0, RGBColor(0xff, 0xd0, 0xd0)),
        (25.0, 50.0, RGBColor(0xff, 0xe8, 0xc0)),
        (50.0, 75.0, RGBColor(0xd0, 0xf0, 0xd0)),
        (75.0, 100.0, RGBColor(0xc0, 0xdf, 0xf0)),
    ];
    for (lo, hi, color) in zones {
        chart
            .draw_series(std::iter::once(Rectangle::new([(lo, 0), (hi, n as i32)], color.mix(0.35).filled())))
            .ok()?;
    }

    for (i, (pct, cat)) in live_pcts.iter().zip(&categories).enumerate() {
        let y = i as i32;
        chart
            .draw_series(std::iter::once(Rectangle::new([(0.0, y), (*pct, y + 1)], health_color(cat).mix(0.9).filled())))
            .ok()?;
        chart
            .draw_series(std::iter::once(Text::new(format!("{pct:.1}% - {cat}"), (pct + 1.0, y), ("sans-serif", 12))))
            .ok()?;
    }
    root.present().ok()?;
    Some(path)
}

/// Scatter plot of PCoA axis 1 vs axis 2, labelled by station name.
pub fn plot_ordination(pcoa_result: &PcoaResult, sample_names: &[String], output_path: impl AsRef<Path>) -> Option<PathBuf> {
    if pcoa_result.coords.nrows() < 2 {
        return None;
    }
    let n_axes = pcoa_result.coords.ncols();
    let x: Vec<f64> = (0..pcoa_result.coords.nrows()).map(|i| pcoa_result.coords[(i, 0)]).collect();
    let y: Vec<f64> = if n_axes > 1 {
        (0..pcoa_result.coords.nrows()).map(|i| pcoa_result.coords[(i, 1)]).collect()
    } else {
        vec![0.0; x.len()]
    };

    let (x_min, x_max) = pad_range(&x);
    let (y_min, y_max) = pad_range(&y);

    let path = output_path.as_ref().to_path_buf();
    let save_path = path.clone();
    let root = BitMapBackend::new(&save_path, (900, 800)).into_drawing_area();
    root.fill(&WHITE).ok()?;
    let pct1 = pcoa_result.variance_explained.first().map(|v| format!("{:.1}%", v * 100.0)).unwrap_or_default();
    let pct2 = pcoa_result.variance_explained.get(1).map(|v| format!("{:.1}%", v * 100.0)).unwrap_or_default();
    let mut chart = ChartBuilder::on(&root)
        .caption("PCoA Ordination (Bray-Curtis)", ("sans-serif", 22))
        .margin(20)
        .x_label_area_size(40)
        .y_label_area_size(50)
        .build_cartesian_2d(x_min..x_max, y_min..y_max)
        .ok()?;
    chart.configure_mesh().x_desc(format!("PCoA1 ({pct1})")).y_desc(format!("PCoA2 ({pct2})")).draw().ok()?;
    chart.draw_series(std::iter::once(PathElement::new(vec![(x_min, 0.0), (x_max, 0.0)], BLACK.mix(0.3)))).ok()?;
    chart.draw_series(std::iter::once(PathElement::new(vec![(0.0, y_min), (0.0, y_max)], BLACK.mix(0.3)))).ok()?;
    chart.draw_series(x.iter().zip(&y).map(|(&px, &py)| Circle::new((px, py), 5, BAR_BLUE.mix(0.85).filled()))).ok()?;
    for (i, name) in sample_names.iter().enumerate() {
        chart.draw_series(std::iter::once(Text::new(name.clone(), (x[i], y[i]), ("sans-serif", 12)))).ok()?;
    }
    root.present().ok()?;
    Some(path)
}

fn pad_range(values: &[f64]) -> (f64, f64) {
    let lo = values.iter().copied().fold(f64::INFINITY, f64::min).min(0.0);
    let hi = values.iter().copied().fold(f64::NEG_INFINITY, f64::max).max(0.0);
    let pad = ((hi - lo) * 0.15).max(0.01);
    (lo - pad, hi + pad)
}

/// Hierarchical clustering dendrogram from a Bray-Curtis / UPGMA linkage matrix.
pub fn plot_dendrogram(linkage_result: &LinkageResult, sample_names: &[String], output_path: impl AsRef<Path>) -> Option<PathBuf> {
    if linkage_result.linkage.is_empty() || sample_names.len() < 2 {
        return None;
    }
    let n = sample_names.len();
    // Leaf x-position: in-order traversal of the merge tree so branches don't cross.
    let mut leaf_order: Vec<usize> = Vec::new();
    fn collect_leaves(node: usize, n: usize, linkage: &[[f64; 4]], out: &mut Vec<usize>) {
        if node < n {
            out.push(node);
        } else {
            let row = &linkage[node - n];
            collect_leaves(row[0] as usize, n, linkage, out);
            collect_leaves(row[1] as usize, n, linkage, out);
        }
    }
    collect_leaves(n + linkage_result.linkage.len() - 1, n, &linkage_result.linkage, &mut leaf_order);

    let max_height = linkage_result.linkage.iter().map(|r| r[2]).fold(0.0_f64, f64::max).max(1e-9);

    let path = output_path.as_ref().to_path_buf();
    let save_path = path.clone();
    let root = BitMapBackend::new(&save_path, ((n as u32 * 110).max(800), 550)).into_drawing_area();
    root.fill(&WHITE).ok()?;
    let mut chart = ChartBuilder::on(&root)
        .caption(format!("Hierarchical Clustering ({})", linkage_result.method.to_uppercase()), ("sans-serif", 22))
        .margin(20)
        .x_label_area_size(90)
        .y_label_area_size(60)
        .build_cartesian_2d(-0.5f64..n as f64 - 0.5, 0f64..max_height * 1.1)
        .ok()?;
    chart
        .configure_mesh()
        .disable_x_mesh()
        .x_labels(n)
        .x_label_formatter(&|x| {
            let idx = x.round() as i64;
            if idx < 0 || idx as usize >= leaf_order.len() {
                return String::new();
            }
            sample_names.get(leaf_order[idx as usize]).cloned().unwrap_or_default()
        })
        .x_label_style(("sans-serif", 13).into_font().transform(FontTransform::Rotate90))
        .y_desc("Bray-Curtis Dissimilarity")
        .draw()
        .ok()?;

    // Each leaf's x tick slot is its position in the crossing-free leaf order.
    let mut slot_of = vec![0.0; n + linkage_result.linkage.len()];
    for (slot, &leaf) in leaf_order.iter().enumerate() {
        slot_of[leaf] = slot as f64;
    }
    for (i, row) in linkage_result.linkage.iter().enumerate() {
        let (a, b, height) = (row[0] as usize, row[1] as usize, row[2]);
        let node = n + i;
        // An internal node's slot is the midpoint of its children's slots;
        // linkage rows are in merge order, so any child is already resolved.
        let xa = slot_of[a];
        let xb = slot_of[b];
        slot_of[node] = (xa + xb) / 2.0;

        let ya = if a < n { 0.0 } else { linkage_result.linkage[a - n][2] };
        let yb = if b < n { 0.0 } else { linkage_result.linkage[b - n][2] };
        chart
            .draw_series(LineSeries::new(vec![(xa, ya), (xa, height), (xb, height), (xb, yb)], BAR_BLUE))
            .ok()?;
    }
    root.present().ok()?;
    Some(path)
}

/// Generate every applicable chart for `project` into `output_dir`.
///
/// Silently skips charts when data is insufficient (returns fewer paths).
pub fn export_all_charts(project: &Project, output_dir: impl AsRef<Path>) -> std::io::Result<Vec<PathBuf>> {
    let output_dir = output_dir.as_ref();
    std::fs::create_dir_all(output_dir)?;
    let mut paths = Vec::new();

    let summary = crate::core::statistics::project_summary(project);
    let station_rows = crate::core::statistics::per_station_table(project);

    if let Some(s) = &summary {
        if let Some(p) = plot_coverage_bar(s, output_dir.join("01_coverage_bar.png")) {
            paths.push(p);
        }
        if let Some(p) = plot_lifeform_pie(s, output_dir.join("02_lifeform_pie.png")) {
            paths.push(p);
        }
    }
    if let Some(p) = plot_diversity_bar(&station_rows, output_dir.join("03_diversity_bar.png")) {
        paths.push(p);
    }
    if let Some(p) = plot_mortality_bar(&station_rows, output_dir.join("04_mortality_bar.png")) {
        paths.push(p);
    }
    if let Some(p) = plot_reef_health(&station_rows, output_dir.join("05_reef_health.png")) {
        paths.push(p);
    }

    if can_run_multivariate(project).ok {
        let comp = crate::core::multivariate::composition_matrix(project, true, &Default::default(), "none");
        let bc = crate::core::multivariate::bray_curtis_matrix(&comp.matrix);
        let pcoa_result = crate::core::multivariate::pcoa(&bc, 2);
        if let Some(p) = plot_ordination(&pcoa_result, &comp.sample_names, output_dir.join("06_ordination.png")) {
            paths.push(p);
        }
        let link_result = crate::core::multivariate::hierarchical_clusters(&bc, "average");
        if let Some(p) = plot_dendrogram(&link_result, &comp.sample_names, output_dir.join("07_dendrogram.png")) {
            paths.push(p);
        }
    }

    Ok(paths)
}
