"""Export measurement data to Excel."""

from __future__ import annotations

from pathlib import Path

import pandas as pd

from src.models.project import Project


def export_measurements_excel(project: Project, output_path: str) -> None:
    """Write all measurements across all stations to a single Excel file."""
    rows = []
    for station in project.stations:
        for ann in station.annotations:
            image_name = Path(ann.image_path).name
            calib_info = (
                f"{ann.scale_factor:.2f} px/{ann.scale_unit}"
                if ann.scale_factor > 1.0
                else "uncalibrated"
            )
            for m in ann.measurements:
                unit_str = f"{m.unit}²" if m.type == "polygon" else m.unit
                rows.append({
                    "Station": station.name,
                    "Image": image_name,
                    "Calibration": calib_info,
                    "Fragment Label": m.label,
                    "Measurement Type": m.type,
                    "Value": round(m.value, 4),
                    "Unit": unit_str,
                    "ID": m.id,
                })

    df = pd.DataFrame(rows) if rows else pd.DataFrame(columns=[
        "Station", "Image", "Calibration", "Fragment Label",
        "Measurement Type", "Value", "Unit", "ID",
    ])

    with pd.ExcelWriter(output_path, engine="openpyxl") as writer:
        df.to_excel(writer, sheet_name="Measurements", index=False)

        ws = writer.sheets["Measurements"]
        # Auto-size columns
        for col in ws.columns:
            max_len = max((len(str(cell.value or "")) for cell in col), default=8)
            ws.column_dimensions[col[0].column_letter].width = min(max_len + 4, 50)
