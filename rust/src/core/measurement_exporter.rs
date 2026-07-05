//! Export measurement data to Excel.

use std::path::Path;

use anyhow::Result;
use rust_xlsxwriter::{Workbook, Worksheet};

use crate::core::table::{column_union, Cell, Row};
use crate::models::Project;

/// Write all measurements across all stations to a single Excel file.
pub fn export_measurements_excel(project: &Project, output_path: impl AsRef<Path>) -> Result<()> {
    let mut rows: Vec<Row> = Vec::new();

    for station in &project.stations {
        for ann in &station.annotations {
            let image_name = Path::new(&ann.image_path)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| ann.image_path.clone());
            let calibrated = ann.scale_factor > 1.0;
            let calib_info = if calibrated {
                format!("{:.2} px/{}", ann.scale_factor, ann.scale_unit)
            } else {
                "uncalibrated".to_string()
            };
            let unit = if calibrated { ann.scale_unit.clone() } else { "px".to_string() };
            let unit2 = format!("{unit}\u{b2}");

            for m in &ann.measurements {
                let is_area = m.kind == "polygon";
                let row: Row = vec![
                    ("Station".into(), station.name.clone().into()),
                    ("Image".into(), image_name.clone().into()),
                    ("Calibration".into(), calib_info.clone().into()),
                    ("Fragment Label".into(), m.label.clone().into()),
                    ("Spesies/Genus".into(), m.species.clone().into()),
                    ("Measurement Type".into(), m.kind.clone().into()),
                    (
                        format!("Width ({unit})"),
                        if m.auto_width != 0.0 { round4(m.auto_width).into() } else { Cell::None },
                    ),
                    (
                        format!("Height ({unit})"),
                        if m.auto_height != 0.0 { round4(m.auto_height).into() } else { Cell::None },
                    ),
                    (
                        format!("Area ({unit2})"),
                        if is_area { round4(m.area).into() } else { Cell::None },
                    ),
                    (
                        format!("Perimeter ({unit})"),
                        if is_area { round4(m.perimeter_len).into() } else { Cell::None },
                    ),
                    ("Value".into(), round4(m.value).into()),
                    ("Unit".into(), if is_area { unit2.clone() } else { unit.clone() }.into()),
                    ("ID".into(), m.id.clone().into()),
                ];
                rows.push(row);
            }
        }
    }

    // Fall back to generic headers when no measurements exist at all.
    let columns = if rows.is_empty() {
        vec![
            "Station", "Image", "Calibration", "Fragment Label", "Spesies/Genus",
            "Measurement Type", "Width", "Height", "Area", "Perimeter", "Value", "Unit", "ID",
        ]
        .into_iter()
        .map(String::from)
        .collect::<Vec<_>>()
    } else {
        column_union(&rows)
    };

    let mut workbook = Workbook::new();
    let sheet: &mut Worksheet = workbook.add_worksheet();
    sheet.set_name("Measurements")?;

    for (col, name) in columns.iter().enumerate() {
        sheet.write_string(0, col as u16, name.as_str())?;
    }
    for (r, row) in rows.iter().enumerate() {
        let lookup: std::collections::HashMap<&str, &Cell> =
            row.iter().map(|(k, v)| (k.as_str(), v)).collect();
        for (col, name) in columns.iter().enumerate() {
            if let Some(cell) = lookup.get(name.as_str()) {
                write_cell(sheet, r as u32 + 1, col as u16, cell)?;
            }
        }
    }

    sheet.autofit();
    workbook.save(output_path)?;
    Ok(())
}

fn write_cell(sheet: &mut Worksheet, row: u32, col: u16, cell: &Cell) -> Result<()> {
    match cell {
        Cell::Str(s) => {
            sheet.write_string(row, col, s.as_str())?;
        }
        Cell::Num(v) => {
            sheet.write_number(row, col, *v)?;
        }
        Cell::Int(v) => {
            sheet.write_number(row, col, *v as f64)?;
        }
        Cell::None => {}
    }
    Ok(())
}

fn round4(v: f64) -> f64 {
    (v * 10000.0).round() / 10000.0
}
