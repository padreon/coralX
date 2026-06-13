"""Fragment Measurement Mode window."""

from __future__ import annotations

import os
from pathlib import Path

from PyQt6.QtWidgets import (
    QMainWindow, QWidget, QVBoxLayout, QHBoxLayout, QSplitter,
    QListWidget, QListWidgetItem, QPushButton, QLabel, QSlider,
    QTableWidget, QTableWidgetItem, QHeaderView, QFileDialog,
    QMessageBox, QAbstractItemView, QSizePolicy, QGroupBox,
    QStatusBar,
)
from PyQt6.QtCore import Qt, pyqtSignal
from PyQt6.QtGui import QFont

from src.models.project import Project, Station, ImageAnnotation, Measurement
from src.ui.image_canvas import ImageCanvas
from src.ui.calibration_dialog import CalibrationDialog
from src.ui.measurement_label_dialog import MeasurementLabelDialog


class MeasurementWindow(QMainWindow):
    """Main window for the Fragment Measurement workflow."""

    closed = pyqtSignal()

    def __init__(self, project: Project | None = None, parent=None):
        super().__init__(parent)
        self.setWindowTitle("coralX — Fragment Measurement")
        self.resize(1280, 800)

        self._project: Project = project or Project(name="Untitled Measurement")
        if not self._project.stations:
            self._project.stations.append(Station(name="Station 1"))

        self._current_ann: ImageAnnotation | None = None

        self._build_ui()
        self._refresh_image_list()

    # ------------------------------------------------------------------ build

    def _build_ui(self):
        central = QWidget()
        self.setCentralWidget(central)
        root = QHBoxLayout(central)
        root.setContentsMargins(4, 4, 4, 4)

        splitter = QSplitter(Qt.Orientation.Horizontal)
        root.addWidget(splitter)

        splitter.addWidget(self._build_left_panel())
        self._canvas = ImageCanvas()
        self._canvas.measurement_drawn.connect(self._on_measurement_drawn)
        self._canvas.status_message.connect(self._show_status)
        splitter.addWidget(self._canvas)
        splitter.addWidget(self._build_right_panel())

        splitter.setStretchFactor(0, 0)
        splitter.setStretchFactor(1, 1)
        splitter.setStretchFactor(2, 0)
        splitter.setSizes([220, 800, 280])

        self.setStatusBar(QStatusBar())
        self._show_status("Ready")

    def _build_left_panel(self) -> QWidget:
        panel = QWidget()
        panel.setFixedWidth(220)
        layout = QVBoxLayout(panel)
        layout.setContentsMargins(4, 4, 4, 4)
        layout.setSpacing(6)

        # Project buttons
        btn_new = QPushButton("New Project")
        btn_open = QPushButton("Open Project")
        btn_save = QPushButton("Save Project")
        btn_add = QPushButton("+ Add Images")
        for b in (btn_new, btn_open, btn_save, btn_add):
            layout.addWidget(b)
        btn_new.clicked.connect(self._new_project)
        btn_open.clicked.connect(self._open_project)
        btn_save.clicked.connect(self._save_project)
        btn_add.clicked.connect(self._add_images)

        # Image list
        layout.addWidget(QLabel("Images:"))
        self._img_list = QListWidget()
        self._img_list.currentRowChanged.connect(self._on_image_selected)
        layout.addWidget(self._img_list)

        # Zoom controls
        grp_zoom = QGroupBox("View")
        zoom_layout = QHBoxLayout(grp_zoom)
        zoom_layout.setContentsMargins(4, 4, 4, 4)
        btn_zi = QPushButton("↑ In")
        btn_zo = QPushButton("↓ Out")
        btn_zf = QPushButton("Fit")
        for b in (btn_zi, btn_zo, btn_zf):
            b.setFixedHeight(24)
            zoom_layout.addWidget(b)
        btn_zi.clicked.connect(lambda: self._canvas.zoom_in())
        btn_zo.clicked.connect(lambda: self._canvas.zoom_out())
        btn_zf.clicked.connect(lambda: self._canvas.zoom_fit())
        layout.addWidget(grp_zoom)

        # Calibration
        grp_calib = QGroupBox("Calibration")
        calib_layout = QVBoxLayout(grp_calib)
        btn_calib = QPushButton("📏 Calibrate Scale")
        btn_calib.clicked.connect(self._calibrate)
        calib_layout.addWidget(btn_calib)
        self._calib_label = QLabel("Not calibrated")
        self._calib_label.setWordWrap(True)
        self._calib_label.setStyleSheet("color: #aaa; font-size: 10px;")
        calib_layout.addWidget(self._calib_label)
        layout.addWidget(grp_calib)

        # Measurement tools
        grp_tools = QGroupBox("Measure")
        tools_layout = QVBoxLayout(grp_tools)

        btn_line = QPushButton("📏 Straight Line")
        btn_poly = QPushButton("〰 Polyline")
        btn_polygon = QPushButton("⬠ Polygon (manual)")
        btn_magic = QPushButton("🪄 Magic Wand")
        for b in (btn_line, btn_poly, btn_polygon, btn_magic):
            b.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Fixed)
            tools_layout.addWidget(b)

        btn_line.clicked.connect(lambda: self._start_measure("line"))
        btn_poly.clicked.connect(lambda: self._start_measure("polyline"))
        btn_polygon.clicked.connect(lambda: self._start_measure("polygon"))
        btn_magic.clicked.connect(lambda: self._start_measure("magic"))

        tol_label = QLabel("Magic tolerance:")
        self._tol_slider = QSlider(Qt.Orientation.Horizontal)
        self._tol_slider.setRange(1, 80)
        self._tol_slider.setValue(20)
        self._tol_value_lbl = QLabel("20")
        self._tol_slider.valueChanged.connect(self._on_tolerance_changed)
        tol_row = QHBoxLayout()
        tol_row.addWidget(self._tol_slider)
        tol_row.addWidget(self._tol_value_lbl)
        tools_layout.addWidget(tol_label)
        tools_layout.addLayout(tol_row)

        layout.addWidget(grp_tools)
        layout.addStretch()
        return panel

    def _build_right_panel(self) -> QWidget:
        panel = QWidget()
        panel.setFixedWidth(280)
        layout = QVBoxLayout(panel)
        layout.setContentsMargins(4, 4, 4, 4)
        layout.setSpacing(6)

        layout.addWidget(QLabel("Measurements (current image):"))
        self._meas_table = QTableWidget(0, 4)
        self._meas_table.setHorizontalHeaderLabels(["Label", "Type", "Value", "Unit"])
        self._meas_table.horizontalHeader().setSectionResizeMode(
            0, QHeaderView.ResizeMode.Stretch
        )
        self._meas_table.setSelectionBehavior(
            QAbstractItemView.SelectionBehavior.SelectRows
        )
        self._meas_table.setEditTriggers(
            QAbstractItemView.EditTrigger.NoEditTriggers
        )
        self._meas_table.itemSelectionChanged.connect(self._on_table_selection)
        layout.addWidget(self._meas_table)

        btn_delete = QPushButton("Delete Selected")
        btn_delete.clicked.connect(self._delete_measurement)
        layout.addWidget(btn_delete)

        layout.addSpacing(12)
        btn_export = QPushButton("Export Excel…")
        btn_export.clicked.connect(self._export_excel)
        layout.addWidget(btn_export)
        layout.addStretch()
        return panel

    # ------------------------------------------------------------------ project

    def _new_project(self):
        self._project = Project(name="Untitled Measurement")
        self._project.stations.append(Station(name="Station 1"))
        self._current_ann = None
        self._refresh_image_list()
        self._refresh_table()

    def _open_project(self):
        path, _ = QFileDialog.getOpenFileName(
            self, "Open Project", "", "coralX Projects (*.cpce)"
        )
        if path:
            try:
                self._project = Project.load(path)
                if not self._project.stations:
                    self._project.stations.append(Station(name="Station 1"))
                self._current_ann = None
                self._refresh_image_list()
                self._refresh_table()
            except Exception as e:
                QMessageBox.critical(self, "Error", f"Could not open project:\n{e}")

    def _save_project(self):
        path = self._project.save_path
        if not path:
            path, _ = QFileDialog.getSaveFileName(
                self, "Save Project", "", "coralX Projects (*.cpce)"
            )
        if path:
            try:
                self._project.save(path)
                self._show_status(f"Saved: {path}")
            except Exception as e:
                QMessageBox.critical(self, "Error", f"Could not save project:\n{e}")

    def _add_images(self):
        paths, _ = QFileDialog.getOpenFileNames(
            self, "Add Images", "",
            "Images (*.jpg *.jpeg *.png *.tif *.tiff *.bmp)"
        )
        if not paths:
            return
        station = self._project.stations[0]
        existing = {a.image_path for a in station.annotations}
        for p in paths:
            if p not in existing:
                station.annotations.append(ImageAnnotation(image_path=p))
        self._refresh_image_list()

    # ------------------------------------------------------------------ images

    def _refresh_image_list(self):
        self._img_list.clear()
        station = self._project.stations[0] if self._project.stations else None
        if not station:
            return
        for ann in station.annotations:
            name = Path(ann.image_path).name
            item = QListWidgetItem(name)
            item.setData(Qt.ItemDataRole.UserRole, ann)
            self._img_list.addItem(item)

    def _on_image_selected(self, row: int):
        station = self._project.stations[0] if self._project.stations else None
        if not station or row < 0 or row >= len(station.annotations):
            return
        ann = station.annotations[row]
        self._current_ann = ann
        self._canvas.load_image(ann, {})
        self._canvas.set_measurements(ann.measurements)
        self._canvas.set_measure_tolerance(self._tol_slider.value())
        self._update_calib_label(ann)
        self._refresh_table()

    def _update_calib_label(self, ann: ImageAnnotation):
        if ann.scale_factor > 1.0:
            self._calib_label.setText(
                f"1 {ann.scale_unit} = {ann.scale_factor:.1f} px"
            )
            self._calib_label.setStyleSheet("color: #6f6; font-size: 10px;")
        else:
            self._calib_label.setText("Not calibrated")
            self._calib_label.setStyleSheet("color: #aaa; font-size: 10px;")

    # ------------------------------------------------------------------ calib

    def _calibrate(self):
        if not self._current_ann:
            QMessageBox.information(self, "No Image", "Select an image first.")
            return
        dlg = CalibrationDialog(self._current_ann, self)
        dlg.calibration_applied.connect(self._on_calibration_applied)
        dlg.exec()

    def _on_calibration_applied(self, scale_factor: float, scale_unit: str, apply_all: bool):
        station = self._project.stations[0] if self._project.stations else None
        if not station:
            return
        targets = station.annotations if apply_all else (
            [self._current_ann] if self._current_ann else []
        )
        for a in targets:
            a.scale_factor = scale_factor
            a.scale_unit = scale_unit
        if self._current_ann:
            self._update_calib_label(self._current_ann)

    # ------------------------------------------------------------------ measure

    def _start_measure(self, mode: str):
        if not self._current_ann:
            QMessageBox.information(self, "No Image", "Select an image first.")
            return
        if self._current_ann.scale_factor <= 1.0:
            reply = QMessageBox.question(
                self, "Not Calibrated",
                "This image is not calibrated. Measurements will be in pixels.\n"
                "Continue anyway?",
                QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.No,
            )
            if reply != QMessageBox.StandardButton.Yes:
                return
        self._canvas.start_measurement(mode)

    def _on_tolerance_changed(self, val: int):
        self._tol_value_lbl.setText(str(val))
        self._canvas.set_measure_tolerance(val)

    def _on_measurement_drawn(self, measurement: Measurement):
        """Measurement geometry done — ask user to name the fragment."""
        dlg = MeasurementLabelDialog(measurement, self)
        if dlg.exec() != MeasurementLabelDialog.DialogCode.Accepted:
            return
        m = dlg.measurement()
        if self._current_ann is not None:
            self._current_ann.measurements.append(m)
            self._canvas.set_measurements(self._current_ann.measurements)
            self._refresh_table()
            self._show_status(
                f"Saved: {m.label} — {m.value:.3f} "
                f"{''+m.unit+'²' if m.type=='polygon' else m.unit}"
            )

    def _on_table_selection(self):
        self._canvas.update()

    def _refresh_table(self):
        self._meas_table.setRowCount(0)
        if not self._current_ann:
            return
        for m in self._current_ann.measurements:
            row = self._meas_table.rowCount()
            self._meas_table.insertRow(row)
            unit_str = f"{m.unit}²" if m.type == "polygon" else m.unit
            self._meas_table.setItem(row, 0, QTableWidgetItem(m.label))
            self._meas_table.setItem(row, 1, QTableWidgetItem(m.type))
            self._meas_table.setItem(row, 2, QTableWidgetItem(f"{m.value:.3f}"))
            self._meas_table.setItem(row, 3, QTableWidgetItem(unit_str))

    def _delete_measurement(self):
        row = self._meas_table.currentRow()
        if row < 0 or not self._current_ann:
            return
        if row < len(self._current_ann.measurements):
            self._current_ann.measurements.pop(row)
            self._canvas.set_measurements(self._current_ann.measurements)
            self._refresh_table()

    # ------------------------------------------------------------------ export

    def _export_excel(self):
        path, _ = QFileDialog.getSaveFileName(
            self, "Export Measurements", "", "Excel (*.xlsx)"
        )
        if not path:
            return
        try:
            from src.core.measurement_exporter import export_measurements_excel
            export_measurements_excel(self._project, path)
            self._show_status(f"Exported: {path}")
            QMessageBox.information(self, "Export Done", f"Saved to:\n{path}")
        except Exception as e:
            QMessageBox.critical(self, "Export Error", str(e))

    # ------------------------------------------------------------------ util

    def _show_status(self, msg: str):
        sb = self.statusBar()
        if sb:
            sb.showMessage(msg)

    def closeEvent(self, event):
        self.closed.emit()
        super().closeEvent(event)
