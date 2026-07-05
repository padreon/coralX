//! Import functions: coral codes, station metadata, labeled points, CPCe Excel/CPC.

use std::collections::HashMap;
use std::path::Path;

use calamine::{open_workbook_auto, Data, Reader};

use crate::models::{ImageAnnotation, Point, Project, Station};

#[derive(Debug, Clone, Default)]
pub struct ImportResult {
    pub success: bool,
    pub message: String,
    pub warnings: Vec<String>,
}

impl ImportResult {
    fn ok(message: impl Into<String>, warnings: Vec<String>) -> Self {
        ImportResult { success: true, message: message.into(), warnings }
    }
    fn err(message: impl Into<String>) -> Self {
        ImportResult { success: false, message: message.into(), warnings: Vec::new() }
    }
}

// ─────────────────────────────────────────────────────────────
// Small CSV/Excel helpers (stand-ins for `pd.read_csv(dtype=str).fillna("")`)
// ─────────────────────────────────────────────────────────────

/// Lower-cased header row + string rows (missing/blank cells become "").
struct Table {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
}

impl Table {
    fn col(&self, name: &str) -> Option<usize> {
        self.headers.iter().position(|h| h == name)
    }
    fn find_col(&self, aliases: &[&str]) -> Option<usize> {
        aliases.iter().find_map(|a| self.col(a))
    }
    fn get<'a>(&'a self, row: &'a [String], col: Option<usize>) -> &'a str {
        col.and_then(|c| row.get(c)).map(|s| s.as_str()).unwrap_or("")
    }
}

fn read_csv_table(path: &Path, delimiter: u8) -> Result<Table, String> {
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .flexible(true)
        .from_path(path)
        .map_err(|e| e.to_string())?;
    let headers: Vec<String> =
        reader.headers().map_err(|e| e.to_string())?.iter().map(|h| h.trim().to_lowercase()).collect();
    let mut rows = Vec::new();
    for rec in reader.records() {
        let rec = rec.map_err(|e| e.to_string())?;
        rows.push(rec.iter().map(|s| s.to_string()).collect());
    }
    Ok(Table { headers, rows })
}

fn read_excel_table(path: &Path, preferred_sheets: &[&str]) -> Result<(String, Table), String> {
    let mut wb = open_workbook_auto(path).map_err(|e| e.to_string())?;
    let names = wb.sheet_names();
    let sheet = preferred_sheets
        .iter()
        .find(|s| names.iter().any(|n| n == *s))
        .map(|s| s.to_string())
        .or_else(|| names.first().cloned())
        .ok_or("workbook has no sheets")?;
    let table = read_excel_sheet(&mut wb, &sheet)?;
    Ok((sheet, table))
}

fn read_excel_sheet<R: std::io::Read + std::io::Seek>(
    wb: &mut calamine::Sheets<R>,
    sheet: &str,
) -> Result<Table, String> {
    let range = wb.worksheet_range(sheet).map_err(|e| e.to_string())?;
    let mut rows_iter = range.rows();
    let Some(header_row) = rows_iter.next() else {
        return Ok(Table { headers: Vec::new(), rows: Vec::new() });
    };
    let headers: Vec<String> = header_row.iter().map(|c| cell_to_string(c).trim().to_lowercase()).collect();
    let rows: Vec<Vec<String>> =
        rows_iter.map(|r| r.iter().map(cell_to_string).collect()).collect();
    Ok(Table { headers, rows })
}

fn cell_to_string(c: &Data) -> String {
    match c {
        Data::Empty => String::new(),
        other => other.to_string(),
    }
}

fn parse_f64(s: &str) -> Option<f64> {
    let s = s.trim();
    if s.is_empty() {
        None
    } else {
        s.parse().ok()
    }
}

// ─────────────────────────────────────────────────────────────
// 1. Import coral codes from JSON, CSV, or CPCe .txt
// ─────────────────────────────────────────────────────────────

pub struct ImportedGroup {
    pub name: String,
    pub codes: Vec<String>,
    pub color: Option<String>,
}

/// Split one CPCe-style quoted-CSV line into up to 3 trimmed, unquoted fields.
fn parse_cpce_field_line(line: &str) -> Option<(String, String, String)> {
    let line = line.trim();
    if line.is_empty() || line.to_uppercase().starts_with("NOTES") {
        return None;
    }
    let mut reader = csv::ReaderBuilder::new().has_headers(false).trim(csv::Trim::All).from_reader(line.as_bytes());
    let rec = reader.records().next()?.ok()?;
    let fields: Vec<String> = rec.iter().map(|f| f.trim().to_string()).collect();
    if fields.len() < 2 {
        return None;
    }
    let code = fields[0].trim().trim_matches('"').to_string();
    let desc = fields.get(1).cloned().unwrap_or_default();
    let third = fields.get(2).cloned().unwrap_or_default();
    if code.is_empty() {
        return None;
    }
    Some((code, desc, third))
}

fn is_hex_color(s: &str) -> bool {
    let s = s.trim();
    s.len() == 6 && u32::from_str_radix(s, 16).is_ok()
}

fn parse_cpce_code_txt(content: &str) -> Result<(HashMap<String, String>, Vec<ImportedGroup>, Vec<String>), String> {
    let lines: Vec<&str> = content.lines().map(|l| l.trim_end()).filter(|l| !l.trim().is_empty()).collect();
    let mut warnings = Vec::new();
    let mut codes: HashMap<String, String> = HashMap::new();

    if lines.len() < 3 {
        return Err("File too short to be a CPCe code file.".to_string());
    }
    let n_cats: usize =
        lines[2].trim().parse().map_err(|_| format!("Expected integer on line 3, got: '{}'", lines[2]))?;

    struct CategoryInfo {
        description: String,
        color: String,
    }
    let mut category_map: HashMap<String, CategoryInfo> = HashMap::new();
    let mut category_order: Vec<String> = Vec::new();

    for (i, line) in lines.iter().enumerate().skip(3).take(n_cats) {
        let Some((cat_code, cat_desc, third)) = parse_cpce_field_line(line) else {
            warnings.push(format!("Could not parse category line {}: '{}'", i + 1, line));
            continue;
        };
        let color = if is_hex_color(&third) { third } else { String::new() };
        codes.insert(cat_code.clone(), cat_desc.clone());
        category_order.push(cat_code.clone());
        category_map.insert(cat_code, CategoryInfo { description: cat_desc, color });
    }

    let mut group_members: HashMap<String, Vec<String>> =
        category_map.keys().map(|k| (k.clone(), Vec::new())).collect();

    for line in lines.iter().skip(3 + n_cats) {
        let line = line.trim();
        if line.is_empty() || line.to_uppercase().starts_with("NOTES") {
            continue;
        }
        let Some((code, desc, parent)) = parse_cpce_field_line(line) else { continue };
        if is_hex_color(&code) {
            continue;
        }
        codes.insert(code.clone(), desc);

        if category_map.contains_key(&parent) {
            let members = group_members.entry(parent).or_default();
            if !members.contains(&code) {
                members.push(code);
            }
        } else if !parent.is_empty() && parent != "NA" && parent != "N/A" {
            warnings.push(format!("Code '{code}' has unknown parent '{parent}' — added ungrouped."));
        }
    }

    let groups = category_order
        .into_iter()
        .map(|cat_code| {
            let info = &category_map[&cat_code];
            ImportedGroup {
                name: info.description.clone(),
                codes: group_members.remove(&cat_code).unwrap_or_default(),
                color: if info.color.is_empty() { None } else { Some(info.color.clone()) },
            }
        })
        .collect();

    Ok((codes, groups, warnings))
}

/// Read coral codes (and optionally groups) from a JSON, CSV, or CPCe `.txt` file.
pub fn import_coral_codes(
    path: impl AsRef<Path>,
) -> (HashMap<String, String>, Vec<ImportedGroup>, ImportResult) {
    let path = path.as_ref();
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    let mut codes: HashMap<String, String> = HashMap::new();
    let mut groups: Vec<ImportedGroup> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    let result: Result<(), String> = (|| {
        match ext.as_str() {
            "json" => {
                let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
                let value: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
                match value {
                    serde_json::Value::Object(map) => {
                        let raw = if let Some(c) = map.get("codes") {
                            if let Some(gs) = map.get("groups").and_then(|g| g.as_array()) {
                                for g in gs {
                                    let name = g.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                                    let gcodes = g
                                        .get("codes")
                                        .and_then(|v| v.as_array())
                                        .map(|a| a.iter().filter_map(|c| c.as_str().map(String::from)).collect())
                                        .unwrap_or_default();
                                    groups.push(ImportedGroup { name, codes: gcodes, color: None });
                                }
                            }
                            c.clone()
                        } else {
                            serde_json::Value::Object(map.clone())
                        };
                        if let serde_json::Value::Object(raw_map) = raw {
                            for (k, v) in raw_map {
                                codes.insert(k, v.as_str().unwrap_or_default().to_string());
                            }
                        } else {
                            return Err(format!("Unexpected JSON structure in {}", path.display()));
                        }
                    }
                    serde_json::Value::Array(items) => {
                        for item in items {
                            let code = item.get("code").and_then(|v| v.as_str()).unwrap_or("").to_uppercase();
                            let desc = item.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string();
                            if !code.is_empty() {
                                codes.insert(code, desc);
                            }
                        }
                    }
                    _ => return Err(format!("Unexpected JSON root type in {}", path.display())),
                }
            }
            "txt" => {
                let content = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
                let (c, g, w) = parse_cpce_code_txt(&content)?;
                codes = c;
                groups = g;
                warnings = w;
            }
            "csv" | "tsv" => {
                let delim = if ext == "tsv" { b'\t' } else { b',' };
                let table = read_csv_table(path, delim)?;
                let code_col = table.find_col(&["code", "kode"]);
                let desc_col = table.find_col(&["description", "desc", "deskripsi", "name"]);
                let group_col = table.find_col(&["group", "grup", "category"]);
                let Some(code_col) = code_col else {
                    return Err("CSV must have a 'code' column.".to_string());
                };
                let mut group_map: HashMap<String, Vec<String>> = HashMap::new();
                for row in &table.rows {
                    let code = table.get(row, Some(code_col)).trim().to_uppercase();
                    if code.is_empty() {
                        continue;
                    }
                    let desc = table.get(row, desc_col).trim().to_string();
                    codes.insert(code.clone(), desc);
                    if let Some(gc) = group_col {
                        let grp = table.get(row, Some(gc)).trim().to_string();
                        if !grp.is_empty() {
                            group_map.entry(grp).or_default().push(code);
                        }
                    }
                }
                groups = group_map
                    .into_iter()
                    .map(|(name, codes)| ImportedGroup { name, codes, color: None })
                    .collect();
            }
            other => return Err(format!("Unsupported file type: .{other}. Use .txt, .json, or .csv")),
        }
        Ok(())
    })();

    if let Err(e) = result {
        return (HashMap::new(), Vec::new(), ImportResult::err(format!("Failed to read file: {e}")));
    }
    if codes.is_empty() {
        return (HashMap::new(), Vec::new(), ImportResult::err("No codes found in file."));
    }

    let msg = if groups.is_empty() {
        format!("Loaded {} code(s)", codes.len())
    } else {
        format!("Loaded {} code(s) and {} group(s)", codes.len(), groups.len())
    };
    (codes, groups, ImportResult::ok(msg, warnings))
}

// ─────────────────────────────────────────────────────────────
// 2. Import station metadata from CSV
// ─────────────────────────────────────────────────────────────

pub struct ImportedStation {
    pub name: String,
    pub depth_m: Option<f64>,
    pub date: Option<String>,
    pub gps_lat: Option<f64>,
    pub gps_lon: Option<f64>,
    pub notes: String,
}

/// Read station metadata from a CSV file.
///
/// Required column: station (or station_name / site / transect). Optional:
/// depth_m, date (ISO-8601), gps_lat, gps_lon, notes.
pub fn import_station_metadata(path: impl AsRef<Path>) -> (Vec<ImportedStation>, ImportResult) {
    let table = match read_csv_table(path.as_ref(), b',') {
        Ok(t) => t,
        Err(e) => return (Vec::new(), ImportResult::err(format!("Cannot read CSV: {e}"))),
    };

    let station_col = table.find_col(&["station", "station_name", "nama_stasiun", "site", "transect"]);
    let depth_col = table.find_col(&["depth_m", "depth", "kedalaman", "kedalaman_m"]);
    let date_col = table.find_col(&["date", "tanggal", "survey_date"]);
    let lat_col = table.find_col(&["gps_lat", "lat", "latitude", "lintang"]);
    let lon_col = table.find_col(&["gps_lon", "lon", "longitude", "bujur"]);
    let notes_col = table.find_col(&["notes", "catatan", "remarks", "note"]);

    let Some(station_col) = station_col else {
        return (
            Vec::new(),
            ImportResult::err("CSV must have a station name column (e.g. 'station', 'site', or 'transect')."),
        );
    };

    let mut stations = Vec::new();
    let mut warnings = Vec::new();

    for row in &table.rows {
        let name = table.get(row, Some(station_col)).trim().to_string();
        if name.is_empty() {
            continue;
        }
        let mut parse_opt = |col: Option<usize>, field: &str| -> Option<f64> {
            let val = table.get(row, col).trim();
            if val.is_empty() {
                return None;
            }
            match parse_f64(val) {
                Some(v) => Some(v),
                None => {
                    warnings.push(format!("Station '{name}': invalid {field} value '{val}'"));
                    None
                }
            }
        };
        let depth_m = parse_opt(depth_col, "depth_m");
        let gps_lat = parse_opt(lat_col, "gps_lat");
        let gps_lon = parse_opt(lon_col, "gps_lon");
        let date = {
            let v = table.get(row, date_col).trim().to_string();
            if v.is_empty() { None } else { Some(v) }
        };
        let notes = table.get(row, notes_col).trim().to_string();

        stations.push(ImportedStation { name, depth_m, date, gps_lat, gps_lon, notes });
    }

    if stations.is_empty() {
        return (Vec::new(), ImportResult::err("No station rows found in CSV."));
    }
    let msg = format!("Read {} station(s).", stations.len());
    (stations, ImportResult::ok(msg, warnings))
}

// ─────────────────────────────────────────────────────────────
// 3. Import labeled points from CSV/Excel (coralX export format)
// ─────────────────────────────────────────────────────────────

/// Read labeled point data from a CSV or Excel file and apply labels to
/// annotations already present in `project`.
///
/// The image column is matched by basename against annotations already in
/// the project.
pub fn import_labeled_points(path: impl AsRef<Path>, project: &mut Project) -> ImportResult {
    let path = path.as_ref();
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();

    let table = match ext.as_str() {
        "xlsx" | "xls" => match read_excel_table(path, &["Raw Points"]) {
            Ok((_, t)) => t,
            Err(e) => return ImportResult::err(format!("Cannot read file: {e}")),
        },
        "csv" => match read_csv_table(path, b',') {
            Ok(t) => t,
            Err(e) => return ImportResult::err(format!("Cannot read file: {e}")),
        },
        other => return ImportResult::err(format!("Unsupported file type: .{other}. Use .csv or .xlsx")),
    };

    let image_col = table.find_col(&["image", "gambar", "frame", "filename", "file"]);
    let idx_col = table.find_col(&["point_index", "point", "index", "no", "#"]);
    let label_col = table.find_col(&["label", "code", "kode", "species", "substrate"]);
    let cat_col = table.find_col(&["category", "group", "kategori"]);

    let (Some(image_col), Some(label_col)) = (image_col, label_col) else {
        return ImportResult::err("File must have columns: 'image' (or 'frame') and 'label' (or 'code').");
    };

    // basename (lowercase) -> (station_idx, annotation_idx)
    let mut ann_map: HashMap<String, (usize, usize)> = HashMap::new();
    for (si, st) in project.stations.iter().enumerate() {
        for (ai, ann) in st.annotations.iter().enumerate() {
            let base = basename_lower(&ann.image_path);
            ann_map.insert(base, (si, ai));
        }
    }

    let mut matched = 0usize;
    let mut updated_labels = 0usize;
    let mut skipped_images: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut warnings = Vec::new();

    for row in &table.rows {
        let img_raw = table.get(row, Some(image_col)).trim();
        let img_key = basename_lower(img_raw);
        let label = table.get(row, Some(label_col)).trim().to_uppercase();
        if img_key.is_empty() || label.is_empty() {
            continue;
        }

        let Some(&(si, ai)) = ann_map.get(&img_key) else {
            skipped_images.insert(img_key);
            continue;
        };
        matched += 1;

        let ann = &mut project.stations[si].annotations[ai];
        let target_idx = if let Some(ic) = idx_col {
            table.get(row, Some(ic)).trim().parse::<i64>().ok()
        } else {
            None
        };

        let pt = if let Some(idx) = target_idx {
            ann.points.iter_mut().find(|p| p.index == idx)
        } else {
            ann.points.iter_mut().find(|p| p.label.is_none())
        };

        let Some(pt) = pt else {
            warnings.push(format!("{img_key}: no matching point for row (label={label})"));
            continue;
        };

        pt.label = Some(label);
        if let Some(cc) = cat_col {
            let cat = table.get(row, Some(cc)).trim().to_string();
            pt.category = if cat.is_empty() { None } else { Some(cat) };
        }
        updated_labels += 1;
    }

    if !skipped_images.is_empty() {
        let sample: Vec<&String> = skipped_images.iter().take(5).collect();
        let more = if skipped_images.len() > 5 { "…" } else { "" };
        warnings.push(format!(
            "{} image(s) not found in project: {}{more}",
            skipped_images.len(),
            sample.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
        ));
    }

    if updated_labels == 0 && warnings.is_empty() {
        return ImportResult::err("No labels were applied. Check that images are added to the project first.");
    }

    ImportResult::ok(format!("Applied {updated_labels} label(s) across {matched} row(s)."), warnings)
}

fn basename_lower(p: &str) -> String {
    Path::new(p).file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_else(|| p.to_lowercase())
}

// ─────────────────────────────────────────────────────────────
// 4. Import from old CPCe Excel export
// ─────────────────────────────────────────────────────────────

struct CpceColumnMap {
    code: Option<usize>,
    x: Option<usize>,
    y: Option<usize>,
    idx: Option<usize>,
    image: Option<usize>,
    station: Option<usize>,
}

fn detect_cpce_cols(table: &Table) -> CpceColumnMap {
    CpceColumnMap {
        code: table.find_col(&["code", "substrate", "species", "label", "kode"]),
        x: table.find_col(&["x", "x_coord", "x coordinate", "col", "column"]),
        y: table.find_col(&["y", "y_coord", "y coordinate", "row"]),
        idx: table.find_col(&["point", "point #", "point#", "#", "no", "index", "point_index"]),
        image: table.find_col(&["image", "frame", "image name", "filename", "file", "photo"]),
        station: table.find_col(&["station", "transect", "site", "stasiun"]),
    }
}

/// Import an Excel file exported by the original CPCe (Visual Basic) software.
///
/// Tries a preferred sheet name first; if none matches, treats every sheet as
/// a separate image (multi-sheet CPCe export). See the Python implementation
/// for the full strategy notes.
pub fn import_cpce_excel(path: impl AsRef<Path>) -> (Option<Project>, ImportResult) {
    let path = path.as_ref();
    let mut wb = match open_workbook_auto(path) {
        Ok(wb) => wb,
        Err(e) => return (None, ImportResult::err(format!("Cannot open Excel file: {e}"))),
    };
    let names = wb.sheet_names();
    let preferred = ["Raw Points", "Data", "Points", "Titik"];
    let target_sheet = preferred.iter().find(|s| names.iter().any(|n| n == *s));

    let mut warnings = Vec::new();
    let project_name =
        path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "Imported".to_string());
    let mut project = Project::new(project_name);
    let mut total_points = 0usize;

    let sheets_to_read: Vec<String> =
        if let Some(t) = target_sheet { vec![t.to_string()] } else { names.clone() };
    let multi_sheet_mode = target_sheet.is_none();

    let mut tables: Vec<(String, Table)> = Vec::new();
    for name in &sheets_to_read {
        match read_excel_sheet(&mut wb, name) {
            Ok(t) => tables.push((name.clone(), t)),
            Err(e) => warnings.push(format!("Sheet '{name}': {e}")),
        }
    }

    if multi_sheet_mode || tables.len() > 1 {
        let mut default_station = Station::new("Imported Station");
        for (sheet_name, table) in &tables {
            if table.rows.is_empty() || table.headers.len() < 2 {
                continue;
            }
            let mapping = detect_cpce_cols(table);
            let Some(code_col) = mapping.code else {
                warnings.push(format!("Sheet '{sheet_name}': no code column found, skipped."));
                continue;
            };
            let mut ann = ImageAnnotation::new(sheet_name.clone());
            for (i, row) in table.rows.iter().enumerate() {
                let code = table.get(row, Some(code_col)).trim().to_uppercase();
                if code.is_empty() || code == "CODE" || code == "KODE" {
                    continue;
                }
                let x = mapping.x.and_then(|c| parse_f64(table.get(row, Some(c)))).unwrap_or(i as f64);
                let y = mapping.y.and_then(|c| parse_f64(table.get(row, Some(c)))).unwrap_or(0.0);
                let idx = mapping
                    .idx
                    .and_then(|c| table.get(row, Some(c)).trim().parse::<i64>().ok())
                    .unwrap_or(i as i64);
                ann.points.push(Point { x, y, index: idx, label: Some(code), category: None });
                total_points += 1;
            }
            if !ann.points.is_empty() {
                default_station.annotations.push(ann);
            }
        }
        if !default_station.annotations.is_empty() {
            project.stations.push(default_station);
        }
    } else if let Some((_, table)) = tables.into_iter().next() {
        let mapping = detect_cpce_cols(&table);
        let Some(code_col) = mapping.code else {
            return (
                None,
                ImportResult::err(format!(
                    "No code/species column found. Expected columns like 'code', 'substrate', or 'species'.\nFound: {:?}",
                    table.headers
                )),
            );
        };
        let mut station_order: Vec<String> = Vec::new();
        let mut station_map: HashMap<String, usize> = HashMap::new();

        for (i, row) in table.rows.iter().enumerate() {
            let code = table.get(row, Some(code_col)).trim().to_uppercase();
            if code.is_empty() || matches!(code.as_str(), "CODE" | "KODE" | "SUBSTRATE" | "SPECIES") {
                continue;
            }
            let img_name = mapping
                .image
                .map(|c| table.get(row, Some(c)).trim().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "image_1".to_string());
            let st_name = mapping
                .station
                .map(|c| table.get(row, Some(c)).trim().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "Imported Station".to_string());

            let si = *station_map.entry(st_name.clone()).or_insert_with(|| {
                station_order.push(st_name.clone());
                project.stations.push(Station::new(st_name.clone()));
                project.stations.len() - 1
            });

            let img_key = basename_lower(&img_name);
            let ai = project.stations[si]
                .annotations
                .iter()
                .position(|a| basename_lower(&a.image_path) == img_key)
                .unwrap_or_else(|| {
                    project.stations[si].annotations.push(ImageAnnotation::new(img_name.clone()));
                    project.stations[si].annotations.len() - 1
                });

            let x = mapping.x.and_then(|c| parse_f64(table.get(row, Some(c)))).unwrap_or(i as f64);
            let y = mapping.y.and_then(|c| parse_f64(table.get(row, Some(c)))).unwrap_or(0.0);
            let ann = &mut project.stations[si].annotations[ai];
            let idx = mapping
                .idx
                .and_then(|c| table.get(row, Some(c)).trim().parse::<i64>().ok())
                .unwrap_or(ann.points.len() as i64);
            ann.points.push(Point { x, y, index: idx, label: Some(code), category: None });
            total_points += 1;
        }
    }

    if total_points == 0 {
        return (
            None,
            ImportResult {
                success: false,
                message: "No point data found in the file. Make sure the file contains per-point rows (not just summary stats).".to_string(),
                warnings,
            },
        );
    }

    let n_stations = project.stations.len();
    let n_images: usize = project.stations.iter().map(|s| s.annotations.len()).sum();
    (
        Some(project),
        ImportResult::ok(
            format!("Imported {total_points} points across {n_images} image(s) in {n_stations} station(s)."),
            warnings,
        ),
    )
}

// ─────────────────────────────────────────────────────────────
// 5. Import CPCe native .cpc annotation file
// ─────────────────────────────────────────────────────────────

/// Import a single CPCe native `.cpc` annotation file.
///
/// CPCe uses twips (1/1440 inch) as its coordinate unit; see the Python
/// implementation's docstring for the full format layout and image-path
/// resolution order.
pub fn import_cpce_cpc(path: impl AsRef<Path>, image_dir: Option<&Path>) -> (Option<ImageAnnotation>, ImportResult) {
    let path = path.as_ref();
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => return (None, ImportResult::err(format!("Cannot read .cpc file: {e}"))),
    };
    let lines: Vec<&str> = content.lines().map(|l| l.trim_end()).collect();
    if lines.len() < 7 {
        return (None, ImportResult::err("File too short to be a valid .cpc file."));
    }

    let mut warnings = Vec::new();

    let mut header_reader =
        csv::ReaderBuilder::new().has_headers(false).from_reader(lines[0].as_bytes());
    let Some(Ok(header_rec)) = header_reader.records().next() else {
        return (None, ImportResult::err("Cannot parse header line."));
    };
    let header: Vec<String> = header_rec.iter().map(|f| f.trim().to_string()).collect();
    if header.len() < 4 {
        return (None, ImportResult::err(format!("Header has too few fields ({}): {}", header.len(), lines[0])));
    }

    let orig_image_path = header[1].trim().to_string();
    let (Some(int_width), Some(int_height)) = (parse_f64(&header[2]), parse_f64(&header[3])) else {
        return (None, ImportResult::err("Cannot parse image dimensions from header."));
    };
    if int_width <= 0.0 || int_height <= 0.0 {
        return (None, ImportResult::err(format!("Invalid internal dimensions: {int_width}x{int_height}")));
    }

    let normalized = orig_image_path.replace('\\', "/");
    let image_basename = Path::new(&normalized).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let mut resolved_path = orig_image_path.clone();
    let mut img_width: Option<u32> = None;
    let mut img_height: Option<u32> = None;

    let cpc_dir = path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
    let mut search_dirs = Vec::new();
    if let Some(d) = image_dir {
        search_dirs.push(d.to_path_buf());
    }
    search_dirs.push(cpc_dir);

    let mut candidates = vec![std::path::PathBuf::from(&orig_image_path)];
    candidates.extend(search_dirs.iter().map(|d| d.join(&image_basename)));

    for candidate in &candidates {
        if candidate.exists() {
            if let Ok((w, h)) = image::image_dimensions(candidate) {
                img_width = Some(w);
                img_height = Some(h);
                resolved_path = candidate.to_string_lossy().into_owned();
                break;
            }
        }
    }

    let (img_width, img_height) = match (img_width, img_height) {
        (Some(w), Some(h)) => (w, h),
        _ => {
            let w = (int_width / 15.0).round() as u32;
            let h = (int_height / 15.0).round() as u32;
            warnings.push(format!(
                "Image not found: '{image_basename}'. Estimated dimensions {w}x{h} px from .cpc header (96 DPI)."
            ));
            (w, h)
        }
    };

    let scale_x = img_width as f64 / int_width;
    let scale_y = img_height as f64 / int_height;
    let twips_to_px = |tx: f64, ty: f64| -> (f64, f64) {
        (((tx * scale_x) * 100.0).round() / 100.0, ((ty * scale_y) * 100.0).round() / 100.0)
    };

    // Border rectangle (lines 1-4) — parsed for parity with the Python importer,
    // not currently attached to ImageAnnotation (no border field there yet).
    for (i, line) in lines.iter().enumerate().take(5).skip(1) {
        let parts: Vec<&str> = line.split(',').collect();
        if parts.len() < 2 || parse_f64(parts[0]).is_none() || parse_f64(parts[1]).is_none() {
            warnings.push(format!("Cannot parse border corner on line {}", i + 1));
        }
    }

    let Ok(n_points) = lines[5].trim().parse::<usize>() else {
        return (None, ImportResult::err(format!("Cannot parse point count on line 6: '{}'", lines[5])));
    };
    let coord_end = 6 + n_points;
    let label_end = coord_end + n_points;
    if coord_end > lines.len() {
        return (
            None,
            ImportResult::err(format!(
                ".cpc declares {n_points} points but file has only {} lines after header.",
                lines.len().saturating_sub(6)
            )),
        );
    }

    let mut points: Vec<Point> = Vec::new();
    for (i, line) in lines.iter().enumerate().take(coord_end).skip(6) {
        let parts: Vec<&str> = line.split(',').collect();
        match (parts.first().and_then(|s| parse_f64(s)), parts.get(1).and_then(|s| parse_f64(s))) {
            (Some(tx), Some(ty)) => {
                let (px, py) = twips_to_px(tx, ty);
                points.push(Point { x: px, y: py, index: (i - 5) as i64, label: None, category: None });
            }
            _ => warnings.push(format!("Cannot parse point coordinate on line {}: '{line}'", i + 1)),
        }
    }

    let mut label_map: HashMap<i64, String> = HashMap::new();
    for line in lines.iter().take(label_end.min(lines.len())).skip(coord_end) {
        let line = line.trim();
        if line.is_empty() || line.chars().all(|c| c == ' ' || c == '"') {
            continue;
        }
        let mut reader =
            csv::ReaderBuilder::new().has_headers(false).trim(csv::Trim::All).from_reader(line.as_bytes());
        if let Some(Ok(rec)) = reader.records().next() {
            let fields: Vec<String> = rec.iter().map(|f| f.trim().trim_matches('"').to_string()).collect();
            if fields.len() >= 2 {
                if let Ok(pt_idx) = fields[0].parse::<i64>() {
                    if !fields[1].is_empty() {
                        label_map.insert(pt_idx, fields[1].clone());
                    }
                }
            }
        }
    }

    for pt in &mut points {
        if let Some(code) = label_map.get(&pt.index) {
            pt.label = Some(code.clone());
        }
    }

    let labeled = points.iter().filter(|p| p.label.is_some()).count();
    let msg = format!("Imported {} points ({labeled} labeled) from '{image_basename}'.", points.len());

    let ann = ImageAnnotation {
        image_path: resolved_path,
        points,
        image_width: img_width as i64,
        image_height: img_height as i64,
        scale_factor: 1.0,
        scale_unit: "cm".to_string(),
        measurements: Vec::new(),
    };
    (Some(ann), ImportResult::ok(msg, warnings))
}
