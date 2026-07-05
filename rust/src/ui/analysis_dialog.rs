//! Advanced analysis dialog (Lapis 3): pick which advanced analyses to run
//! and export only those to a custom Excel file.

use anyhow::Result;
use rust_xlsxwriter::Workbook;

use crate::core::comparison::{depth_gradient, temporal_trend};
use crate::core::exporter::{metric_row, write_table_sheet};
use crate::core::multivariate::{bray_curtis_matrix, composition_matrix, pcoa, permanova, simper};
use crate::core::statistics::station_summary;
use crate::core::table::Row;
use crate::core::validation::{can_run_multivariate, validate_metadata_completeness, validate_sampling_consistency};
use crate::models::Project;

pub struct AnalysisDialog {
    mv_ok: bool,
    mv_reason: String,
    temporal_ok: bool,
    temporal_reason: String,
    depth_ok: bool,
    depth_reason: String,
    spatial_ok: bool,
    spatial_reason: String,
    warnings: Vec<String>,

    bray: bool,
    pcoa: bool,
    permanova: bool,
    simper: bool,
    temporal: bool,
    depth: bool,
    spatial: bool,
    biotic_only: bool,
    transform: Transform,
}

#[derive(PartialEq, Clone, Copy)]
enum Transform {
    None,
    Sqrt,
    FourthRoot,
}

impl Transform {
    fn as_str(self) -> &'static str {
        match self {
            Transform::None => "none",
            Transform::Sqrt => "sqrt",
            Transform::FourthRoot => "fourth_root",
        }
    }
}

impl AnalysisDialog {
    pub fn new(project: &Project) -> Self {
        let meta = validate_metadata_completeness(project);
        let mv_gate = can_run_multivariate(project);
        let sampling = validate_sampling_consistency(project);

        AnalysisDialog {
            mv_ok: mv_gate.ok,
            mv_reason: mv_gate.reasons.join(" | "),
            temporal_ok: meta["temporal"].ok,
            temporal_reason: meta["temporal"].reasons.join(" | "),
            depth_ok: meta["depth"].ok,
            depth_reason: meta["depth"].reasons.join(" | "),
            spatial_ok: meta["spatial"].ok,
            spatial_reason: meta["spatial"].reasons.join(" | "),
            warnings: sampling.warnings,
            bray: false,
            pcoa: false,
            permanova: false,
            simper: false,
            temporal: false,
            depth: false,
            spatial: false,
            biotic_only: true,
            transform: Transform::None,
        }
    }

    /// Returns `Some(path)` once the user runs the export (already written to
    /// disk by the time this returns `Some`), or `None` while open/cancelled.
    pub fn show(&mut self, ctx: &egui::Context, project: &Project) -> Option<Result<std::path::PathBuf, String>> {
        let mut outcome = None;

        egui::Window::new("Analisa Lanjutan").collapsible(false).min_width(480.0).show(ctx, |ui| {
            ui.group(|ui| {
                ui.label("Multivariat (Bray-Curtis / PCoA / PERMANOVA / SIMPER)");
                for (label, checked) in [
                    ("Matriks Bray-Curtis", &mut self.bray),
                    ("Ordinasi PCoA", &mut self.pcoa),
                    ("PERMANOVA", &mut self.permanova),
                    ("SIMPER", &mut self.simper),
                ] {
                    let resp = ui.add_enabled(self.mv_ok, egui::Checkbox::new(checked, label));
                    if !self.mv_ok {
                        resp.on_hover_text(&self.mv_reason);
                    }
                }
                ui.add_enabled_ui(self.mv_ok, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Biotic only:");
                        egui::ComboBox::from_id_salt("biotic_only")
                            .selected_text(if self.biotic_only { "Ya (default)" } else { "Tidak" })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut self.biotic_only, true, "Ya (default)");
                                ui.selectable_value(&mut self.biotic_only, false, "Tidak");
                            });
                        ui.label("Transform:");
                        egui::ComboBox::from_id_salt("transform")
                            .selected_text(self.transform.as_str())
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut self.transform, Transform::None, "none");
                                ui.selectable_value(&mut self.transform, Transform::Sqrt, "sqrt");
                                ui.selectable_value(&mut self.transform, Transform::FourthRoot, "fourth_root");
                            });
                    });
                });
            });

            ui.group(|ui| {
                ui.label("Analisa Temporal (tren waktu)");
                let resp = ui.add_enabled(self.temporal_ok, egui::Checkbox::new(&mut self.temporal, "Hitung tren temporal per stasiun"));
                if !self.temporal_ok {
                    resp.on_hover_text(&self.temporal_reason);
                }
            });

            ui.group(|ui| {
                ui.label("Gradien Kedalaman");
                let resp = ui.add_enabled(self.depth_ok, egui::Checkbox::new(&mut self.depth, "Regresi metrik vs kedalaman (depth_m)"));
                if !self.depth_ok {
                    resp.on_hover_text(&self.depth_reason);
                }
            });

            ui.group(|ui| {
                ui.label("Data Peta (GPS + metrik)");
                let resp = ui.add_enabled(self.spatial_ok, egui::Checkbox::new(&mut self.spatial, "Export sheet Map Data (GIS-ready)"));
                if !self.spatial_ok {
                    resp.on_hover_text(&self.spatial_reason);
                }
            });

            if !self.warnings.is_empty() {
                ui.colored_label(egui::Color32::from_rgb(0xb8, 0x86, 0x0b), self.warnings.iter().map(|w| format!("\u{26A0} {w}")).collect::<Vec<_>>().join("\n"));
            }

            ui.separator();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Jalankan & Export...").clicked() {
                    if let Some(path) = rfd::FileDialog::new().add_filter("Excel", &["xlsx"]).set_file_name("advanced_analysis.xlsx").save_file() {
                        let opts = ExportOptions {
                            bray: self.bray,
                            pcoa: self.pcoa,
                            permanova: self.permanova,
                            simper: self.simper,
                            temporal: self.temporal,
                            depth: self.depth,
                            spatial: self.spatial,
                            biotic_only: self.biotic_only,
                            transform: self.transform.as_str(),
                        };
                        outcome = Some(run_advanced_export(project, &path, &opts).map(|()| path).map_err(|e| e.to_string()));
                    }
                }
                if ui.button("Cancel").clicked() {
                    outcome = Some(Err("cancelled".to_string()));
                }
            });
        });

        outcome
    }
}

struct ExportOptions<'a> {
    bray: bool,
    pcoa: bool,
    permanova: bool,
    simper: bool,
    temporal: bool,
    depth: bool,
    spatial: bool,
    biotic_only: bool,
    transform: &'a str,
}

/// Write an Excel file with only the selected advanced analyses.
fn run_advanced_export(project: &Project, output_path: &std::path::Path, opts: &ExportOptions) -> Result<()> {
    let mut workbook = Workbook::new();
    let mut any_sheet = false;

    if opts.bray || opts.pcoa || opts.permanova || opts.simper {
        let comp = composition_matrix(project, opts.biotic_only, &Default::default(), opts.transform);
        let bc = bray_curtis_matrix(&comp.matrix);

        if opts.bray {
            let sheet = workbook.add_worksheet();
            sheet.set_name("Bray-Curtis")?;
            for (c, name) in comp.sample_names.iter().enumerate() {
                sheet.write_string(0, c as u16 + 1, name.as_str())?;
            }
            for (r, name) in comp.sample_names.iter().enumerate() {
                sheet.write_string(r as u32 + 1, 0, name.as_str())?;
                for c in 0..comp.sample_names.len() {
                    sheet.write_number(r as u32 + 1, c as u16 + 1, (bc[(r, c)] * 10000.0).round() / 10000.0)?;
                }
            }
            any_sheet = true;
        }

        if opts.pcoa {
            let result = pcoa(&bc, comp.sample_names.len().min(2).max(1));
            let n_axes = result.coords.ncols();
            let mut rows: Vec<Row> = Vec::new();
            for (i, name) in comp.sample_names.iter().enumerate() {
                let mut row: Row = vec![("station".into(), name.clone().into())];
                for ax in 0..n_axes {
                    row.push((format!("PCoA{}", ax + 1), (((result.coords[(i, ax)]) * 1_000_000.0).round() / 1_000_000.0).into()));
                }
                rows.push(row);
            }
            let mut var_row: Row = vec![("station".into(), "Variance explained".into())];
            for ax in 0..n_axes {
                var_row.push((format!("PCoA{}", ax + 1), result.variance_explained.get(ax).copied().unwrap_or(0.0).into()));
            }
            rows.push(var_row);
            write_table_sheet(&mut workbook, "Ordination", &rows, false)?;
            any_sheet = true;
        }

        if opts.permanova {
            match permanova(&bc, &comp.sample_names, 999, 42) {
                Err(msg) => write_table_sheet(&mut workbook, "PERMANOVA", &[vec![("Note".into(), msg.into())]], false)?,
                Ok(r) => write_table_sheet(
                    &mut workbook,
                    "PERMANOVA",
                    &[
                        metric_row("pseudo-F", r.pseudo_f),
                        metric_row("p-value", r.p_value),
                        metric_row("permutations", r.permutations as i64),
                        metric_row("significant (p<0.05)", r.significant.to_string()),
                    ],
                    false,
                )?,
            }
            any_sheet = true;
        }

        if opts.simper && comp.sample_names.len() >= 2 {
            let rows = simper(&comp.matrix, &comp.code_names, &comp.sample_names, &comp.sample_names[0], &comp.sample_names[1]);
            if !rows.is_empty() {
                let table: Vec<Row> = rows
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
                write_table_sheet(&mut workbook, "SIMPER", &table, false)?;
                any_sheet = true;
            }
        }
    }

    if opts.temporal {
        let trend = temporal_trend(project, "live_coral_pct");
        if trend.ok {
            let mut rows: Vec<Row> = Vec::new();
            for (sname, data) in &trend.stations {
                for (d, v) in data.dates.iter().zip(&data.values) {
                    rows.push(vec![("station".into(), sname.clone().into()), ("date".into(), d.clone().into()), ("value".into(), (*v).into())]);
                }
            }
            if !rows.is_empty() {
                write_table_sheet(&mut workbook, "Temporal", &rows, false)?;
                any_sheet = true;
            }
        } else {
            write_table_sheet(&mut workbook, "Temporal", &[vec![("Reason".into(), trend.reason.unwrap_or_default().into())]], false)?;
            any_sheet = true;
        }
    }

    if opts.depth {
        let dg = depth_gradient(project, "live_coral_pct");
        if dg.ok {
            write_table_sheet(
                &mut workbook,
                "Depth Gradient",
                &[metric_row("slope", dg.slope), metric_row("r_squared", dg.r_squared), metric_row("p_value", dg.p_value)],
                false,
            )?;
            any_sheet = true;
        }
    }

    if opts.spatial {
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
        if !rows.is_empty() {
            write_table_sheet(&mut workbook, "Map Data", &rows, false)?;
            any_sheet = true;
        }
    }

    if !any_sheet {
        write_table_sheet(&mut workbook, "Info", &[vec![("Note".into(), "Tidak ada analisa yang dipilih.".into())]], false)?;
    }

    workbook.save(output_path)?;
    Ok(())
}
