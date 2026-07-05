use std::collections::HashMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
    pub index: i64,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
}

/// "line" | "polyline" | "polygon"
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Measurement {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    /// fragment name, e.g. "Frag-01"
    pub label: String,
    /// image-space pixel coords
    pub points: Vec<(f64, f64)>,
    /// length (unit) or area (unit^2)
    pub value: f64,
    /// "cm" or "m"
    pub unit: String,
    #[serde(default)]
    pub species: String,
    /// oriented/bbox width in real units
    #[serde(default)]
    pub auto_width: f64,
    /// oriented/bbox height in real units
    #[serde(default)]
    pub auto_height: f64,
    /// polygon area in real units^2
    #[serde(default)]
    pub area: f64,
    /// polygon perimeter in real units
    #[serde(default)]
    pub perimeter_len: f64,
    /// tilt of the height axis, degrees from horizontal
    #[serde(default)]
    pub angle: f64,
}

impl Measurement {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        kind: impl Into<String>,
        label: impl Into<String>,
        points: Vec<(f64, f64)>,
        value: f64,
        unit: impl Into<String>,
        species: impl Into<String>,
        auto_width: f64,
        auto_height: f64,
        area: f64,
        perimeter_len: f64,
        angle: f64,
    ) -> Self {
        Measurement {
            id: Uuid::new_v4().to_string(),
            kind: kind.into(),
            label: label.into(),
            points,
            value,
            unit: unit.into(),
            species: species.into(),
            auto_width,
            auto_height,
            area,
            perimeter_len,
            angle,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageAnnotation {
    pub image_path: String,
    #[serde(default)]
    pub points: Vec<Point>,
    #[serde(default)]
    pub image_width: i64,
    #[serde(default)]
    pub image_height: i64,
    /// pixels per scale_unit; 1.0 = not calibrated
    #[serde(default = "default_scale_factor")]
    pub scale_factor: f64,
    /// "cm" or "m"
    #[serde(default = "default_scale_unit")]
    pub scale_unit: String,
    #[serde(default)]
    pub measurements: Vec<Measurement>,
}

fn default_scale_factor() -> f64 {
    1.0
}

fn default_scale_unit() -> String {
    "cm".to_string()
}

impl ImageAnnotation {
    pub fn new(image_path: impl Into<String>) -> Self {
        ImageAnnotation {
            image_path: image_path.into(),
            points: Vec::new(),
            image_width: 0,
            image_height: 0,
            scale_factor: 1.0,
            scale_unit: "cm".to_string(),
            measurements: Vec::new(),
        }
    }

    pub fn labeled_count(&self) -> usize {
        self.points.iter().filter(|p| p.label.is_some()).count()
    }

    pub fn is_complete(&self) -> bool {
        !self.points.is_empty() && self.labeled_count() == self.points.len()
    }

    /// Percentage coverage per label, rounded to 2 decimal places.
    pub fn coverage_stats(&self) -> HashMap<String, f64> {
        let labeled: Vec<&Point> = self.points.iter().filter(|p| p.label.is_some()).collect();
        if labeled.is_empty() {
            return HashMap::new();
        }
        let total = labeled.len() as f64;
        let mut counts: HashMap<String, u32> = HashMap::new();
        for p in &labeled {
            let key = p.label.clone().unwrap_or_default();
            *counts.entry(key).or_insert(0) += 1;
        }
        counts
            .into_iter()
            .map(|(k, v)| (k, (v as f64 / total * 100.0 * 100.0).round() / 100.0))
            .collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Station {
    pub name: String,
    #[serde(default)]
    pub depth_m: Option<f64>,
    /// ISO-8601: "2024-03-15"
    #[serde(default)]
    pub date: Option<String>,
    #[serde(default)]
    pub gps_lat: Option<f64>,
    #[serde(default)]
    pub gps_lon: Option<f64>,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub annotations: Vec<ImageAnnotation>,
}

impl Station {
    pub fn new(name: impl Into<String>) -> Self {
        Station {
            name: name.into(),
            depth_m: None,
            date: None,
            gps_lat: None,
            gps_lon: None,
            notes: String::new(),
            annotations: Vec::new(),
        }
    }

    pub fn total_points(&self) -> usize {
        self.annotations.iter().map(|a| a.points.len()).sum()
    }

    pub fn labeled_points(&self) -> usize {
        self.annotations.iter().map(|a| a.labeled_count()).sum()
    }

    pub fn is_complete(&self) -> bool {
        !self.annotations.is_empty() && self.annotations.iter().all(|a| a.is_complete())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoralGroup {
    pub name: String,
    pub codes: Vec<String>,
    /// 6-character hex color (no '#'), when known from a CPCe code-file import.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub name: String,
    #[serde(default = "default_point_count")]
    pub point_count: i64,
    /// random | stratified | uniform
    #[serde(default = "default_distribution")]
    pub point_distribution: String,
    /// uniform pixel border to exclude
    #[serde(default)]
    pub border_exclusion: i64,
    /// [x_min, y_min, x_max, y_max] if set by click
    #[serde(default)]
    pub border_rect: Option<Vec<f64>>,
    /// [[x, y], ...] if set by polygon drawing
    #[serde(default)]
    pub border_polygon: Option<Vec<Vec<f64>>>,
    #[serde(default)]
    pub coral_codes: HashMap<String, String>,
    #[serde(default)]
    pub coral_groups: Vec<CoralGroup>,
    /// known species/genus names
    #[serde(default)]
    pub species_list: Vec<String>,
    #[serde(default)]
    pub stations: Vec<Station>,
    #[serde(skip)]
    pub save_path: Option<String>,
}

fn default_point_count() -> i64 {
    10
}

fn default_distribution() -> String {
    "random".to_string()
}

/// Old flat save format, auto-migrated into a single "Station 1" on load.
#[derive(Debug, Deserialize)]
struct LegacyProjectFile {
    annotations: Vec<ImageAnnotation>,
}

impl Project {
    pub fn new(name: impl Into<String>) -> Self {
        Project {
            name: name.into(),
            point_count: 10,
            point_distribution: "random".to_string(),
            border_exclusion: 0,
            border_rect: None,
            border_polygon: None,
            coral_codes: HashMap::new(),
            coral_groups: Vec::new(),
            species_list: Vec::new(),
            stations: Vec::new(),
            save_path: None,
        }
    }

    /// Flat view across all stations — for statistics and export.
    pub fn annotations(&self) -> Vec<&ImageAnnotation> {
        self.stations.iter().flat_map(|s| &s.annotations).collect()
    }

    pub fn save(&mut self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        let json = serde_json::to_string_pretty(self).context("serializing project")?;
        fs::write(path, json).with_context(|| format!("writing {}", path.display()))?;
        self.save_path = Some(path.to_string_lossy().into_owned());
        Ok(())
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let value: serde_json::Value =
            serde_json::from_str(&text).context("parsing project JSON")?;

        let mut project: Project = if value.get("stations").is_some() {
            serde_json::from_value(value).context("deserializing project")?
        } else {
            // Old flat format — auto-migrate to a single station.
            let legacy: LegacyProjectFile =
                serde_json::from_value(value.clone()).context("deserializing legacy project")?;
            let mut project: Project = serde_json::from_value(value).context("deserializing project header")?;
            project.stations = vec![Station {
                name: "Station 1".to_string(),
                depth_m: None,
                date: None,
                gps_lat: None,
                gps_lon: None,
                notes: String::new(),
                annotations: legacy.annotations,
            }];
            project
        };

        project.save_path = Some(path.to_string_lossy().into_owned());
        Ok(project)
    }
}
