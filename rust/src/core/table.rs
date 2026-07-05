//! A tiny dynamic table representation used by the statistics/export functions
//! that in the Python code built ad hoc `dict` rows (later fed to
//! `pandas.DataFrame`, which unions columns across rows and fills gaps with
//! blank/NaN). Row order and per-row column order are preserved; the column
//! union (for writing an actual sheet) is computed at export time.

#[derive(Debug, Clone, PartialEq)]
pub enum Cell {
    Str(String),
    Num(f64),
    Int(i64),
    None,
}

impl Cell {
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Cell::Num(v) => Some(*v),
            Cell::Int(v) => Some(*v as f64),
            _ => None,
        }
    }
}

impl std::fmt::Display for Cell {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Cell::Str(s) => write!(f, "{s}"),
            Cell::Num(v) => write!(f, "{v}"),
            Cell::Int(v) => write!(f, "{v}"),
            Cell::None => write!(f, ""),
        }
    }
}

impl From<String> for Cell {
    fn from(v: String) -> Self {
        Cell::Str(v)
    }
}
impl From<&str> for Cell {
    fn from(v: &str) -> Self {
        Cell::Str(v.to_string())
    }
}
impl From<f64> for Cell {
    fn from(v: f64) -> Self {
        Cell::Num(v)
    }
}
impl From<i64> for Cell {
    fn from(v: i64) -> Self {
        Cell::Int(v)
    }
}
impl From<usize> for Cell {
    fn from(v: usize) -> Self {
        Cell::Int(v as i64)
    }
}
impl<T: Into<Cell>> From<Option<T>> for Cell {
    fn from(v: Option<T>) -> Self {
        v.map(Into::into).unwrap_or(Cell::None)
    }
}

/// An ordered row of (column, value) pairs — one row of an export table.
pub type Row = Vec<(String, Cell)>;

/// The ordered union of column names across all rows (first-seen order),
/// mirroring `pandas.DataFrame(rows)` column inference.
pub fn column_union(rows: &[Row]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut columns = Vec::new();
    for row in rows {
        for (col, _) in row {
            if seen.insert(col.clone()) {
                columns.push(col.clone());
            }
        }
    }
    columns
}
