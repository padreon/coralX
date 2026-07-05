//! Validation guards for advanced (Lapis 3) analyses.
//!
//! Every function returns a `ValidationResult` (never panics) so callers can
//! display human-readable reasons when prerequisites are not met.

use std::collections::{HashMap, HashSet};

use time::Date;

use crate::models::Project;

#[derive(Debug, Clone, Default)]
pub struct ValidationResult {
    pub ok: bool,
    pub reasons: Vec<String>,
    pub warnings: Vec<String>,
}

fn parse_iso_date(s: &str) -> Option<Date> {
    let fmt = time::macros::format_description!("[year]-[month]-[day]");
    Date::parse(s, &fmt).ok()
}

/// Check per-analysis metadata prerequisites.
///
/// Keys returned: "temporal", "spatial", "area", "depth". Each value explains
/// which stations are missing data. Never panics even when all metadata is absent.
pub fn validate_metadata_completeness(project: &Project) -> HashMap<String, ValidationResult> {
    let stations = &project.stations;

    // --- temporal: >=2 stations with distinct valid ISO-8601 dates ---
    let mut valid_dates: HashSet<String> = HashSet::new();
    let mut missing_date: Vec<String> = Vec::new();
    for st in stations {
        match &st.date {
            Some(d) if !d.is_empty() => {
                if parse_iso_date(d).is_some() {
                    valid_dates.insert(d.clone());
                } else {
                    missing_date.push(format!("{} (invalid date: {d:?})", st.name));
                }
            }
            _ => missing_date.push(format!("{} (no date)", st.name)),
        }
    }
    let temporal_ok = valid_dates.len() >= 2;
    let mut temporal_reasons = Vec::new();
    if !temporal_ok {
        if !missing_date.is_empty() {
            temporal_reasons.push(format!("Missing/invalid date on: {}", missing_date.join(", ")));
        }
        temporal_reasons.push(format!(
            "Need >=2 stations with distinct dates; found {} unique date(s).",
            valid_dates.len()
        ));
    }

    // --- spatial: >=3 stations with valid GPS lat/lon (not None, not 0) ---
    let mut missing_gps: Vec<String> = Vec::new();
    let mut valid_gps_count = 0;
    for st in stations {
        match (st.gps_lat, st.gps_lon) {
            (Some(lat), Some(lon)) if lat != 0.0 && lon != 0.0 => valid_gps_count += 1,
            _ => missing_gps.push(st.name.clone()),
        }
    }
    let spatial_ok = valid_gps_count >= 3;
    let mut spatial_reasons = Vec::new();
    if !spatial_ok {
        if !missing_gps.is_empty() {
            spatial_reasons.push(format!("Missing GPS on: {}", missing_gps.join(", ")));
        }
        spatial_reasons.push(format!("Need >=3 stations with GPS; found {valid_gps_count}."));
    }

    // --- area: >=1 annotation with scale_factor calibrated (!=1.0 and !=0) ---
    let calibrated = project
        .annotations()
        .iter()
        .filter(|a| a.scale_factor != 0.0 && a.scale_factor != 1.0)
        .count();
    let area_ok = calibrated >= 1;
    let area_reasons = if area_ok {
        Vec::new()
    } else {
        vec!["No calibrated images found (scale_factor == 1.0 or 0 for all images).".to_string()]
    };

    // --- depth: >=3 stations with depth_m > 0 ---
    let mut missing_depth: Vec<String> = Vec::new();
    let mut valid_depth_count = 0;
    for st in stations {
        match st.depth_m {
            Some(dm) if dm > 0.0 => valid_depth_count += 1,
            _ => missing_depth.push(st.name.clone()),
        }
    }
    let depth_ok = valid_depth_count >= 3;
    let mut depth_reasons = Vec::new();
    if !depth_ok {
        if !missing_depth.is_empty() {
            depth_reasons.push(format!("Missing depth on: {}", missing_depth.join(", ")));
        }
        depth_reasons.push(format!("Need >=3 stations with depth_m > 0; found {valid_depth_count}."));
    }

    HashMap::from([
        (
            "temporal".to_string(),
            ValidationResult { ok: temporal_ok, reasons: temporal_reasons, warnings: Vec::new() },
        ),
        (
            "spatial".to_string(),
            ValidationResult { ok: spatial_ok, reasons: spatial_reasons, warnings: Vec::new() },
        ),
        (
            "area".to_string(),
            ValidationResult { ok: area_ok, reasons: area_reasons, warnings: Vec::new() },
        ),
        (
            "depth".to_string(),
            ValidationResult { ok: depth_ok, reasons: depth_reasons, warnings: Vec::new() },
        ),
    ])
}

/// Check that all images use the same point distribution and similar point counts.
///
/// `ok == true` when all labeled images use the same `point_distribution` and the
/// max/min labeled-point-count ratio across images is <= 2.0. Warnings are added
/// for images with fewer than 25 labeled points. Never panics.
pub fn validate_sampling_consistency(project: &Project) -> ValidationResult {
    let mut reasons: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    let mut labeled_counts: Vec<usize> = Vec::new();
    for st in &project.stations {
        for ann in &st.annotations {
            let labeled = ann.labeled_count();
            labeled_counts.push(labeled);
            if labeled > 0 && labeled < 25 {
                warnings.push(format!(
                    "{}: only {labeled} labeled points (< 25, less reliable).",
                    ann.image_path
                ));
            }
        }
    }

    let nonempty: Vec<usize> = labeled_counts.into_iter().filter(|&c| c > 0).collect();
    if nonempty.len() >= 2 {
        let max = *nonempty.iter().max().unwrap() as f64;
        let min = *nonempty.iter().min().unwrap() as f64;
        let ratio = max / min;
        if ratio > 2.0 {
            reasons.push(format!(
                "Labeled point counts vary too much across images (max/min ratio = {ratio:.1}, threshold = 2.0). Proportional comparisons may be unreliable."
            ));
        }
    }

    // point_distribution consistency: since all images share the project-level
    // setting, the only source of inconsistency is a missing project value.
    if project.point_distribution.is_empty() {
        reasons.push("point_distribution not set on project.".to_string());
    }

    ValidationResult { ok: reasons.is_empty(), reasons, warnings }
}

/// Gate for multivariate analyses (Bray-Curtis, nMDS, PERMANOVA, SIMPER).
///
/// `ok == true` when the project has >= 4 stations AND
/// `validate_sampling_consistency` passes.
pub fn can_run_multivariate(project: &Project) -> ValidationResult {
    let mut reasons: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    let n_stations = project.stations.len();
    if n_stations < 4 {
        reasons.push(format!(
            "Need at least 4 stations for multivariate analysis; project has {n_stations}."
        ));
    }

    let consistency = validate_sampling_consistency(project);
    if !consistency.ok {
        reasons.extend(consistency.reasons);
    }
    warnings.extend(consistency.warnings);

    ValidationResult { ok: reasons.is_empty(), reasons, warnings }
}
