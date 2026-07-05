use std::collections::HashMap;

use crate::core::analysis::{
    berger_parker_dominance, coral_algae_ratio, coverage_with_ci, cover_area_per_code,
    fisher_alpha, group_coverage, hill_numbers, margalef_richness, mortality_index, photo_area,
    pielou_evenness, reef_health_category, species_richness, HillNumbers, ReefHealth,
};
use crate::core::table::{Cell, Row};
use crate::models::{CoralGroup, ImageAnnotation, Project, Station};

/// Percentage coverage per label for a single image.
pub fn image_coverage(annotation: &ImageAnnotation) -> HashMap<String, f64> {
    annotation.coverage_stats()
}

#[derive(Debug, Clone)]
pub struct Summary {
    pub coverage: HashMap<String, f64>,
    pub coverage_ci: HashMap<String, (f64, f64, f64)>,
    pub group_coverage: HashMap<String, f64>,
    pub shannon_diversity: f64,
    pub simpson_diversity: f64,
    pub species_richness: usize,
    pub pielou_evenness: f64,
    pub margalef_richness: f64,
    pub fisher_alpha: f64,
    pub total_points: usize,
    pub labeled_points: usize,
    pub mortality_index: Option<f64>,
    pub reef_health: ReefHealth,
    pub coral_algae_ratio: Option<f64>,
    pub berger_parker: f64,
    pub hill: HillNumbers,
}

/// Aggregate stats across all images in the project.
pub fn project_summary(project: &Project) -> Option<Summary> {
    let annotations: Vec<&ImageAnnotation> = project.annotations();
    summary_from_annotations(&annotations, &project.coral_groups)
}

/// Aggregate stats scoped to a single station.
pub fn station_summary(station: &Station, coral_groups: &[CoralGroup]) -> Option<Summary> {
    let annotations: Vec<&ImageAnnotation> = station.annotations.iter().collect();
    summary_from_annotations(&annotations, coral_groups)
}

fn summary_from_annotations(
    annotations: &[&ImageAnnotation],
    coral_groups: &[CoralGroup],
) -> Option<Summary> {
    let all_labels: Vec<String> =
        annotations.iter().flat_map(|a| a.points.iter().filter_map(|p| p.label.clone())).collect();

    if all_labels.is_empty() {
        return None;
    }

    let total = all_labels.len();
    let mut counts: HashMap<String, u32> = HashMap::new();
    for label in &all_labels {
        *counts.entry(label.clone()).or_insert(0) += 1;
    }

    let coverage: HashMap<String, f64> =
        counts.iter().map(|(k, &v)| (k.clone(), round2(v as f64 / total as f64 * 100.0))).collect();
    let counts_vals: Vec<u32> = counts.values().copied().collect();
    let h = shannon_index(&counts_vals, total);
    let simpson = simpson_index(&counts_vals, total);
    let s = species_richness(&all_labels);
    let j = pielou_evenness(h, s);
    let d = margalef_richness(s, total);
    let alpha = fisher_alpha(s, total);

    let total_points: usize = annotations.iter().map(|a| a.points.len()).sum();
    let labeled_points: usize = annotations.iter().map(|a| a.labeled_count()).sum();

    let ci_data = coverage_with_ci(&all_labels, 0.95);
    let grp_cov = group_coverage(&all_labels, coral_groups);

    let live_coral_pct = grp_cov.get("Hard Coral").copied().unwrap_or(0.0);
    let mi = mortality_index(&all_labels, coral_groups);
    let health = reef_health_category(live_coral_pct);
    let car = coral_algae_ratio(&all_labels, coral_groups);
    let bp = berger_parker_dominance(&all_labels);
    let hill = hill_numbers(&all_labels);

    Some(Summary {
        coverage,
        coverage_ci: ci_data,
        group_coverage: grp_cov,
        shannon_diversity: round4(h),
        simpson_diversity: round4(simpson),
        species_richness: s,
        pielou_evenness: round4(j),
        margalef_richness: round4(d),
        fisher_alpha: round4(alpha),
        total_points,
        labeled_points,
        mortality_index: mi,
        reef_health: health,
        coral_algae_ratio: car,
        berger_parker: bp,
        hill,
    })
}

/// Per-image stats for table display / export, with station column and CI columns.
pub fn per_image_table(project: &Project) -> Vec<Row> {
    let mut rows = Vec::new();
    for station in &project.stations {
        for ann in &station.annotations {
            let stats = ann.coverage_stats();
            let labels: Vec<String> = ann.points.iter().filter_map(|p| p.label.clone()).collect();
            let ci_data = coverage_with_ci(&labels, 0.95);
            let p_area = photo_area(ann);
            let c_area = cover_area_per_code(ann);

            let mut row: Row = vec![
                ("station".into(), station.name.clone().into()),
                ("image".into(), ann.image_path.clone().into()),
                ("total_points".into(), ann.points.len().into()),
                ("labeled_points".into(), ann.labeled_count().into()),
            ];

            if let Some(area) = p_area {
                row.push((format!("photo_area_{}2", ann.scale_unit), area.into()));
            }

            for (code, pct) in &stats {
                row.push((code.clone(), (*pct).into()));
            }

            for (code, (_, ci_lower, ci_upper)) in &ci_data {
                row.push((format!("{code}_ci_lower"), (*ci_lower).into()));
                row.push((format!("{code}_ci_upper"), (*ci_upper).into()));
            }

            if let Some(c_area) = c_area {
                for (code, area) in c_area {
                    row.push((format!("{code}_area_{}2", ann.scale_unit), area.into()));
                }
            }

            rows.push(row);
        }
    }
    rows
}

/// One row per station with aggregate coverage, diversity, and metadata.
pub fn per_station_table(project: &Project) -> Vec<Row> {
    let mut rows = Vec::new();
    for station in &project.stations {
        let mut counts: HashMap<String, u32> = HashMap::new();
        let mut labels: Vec<String> = Vec::new();
        let mut total_photo_area = 0.0;
        let mut has_area = false;

        for ann in &station.annotations {
            for p in &ann.points {
                if let Some(label) = &p.label {
                    *counts.entry(label.clone()).or_insert(0) += 1;
                    labels.push(label.clone());
                }
            }
            if let Some(p_area) = photo_area(ann) {
                total_photo_area += p_area;
                has_area = true;
            }
        }

        let total_labeled: u32 = counts.values().sum();
        let n = total_labeled as usize;
        let s = species_richness(&labels);
        let counts_vals: Vec<u32> = counts.values().copied().collect();
        let h = if n > 0 { shannon_index(&counts_vals, n) } else { 0.0 };
        let simpson = if n > 0 { simpson_index(&counts_vals, n) } else { 0.0 };

        let mut row: Row = vec![
            ("station".into(), station.name.clone().into()),
            ("depth_m".into(), station.depth_m.into()),
            ("date".into(), station.date.clone().into()),
            ("gps_lat".into(), station.gps_lat.into()),
            ("gps_lon".into(), station.gps_lon.into()),
            ("total_points".into(), station.total_points().into()),
            ("labeled_points".into(), station.labeled_points().into()),
            ("species_richness".into(), s.into()),
            ("shannon_H".into(), round4(h).into()),
            ("simpson_1D".into(), round4(simpson).into()),
            ("pielou_J".into(), round4(pielou_evenness(h, s)).into()),
            ("margalef_d".into(), round4(margalef_richness(s, n)).into()),
        ];

        if has_area {
            let unit =
                station.annotations.first().map(|a| a.scale_unit.clone()).unwrap_or_else(|| "cm".to_string());
            row.push((format!("total_photo_area_{unit}2"), round4(total_photo_area).into()));
        }

        if total_labeled > 0 {
            for (k, v) in &counts {
                row.push((k.clone(), round2(*v as f64 / total_labeled as f64 * 100.0).into()));
            }
        }

        let grp_cov = group_coverage(&labels, &project.coral_groups);
        for (grp, pct) in &grp_cov {
            row.push((format!("group_{grp}"), (*pct).into()));
        }

        let live_coral_pct = grp_cov.get("Hard Coral").copied().unwrap_or(0.0);
        let mi = mortality_index(&labels, &project.coral_groups);
        let health = reef_health_category(live_coral_pct);
        row.push(("mortality_index".into(), mi.into()));
        row.push(("reef_health_category".into(), Cell::Str(health.category.to_string())));
        row.push(("coral_algae_ratio".into(), coral_algae_ratio(&labels, &project.coral_groups).into()));
        row.push(("berger_parker".into(), berger_parker_dominance(&labels).into()));
        let hill = hill_numbers(&labels);
        row.push(("hill_q0".into(), hill.q0.into()));
        row.push(("hill_q1".into(), hill.q1.into()));
        row.push(("hill_q2".into(), hill.q2.into()));

        rows.push(row);
    }
    rows
}

/// Shannon-Weaver diversity index H'.
fn shannon_index(counts: &[u32], total: usize) -> f64 {
    let mut h = 0.0;
    for &c in counts {
        if c > 0 {
            let p = c as f64 / total as f64;
            h -= p * p.ln();
        }
    }
    h
}

/// Simpson's diversity index (1 - D).
fn simpson_index(counts: &[u32], total: usize) -> f64 {
    let d: i64 = counts.iter().map(|&c| c as i64 * (c as i64 - 1)).sum();
    let n = total as i64 * (total as i64 - 1);
    if n > 0 {
        1.0 - (d as f64 / n as f64)
    } else {
        0.0
    }
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

fn round4(v: f64) -> f64 {
    (v * 10000.0).round() / 10000.0
}
