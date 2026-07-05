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
            calibrated = ann.scale_factor > 1.0
            calib_info = (
                f"{ann.scale_factor:.2f} px/{ann.scale_unit}"
                if calibrated else "uncalibrated"
            )
            unit = ann.scale_unit if calibrated else "px"
            unit2 = f"{unit}²"

            for m in ann.measurements:
                is_area = m.type == "polygon"
                rows.append({
                    "Station": station.name,
                    "Image": image_name,
                    "Calibration": calib_info,
                    "Fragment Label": m.label,
                    "Spesies/Genus": m.species,
                    "Measurement Type": m.type,
                    f"Width ({unit})": round(m.auto_width, 4) if m.auto_width else "",
                    f"Height ({unit})": round(m.auto_height, 4) if m.auto_height else "",
                    f"Area ({unit2})": round(m.area, 4) if is_area else "",
                    f"Perimeter ({unit})": round(m.perimeter_len, 4) if is_area else "",
                    f"Value": round(m.value, 4),
                    "Unit": unit2 if is_area else unit,
                    "ID": m.id,
                })

    columns = [
        "Station", "Image", "Calibration",
        "Fragment Label", "Spesies/Genus", "Measurement Type",
        f"Width ({unit})", f"Height ({unit})",
        f"Area ({unit2})", f"Perimeter ({unit})",
        "Value", "Unit", "ID",
    ]
    # fall back to generic headers when no rows exist
    if not rows:
        columns = [
            "Station", "Image", "Calibration",
            "Fragment Label", "Spesies/Genus", "Measurement Type",
            "Width", "Height", "Area", "Perimeter",
            "Value", "Unit", "ID",
        ]

    df = pd.DataFrame(rows) if rows else pd.DataFrame(columns=columns)

    with pd.ExcelWriter(output_path, engine="openpyxl") as writer:
        df.to_excel(writer, sheet_name="Measurements", index=False)

        ws = writer.sheets["Measurements"]
        for col in ws.columns:
            max_len = max((len(str(cell.value or "")) for cell in col), default=8)
            ws.column_dimensions[col[0].column_letter].width = min(max_len + 4, 50)
