//! Excel/CSV/JSON report export for coralX projects.
//!
//! Chart generation + embedding (the Python version's `plots.py` integration)
//! is not yet ported — `export_excel` writes every data sheet but skips the
//! "Charts" sheet for now, the same graceful fallback the Python code takes
//! when its plotting dependencies are unavailable.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use anyhow::{Context, Result};
use rust_xlsxwriter::{Workbook, Worksheet};

use crate::core::analysis::{cover_area_per_code, photo_area};
use crate::core::multivariate::{bray_curtis_matrix, composition_matrix, pcoa, permanova, simper};
use crate::core::statistics::{per_image_table, per_station_table, project_summary, station_summary, Summary};
use crate::core::table::{column_union, Cell, Row};
use crate::core::validation::{can_run_multivariate, validate_metadata_completeness};
use crate::models::Project;

/// Export the per-image coverage table to CSV (includes a station column).
pub fn export_csv(project: &Project, output_path: impl AsRef<Path>) -> Result<()> {
    let rows = per_image_table(project);
    write_csv(&rows, output_path)
}

fn write_csv(rows: &[Row], output_path: impl AsRef<Path>) -> Result<()> {
    let columns = column_union(rows);
    let mut writer = csv::Writer::from_path(output_path)?;
    writer.write_record(&columns)?;
    for row in rows {
        let lookup: HashMap<&str, &Cell> = row.iter().map(|(k, v)| (k.as_str(), v)).collect();
        let record: Vec<String> = columns
            .iter()
            .map(|c| match lookup.get(c.as_str()) {
                Some(Cell::None) | None => "0".to_string(),
                Some(v) => v.to_string(),
            })
            .collect();
        writer.write_record(&record)?;
    }
    writer.flush()?;
    Ok(())
}

/// Write every export sheet (Summary, Group Coverage, Per Station, Per Image,
/// Statistics, Cover Area, Raw Points, multivariate, Map Data). Progress is
/// reported through `progress_cb(done, total, message)`.
pub fn export_excel(project: &Project, output_path: impl AsRef<Path>, mut progress_cb: impl FnMut(u32, u32, &str)) -> Result<()> {
    const TOTAL: u32 = 11;
    let mut step = 0u32;

    progress_cb(step, TOTAL, "Computing statistics...");
    let summary = project_summary(project);
    let per_station = per_station_table(project);
    let per_image = per_image_table(project);
    step += 1;
    progress_cb(step, TOTAL, "Preparing data...");

    let raw_rows = build_raw_points_rows(project);

    let mut workbook = Workbook::new();

    step += 1;
    progress_cb(step, TOTAL, "Writing Summary sheet...");
    write_summary_sheet(&mut workbook, &summary)?;

    step += 1;
    progress_cb(step, TOTAL, "Writing Group Coverage sheet...");
    write_group_coverage_sheet(&mut workbook, project, summary.as_ref())?;

    step += 1;
    progress_cb(step, TOTAL, "Writing Per Station sheet...");
    write_table_sheet(&mut workbook, "Per Station", &per_station, true)?;

    step += 1;
    progress_cb(step, TOTAL, &format!("Writing Per Image sheet ({} images)...", per_image.len()));
    write_table_sheet(&mut workbook, "Per Image", &per_image, true)?;

    step += 1;
    progress_cb(step, TOTAL, "Writing Statistics sheet...");
    write_table_sheet(&mut workbook, "Statistics", &coverage_statistics(project), false)?;

    let cover_rows = build_cover_area_rows(project);
    step += 1;
    if !cover_rows.is_empty() {
        progress_cb(step, TOTAL, "Writing Cover Area sheet...");
        write_table_sheet(&mut workbook, "Cover Area", &cover_rows, true)?;
    }

    step += 1;
    progress_cb(step, TOTAL, &format!("Writing Raw Points sheet ({} points)...", raw_rows.len()));
    write_table_sheet(&mut workbook, "Raw Points", &raw_rows, false)?;

    step += 1;
    progress_cb(step, TOTAL, "Computing multivariate analysis...");
    write_multivariate_sheets(&mut workbook, project)?;

    step += 1;
    progress_cb(step, TOTAL, "Writing Map Data sheet...");
    write_map_data_sheet(&mut workbook, project)?;

    workbook.save(&output_path)?;
    progress_cb(TOTAL, TOTAL, "Done.");
    Ok(())
}

fn sheet_name(name: &str) -> String {
    // rust_xlsxwriter enforces the same 31-char Excel sheet-name limit.
    name.chars().take(31).collect()
}

fn write_row(sheet: &mut Worksheet, row_num: u32, columns: &[String], row: &Row, fill_zero: bool) -> Result<()> {
    let lookup: HashMap<&str, &Cell> = row.iter().map(|(k, v)| (k.as_str(), v)).collect();
    for (col, name) in columns.iter().enumerate() {
        match lookup.get(name.as_str()) {
            Some(Cell::Str(s)) => {
                sheet.write_string(row_num, col as u16, s.as_str())?;
            }
            Some(Cell::Num(v)) => {
                sheet.write_number(row_num, col as u16, *v)?;
            }
            Some(Cell::Int(v)) => {
                sheet.write_number(row_num, col as u16, *v as f64)?;
            }
            Some(Cell::None) | None => {
                if fill_zero {
                    sheet.write_number(row_num, col as u16, 0.0)?;
                }
            }
        }
    }
    Ok(())
}

fn write_table_sheet(workbook: &mut Workbook, name: &str, rows: &[Row], fill_zero: bool) -> Result<()> {
    let sheet = workbook.add_worksheet();
    sheet.set_name(sheet_name(name))?;
    let columns = column_union(rows);
    for (col, name) in columns.iter().enumerate() {
        sheet.write_string(0, col as u16, name.as_str())?;
    }
    for (r, row) in rows.iter().enumerate() {
        write_row(sheet, r as u32 + 1, &columns, row, fill_zero)?;
    }
    Ok(())
}

fn metric_row(metric: &str, value: impl Into<Cell>) -> Row {
    vec![("Metric".into(), metric.into()), ("Value".into(), value.into())]
}

fn write_summary_sheet(workbook: &mut Workbook, summary: &Option<Summary>) -> Result<()> {
    let sheet = workbook.add_worksheet();
    sheet.set_name("Summary")?;

    let mut row_cursor = 0u32;

    let s1_rows: Vec<Row> = match summary {
        None => Vec::new(),
        Some(s) => vec![
            metric_row("Total points", s.total_points),
            metric_row("Labeled points", s.labeled_points),
            metric_row("", ""),
            metric_row("Species richness (S)", s.species_richness),
            metric_row("Shannon diversity (H')", s.shannon_diversity),
            metric_row("Simpson diversity (1-D)", s.simpson_diversity),
            metric_row("Pielou evenness (J')", s.pielou_evenness),
            metric_row("Margalef richness (d)", s.margalef_richness),
            metric_row("Fisher alpha (a)", s.fisher_alpha),
            metric_row("", ""),
            metric_row("Mortality Index (MI)", s.mortality_index),
            metric_row("Reef Health Category", s.reef_health.category),
            metric_row("Coral:Algae Ratio", s.coral_algae_ratio),
            metric_row("Berger-Parker Dominance (d)", s.berger_parker),
            metric_row("Hill q0 (richness)", s.hill.q0),
            metric_row("Hill q1 (exp H')", s.hill.q1),
            metric_row("Hill q2 (1/Simpson D)", s.hill.q2),
        ],
    };
    let cols1 = column_union(&s1_rows);
    for (c, name) in cols1.iter().enumerate() {
        sheet.write_string(row_cursor, c as u16, name.as_str())?;
    }
    for (r, row) in s1_rows.iter().enumerate() {
        write_row(sheet, row_cursor + r as u32 + 1, &cols1, row, false)?;
    }
    row_cursor += s1_rows.len() as u32 + 2;

    if let Some(s) = summary {
        if !s.coverage_ci.is_empty() {
            let cov_rows: Vec<Row> = s
                .coverage_ci
                .iter()
                .map(|(label, (pct, lo, hi))| {
                    vec![
                        ("Code".into(), label.clone().into()),
                        ("Coverage (%)".into(), (*pct).into()),
                        ("95% Confidence Interval Lower (%)".into(), (*lo).into()),
                        ("95% Confidence Interval Upper (%)".into(), (*hi).into()),
                    ]
                })
                .collect();
            let cols = column_union(&cov_rows);
            for (c, name) in cols.iter().enumerate() {
                sheet.write_string(row_cursor, c as u16, name.as_str())?;
            }
            for (r, row) in cov_rows.iter().enumerate() {
                write_row(sheet, row_cursor + r as u32 + 1, &cols, row, false)?;
            }
            row_cursor += cov_rows.len() as u32 + 2;
        }

        if !s.group_coverage.is_empty() {
            let grp_rows: Vec<Row> = s
                .group_coverage
                .iter()
                .map(|(grp, pct)| vec![("Group".into(), grp.clone().into()), ("Coverage (%)".into(), (*pct).into())])
                .collect();
            let cols = column_union(&grp_rows);
            for (c, name) in cols.iter().enumerate() {
                sheet.write_string(row_cursor, c as u16, name.as_str())?;
            }
            for (r, row) in grp_rows.iter().enumerate() {
                write_row(sheet, row_cursor + r as u32 + 1, &cols, row, false)?;
            }
        }
    }

    Ok(())
}

fn write_group_coverage_sheet(workbook: &mut Workbook, project: &Project, summary: Option<&Summary>) -> Result<()> {
    let mut rows: Vec<Row> = Vec::new();
    for station in &project.stations {
        if let Some(st_sum) = station_summary(station, &project.coral_groups) {
            let mut row: Row = vec![("station".into(), station.name.clone().into())];
            for (grp, pct) in &st_sum.group_coverage {
                row.push((grp.clone(), (*pct).into()));
            }
            rows.push(row);
        } else {
            rows.push(vec![("station".into(), station.name.clone().into())]);
        }
    }
    if let Some(s) = summary {
        if !s.group_coverage.is_empty() {
            let mut row: Row = vec![("station".into(), "PROJECT TOTAL".into())];
            for (grp, pct) in &s.group_coverage {
                row.push((grp.clone(), (*pct).into()));
            }
            rows.push(row);
        }
    }
    write_table_sheet(workbook, "Group Coverage", &rows, true)
}

fn build_raw_points_rows(project: &Project) -> Vec<Row> {
    let mut rows = Vec::new();
    for station in &project.stations {
        for ann in &station.annotations {
            for p in &ann.points {
                rows.push(vec![
                    ("station".into(), station.name.clone().into()),
                    ("image".into(), ann.image_path.clone().into()),
                    ("point_index".into(), p.index.into()),
                    ("x".into(), round2(p.x).into()),
                    ("y".into(), round2(p.y).into()),
                    ("label".into(), p.label.clone().unwrap_or_default().into()),
                    ("category".into(), p.category.clone().unwrap_or_default().into()),
                ]);
            }
        }
    }
    rows
}

fn build_cover_area_rows(project: &Project) -> Vec<Row> {
    let mut rows = Vec::new();
    for station in &project.stations {
        for ann in &station.annotations {
            let Some(p_area) = photo_area(ann) else { continue };
            let c_area = cover_area_per_code(ann).unwrap_or_default();
            let mut row: Row = vec![
                ("station".into(), station.name.clone().into()),
                ("image".into(), ann.image_path.clone().into()),
                (format!("photo_area_{}2", ann.scale_unit), p_area.into()),
                ("scale_factor_px_per_unit".into(), ann.scale_factor.into()),
                ("scale_unit".into(), ann.scale_unit.clone().into()),
            ];
            for (code, area) in c_area {
                row.push((format!("{code}_{}2", ann.scale_unit), area.into()));
            }
            rows.push(row);
        }
    }
    rows
}

/// Per-code mean, std dev, and std error across all images in the project.
fn coverage_statistics(project: &Project) -> Vec<Row> {
    let annotations = project.annotations();
    if annotations.is_empty() {
        return Vec::new();
    }
    let per_image: Vec<HashMap<String, f64>> = annotations.iter().map(|a| a.coverage_stats()).collect();
    let all_codes: BTreeSet<String> = per_image.iter().flat_map(|m| m.keys().cloned()).collect();
    let n = per_image.len() as f64;

    all_codes
        .into_iter()
        .map(|code| {
            let values: Vec<f64> = per_image.iter().map(|m| m.get(&code).copied().unwrap_or(0.0)).collect();
            let mean = values.iter().sum::<f64>() / n;
            let std = if per_image.len() > 1 {
                let var = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (n - 1.0);
                var.sqrt()
            } else {
                0.0
            };
            let err = if n > 0.0 { std / n.sqrt() } else { 0.0 };
            vec![
                ("Code".into(), code.into()),
                ("Mean (%)".into(), round4(mean).into()),
                ("Std Dev (%)".into(), round4(std).into()),
                ("Std Error (%)".into(), round4(err).into()),
            ]
        })
        .collect()
}

fn write_multivariate_sheets(workbook: &mut Workbook, project: &Project) -> Result<()> {
    let gate = can_run_multivariate(project);
    if !gate.ok {
        let rows: Vec<Row> = gate.reasons.iter().map(|r| vec![("Reason".into(), r.clone().into())]).collect();
        return write_table_sheet(workbook, "Multivariate", &rows, false);
    }

    let comp = composition_matrix(project, true, &Default::default(), "none");
    let bc = bray_curtis_matrix(&comp.matrix);

    // Bray-Curtis dissimilarity matrix
    {
        let sheet = workbook.add_worksheet();
        sheet.set_name("Bray-Curtis")?;
        sheet.write_string(0, 0, "")?;
        for (c, name) in comp.sample_names.iter().enumerate() {
            sheet.write_string(0, c as u16 + 1, name.as_str())?;
        }
        for (r, name) in comp.sample_names.iter().enumerate() {
            sheet.write_string(r as u32 + 1, 0, name.as_str())?;
            for c in 0..comp.sample_names.len() {
                sheet.write_number(r as u32 + 1, c as u16 + 1, round4(bc[(r, c)]))?;
            }
        }
    }

    // PCoA ordination
    let pcoa_result = pcoa(&bc, 2.max(comp.sample_names.len().saturating_sub(1)).min(comp.sample_names.len()));
    let n_axes = pcoa_result.coords.ncols();
    let mut ord_rows: Vec<Row> = Vec::new();
    for (i, name) in comp.sample_names.iter().enumerate() {
        let mut row: Row = vec![("station".into(), name.clone().into())];
        for ax in 0..n_axes {
            row.push((format!("PCoA{}", ax + 1), round6(pcoa_result.coords[(i, ax)]).into()));
        }
        ord_rows.push(row);
    }
    let mut var_row: Row = vec![("station".into(), "Variance explained".into())];
    for ax in 0..n_axes {
        var_row.push((format!("PCoA{}", ax + 1), pcoa_result.variance_explained.get(ax).copied().unwrap_or(0.0).into()));
    }
    ord_rows.push(var_row);
    write_table_sheet(workbook, "Ordination", &ord_rows, false)?;

    // PERMANOVA — station names used directly as group labels.
    match permanova(&bc, &comp.sample_names, 999, 42) {
        Err(msg) => write_table_sheet(workbook, "PERMANOVA", &[vec![("Note".into(), msg.into())]], false)?,
        Ok(r) => write_table_sheet(
            workbook,
            "PERMANOVA",
            &[
                metric_row("pseudo-F", r.pseudo_f),
                metric_row("p-value", r.p_value),
                metric_row("permutations", r.permutations as i64),
                metric_row("significant (p<0.05)", r.significant.to_string()),
            ],
            false,
        )?,
    };

    // SIMPER — first pair of stations as example.
    if comp.sample_names.len() >= 2 {
        let simper_rows = simper(&comp.matrix, &comp.code_names, &comp.sample_names, &comp.sample_names[0], &comp.sample_names[1]);
        if !simper_rows.is_empty() {
            let rows: Vec<Row> = simper_rows
                .into_iter()
                .map(|r| {
                    vec![
                        ("code".into(), r.code.into()),
                        ("avg_contribution".into(), r.avg_contribution.into()),
                        ("pct_contribution".into(), r.pct_contribution.into()),
                        ("cumulative_pct".into(), r.cumulative_pct.into()),
                    ]
                })
                .collect();
            write_table_sheet(workbook, "SIMPER", &rows, false)?;
        }
    }

    Ok(())
}

fn write_map_data_sheet(workbook: &mut Workbook, project: &Project) -> Result<()> {
    let meta = validate_metadata_completeness(project);
    if !meta["spatial"].ok {
        return Ok(());
    }

    let mut rows: Vec<Row> = Vec::new();
    for st in &project.stations {
        let (Some(lat), Some(lon)) = (st.gps_lat, st.gps_lon) else { continue };
        if lat == 0.0 || lon == 0.0 {
            continue;
        }
        let Some(summ) = station_summary(st, &project.coral_groups) else { continue };
        rows.push(vec![
            ("station".into(), st.name.clone().into()),
            ("lat".into(), lat.into()),
            ("lon".into(), lon.into()),
            ("live_coral_pct".into(), summ.group_coverage.get("Hard Coral").copied().into()),
            ("mortality_index".into(), summ.mortality_index.into()),
            ("reef_health".into(), summ.reef_health.category.into()),
        ]);
    }
    if rows.is_empty() {
        return Ok(());
    }
    write_table_sheet(workbook, "Map Data", &rows, false)
}

/// Export project coral codes to JSON or CSV/TSV.
pub fn export_coral_codes(project: &Project, output_path: impl AsRef<Path>) -> Result<()> {
    let output_path = output_path.as_ref();
    let ext = output_path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();

    match ext.as_str() {
        "json" => {
            let data = serde_json::json!({
                "codes": project.coral_codes,
                "groups": project.coral_groups,
            });
            std::fs::write(output_path, serde_json::to_string_pretty(&data)?)?;
        }
        "csv" | "tsv" => {
            let mut code_to_group: HashMap<&str, &str> = HashMap::new();
            let mut code_to_color: HashMap<&str, &str> = HashMap::new();
            for g in &project.coral_groups {
                for c in &g.codes {
                    code_to_group.insert(c.as_str(), g.name.as_str());
                    code_to_color.insert(c.as_str(), g.color.as_deref().unwrap_or(""));
                }
            }
            let delim = if ext == "tsv" { b'\t' } else { b',' };
            let mut writer = csv::WriterBuilder::new().delimiter(delim).from_path(output_path)?;
            writer.write_record(["code", "description", "group", "color"])?;
            for (code, desc) in &project.coral_codes {
                writer.write_record([
                    code.as_str(),
                    desc.as_str(),
                    code_to_group.get(code.as_str()).copied().unwrap_or(""),
                    code_to_color.get(code.as_str()).copied().unwrap_or(""),
                ])?;
            }
            writer.flush()?;
        }
        other => anyhow::bail!("Unsupported format: .{other}. Use .json, .csv, or .tsv"),
    }
    Ok(())
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}
fn round4(v: f64) -> f64 {
    (v * 10000.0).round() / 10000.0
}
fn round6(v: f64) -> f64 {
    (v * 1_000_000.0).round() / 1_000_000.0
}
