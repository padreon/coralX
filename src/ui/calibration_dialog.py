"""Dialog for calibrating image scale: user clicks two points and enters real-world distance."""

import math
from PyQt6.QtWidgets import (
    QDialog, QVBoxLayout, QHBoxLayout, QLabel, QDoubleSpinBox,
    QComboBox, QCheckBox, QPushButton, QWidget,
    QDialogButtonBox, QSizePolicy,
)
from PyQt6.QtCore import Qt, QPoint, pyqtSignal
from PyQt6.QtGui import QPainter, QPen, QColor, QPixmap, QBrush, QFont, QTransform

from src.models.project import ImageAnnotation


POINT_RADIUS = 6
CANVAS_SIZE = 600   # initial display size


class _CalibCanvas(QWidget):
    """
    Zoomable canvas for placing exactly two calibration points.
    - Scroll wheel or ↑/↓ arrow keys: zoom in/out
    - Middle-mouse drag: pan
    - Left click: place / replace calibration points
    """

    points_changed = pyqtSignal(list)   # emits list of (x, y) in image coords

    def __init__(self, pixmap: QPixmap, parent=None):
        super().__init__(parent)
        self.setFocusPolicy(Qt.FocusPolicy.StrongFocus)
        self._orig = pixmap           # full-resolution original
        self._clicks: list[tuple[int, int]] = []
        self._pan_start: QPoint | None = None

        # Zoom/pan state (same model as ImageCanvas)
        self._zoom: float = 1.0
        self._offset = QPoint(0, 0)

        self.setMinimumSize(CANVAS_SIZE, CANVAS_SIZE)
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding)
        self.setCursor(Qt.CursorShape.CrossCursor)

    # -------------------------------------------------------- public

    def zoom_in(self) -> None:
        self._apply_zoom(self._zoom * 1.25)

    def zoom_out(self) -> None:
        self._apply_zoom(self._zoom / 1.25)

    def zoom_fit(self) -> None:
        if not self._orig.isNull():
            wr = self.width() / self._orig.width()
            hr = self.height() / self._orig.height()
            self._zoom = min(wr, hr) * 0.95
            self._offset = QPoint(0, 0)
            self.update()

    def _apply_zoom(self, new_zoom: float, anchor: QPoint | None = None) -> None:
        new_zoom = max(0.1, min(20.0, new_zoom))
        if anchor and not self._orig.isNull():
            inv, ok = self._build_transform().inverted()
            if ok:
                img = inv.map(anchor)
                pw, ph = self._orig.width(), self._orig.height()
                self._offset = QPoint(
                    int(anchor.x() - self.width()  / 2 - (img.x() - pw / 2) * new_zoom),
                    int(anchor.y() - self.height() / 2 - (img.y() - ph / 2) * new_zoom),
                )
        self._zoom = new_zoom
        self.update()

    def _build_transform(self) -> QTransform:
        cx = self.width()  / 2 + self._offset.x()
        cy = self.height() / 2 + self._offset.y()
        t = QTransform()
        t.translate(cx, cy)
        t.scale(self._zoom, self._zoom)
        if not self._orig.isNull():
            t.translate(-self._orig.width() / 2, -self._orig.height() / 2)
        return t

    def _screen_to_image(self, pos: QPoint) -> tuple[int, int]:
        inv, ok = self._build_transform().inverted()
        if ok:
            m = inv.map(pos)
            return (int(m.x()), int(m.y()))
        return (int(pos.x()), int(pos.y()))

    # -------------------------------------------------------- events

    def mousePressEvent(self, event):
        if event.button() == Qt.MouseButton.MiddleButton:
            self._pan_start = event.pos()
            return
        if event.button() != Qt.MouseButton.LeftButton:
            return
        if len(self._clicks) >= 2:
            self._clicks.clear()
        ix, iy = self._screen_to_image(event.pos())
        # Clamp to image bounds
        ix = max(0, min(self._orig.width() - 1, ix))
        iy = max(0, min(self._orig.height() - 1, iy))
        self._clicks.append((ix, iy))
        self.points_changed.emit(list(self._clicks))
        self.update()

    def mouseMoveEvent(self, event):
        if self._pan_start and event.buttons() & Qt.MouseButton.MiddleButton:
            delta = event.pos() - self._pan_start
            self._offset += delta
            self._pan_start = event.pos()
            self.update()

    def mouseReleaseEvent(self, event):
        if event.button() == Qt.MouseButton.MiddleButton:
            self._pan_start = None

    def wheelEvent(self, event):
        delta = event.angleDelta().y()
        if delta == 0:
            return
        factor = 1.15 if delta > 0 else 0.87
        self._apply_zoom(self._zoom * factor, anchor=event.position().toPoint())

    def keyPressEvent(self, event):
        if event.key() == Qt.Key.Key_Up:
            self.zoom_in()
        elif event.key() == Qt.Key.Key_Down:
            self.zoom_out()
        else:
            super().keyPressEvent(event)

    def paintEvent(self, event):
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)
        painter.fillRect(self.rect(), QColor(30, 30, 30))

        if self._orig.isNull():
            return

        transform = self._build_transform()
        painter.setTransform(transform)
        painter.drawPixmap(0, 0, self._orig)

        # Dashed line between the two points
        if len(self._clicks) == 2:
            ax, ay = self._clicks[0]
            bx, by = self._clicks[1]
            pen = QPen(QColor(255, 220, 0), 2.0 / self._zoom, Qt.PenStyle.DashLine)
            painter.setPen(pen)
            painter.drawLine(ax, ay, bx, by)

        # Click markers
        r = max(2, int(POINT_RADIUS / self._zoom))
        dot_pen = QPen(QColor(255, 80, 80), 1.5 / self._zoom)
        dot_brush = QBrush(QColor(255, 80, 80, 180))
        font = QFont()
        font.setBold(True)
        font.setPointSize(max(1, int(9 / self._zoom)))
        painter.setFont(font)

        for i, (ix, iy) in enumerate(self._clicks):
            painter.setPen(dot_pen)
            painter.setBrush(dot_brush)
            painter.drawEllipse(ix - r, iy - r, r * 2, r * 2)
            painter.setPen(QPen(Qt.GlobalColor.white))
            painter.drawText(ix + r + 1, iy + r // 2, str(i + 1))

    def resizeEvent(self, event):
        super().resizeEvent(event)
        # On first meaningful resize fit the image
        if self._zoom == 1.0:
            self.zoom_fit()

    # -------------------------------------------------------- data

    def reset(self):
        self._clicks.clear()
        self.points_changed.emit([])
        self.update()

    def pixel_distance(self) -> float | None:
        if len(self._clicks) < 2:
            return None
        dx = self._clicks[1][0] - self._clicks[0][0]
        dy = self._clicks[1][1] - self._clicks[0][1]
        return math.sqrt(dx * dx + dy * dy)


class CalibrationDialog(QDialog):
    """
    Modal dialog for setting the pixel-per-unit scale of an image.

    Emits calibration_applied(scale_factor, scale_unit, apply_to_all)
    where scale_factor = pixels per unit.
    """

    calibration_applied = pyqtSignal(float, str, bool)  # scale_factor, unit, apply_all

    def __init__(self, annotation: ImageAnnotation, parent=None):
        super().__init__(parent)
        self._ann = annotation
        self.setWindowTitle("Calibrate Image Scale")
        self.setModal(True)
        self.resize(700, 820)
        self.setMinimumSize(500, 600)

        pixmap = QPixmap(annotation.image_path)
        if pixmap.isNull():
            pixmap = QPixmap(PREVIEW_MAX_SIZE, PREVIEW_MAX_SIZE)
            pixmap.fill(QColor(40, 40, 40))

        self._canvas = _CalibCanvas(pixmap, self)
        self._canvas.points_changed.connect(self._on_points_changed)

        # Zoom controls
        zoom_row = QHBoxLayout()
        btn_zoom_in = QPushButton("↑  Zoom In")
        btn_zoom_out = QPushButton("↓  Zoom Out")
        btn_zoom_fit = QPushButton("Fit")
        for b in (btn_zoom_in, btn_zoom_out, btn_zoom_fit):
            b.setFixedHeight(26)
            zoom_row.addWidget(b)
        btn_zoom_in.clicked.connect(self._canvas.zoom_in)
        btn_zoom_out.clicked.connect(self._canvas.zoom_out)
        btn_zoom_fit.clicked.connect(self._canvas.zoom_fit)

        # Controls
        self._lbl_instruction = QLabel(
            "Step 1: Click two points on a known feature (e.g. scale bar or ruler).\n"
            "Step 2: Enter the real-world distance between those points."
        )
        self._lbl_instruction.setWordWrap(True)

        self._lbl_px_dist = QLabel("Pixel distance: —")

        dist_layout = QHBoxLayout()
        self._spin_dist = QDoubleSpinBox()
        self._spin_dist.setRange(0.01, 100000)
        self._spin_dist.setDecimals(2)
        self._spin_dist.setValue(50.0)
        self._spin_dist.setSuffix("")
        self._spin_dist.valueChanged.connect(self._update_preview)

        self._combo_unit = QComboBox()
        self._combo_unit.addItems(["cm", "m"])
        idx = 0 if annotation.scale_unit == "cm" else 1
        self._combo_unit.setCurrentIndex(idx)
        self._combo_unit.currentTextChanged.connect(self._update_preview)

        dist_layout.addWidget(QLabel("Real-world distance:"))
        dist_layout.addWidget(self._spin_dist)
        dist_layout.addWidget(self._combo_unit)
        dist_layout.addStretch()

        self._lbl_scale_preview = QLabel("Scale factor: —")
        self._lbl_area_preview = QLabel("Photo area: —")

        self._chk_apply_all = QCheckBox("Apply this scale to all images in this station")
        self._chk_apply_all.setChecked(False)

        btn_reset = QPushButton("Reset points")
        btn_reset.clicked.connect(self._canvas.reset)

        self._buttons = QDialogButtonBox(
            QDialogButtonBox.StandardButton.Ok | QDialogButtonBox.StandardButton.Cancel
        )
        _ok_btn = self._buttons.button(QDialogButtonBox.StandardButton.Ok)
        if _ok_btn is not None:
            _ok_btn.setText("Apply Calibration")
            _ok_btn.setEnabled(False)
        self._buttons.accepted.connect(self._apply)
        self._buttons.rejected.connect(self.reject)

        # Pre-fill existing calibration
        if annotation.scale_factor > 1.0:
            self._lbl_scale_preview.setText(
                f"Current: {annotation.scale_factor:.2f} px/{annotation.scale_unit}"
            )

        layout = QVBoxLayout(self)
        layout.addWidget(self._lbl_instruction)
        layout.addLayout(zoom_row)
        layout.addWidget(self._canvas)
        layout.addWidget(self._lbl_px_dist)
        layout.addLayout(dist_layout)
        layout.addWidget(self._lbl_scale_preview)
        layout.addWidget(self._lbl_area_preview)
        layout.addWidget(btn_reset)
        layout.addWidget(self._chk_apply_all)
        layout.addWidget(self._buttons)
        self.setLayout(layout)

    def _on_points_changed(self, pts: list):
        px_dist = self._canvas.pixel_distance()
        _ok_btn = self._buttons.button(QDialogButtonBox.StandardButton.Ok)
        if px_dist is not None:
            self._lbl_px_dist.setText(f"Pixel distance: {px_dist:.1f} px")
            if _ok_btn is not None:
                _ok_btn.setEnabled(True)
        else:
            self._lbl_px_dist.setText("Pixel distance: —")
            if _ok_btn is not None:
                _ok_btn.setEnabled(False)
        self._update_preview()

    def _update_preview(self):
        px_dist = self._canvas.pixel_distance()
        if px_dist is None or px_dist == 0:
            self._lbl_scale_preview.setText("Scale factor: —")
            self._lbl_area_preview.setText("Photo area: —")
            return

        real_dist = self._spin_dist.value()
        unit = self._combo_unit.currentText()
        sf = px_dist / real_dist
        self._lbl_scale_preview.setText(f"Scale factor: {sf:.3f} px/{unit}")

        w = self._ann.image_width
        h = self._ann.image_height
        if w and h and sf > 0:
            area = (w / sf) * (h / sf)
            self._lbl_area_preview.setText(
                f"Photo area: {area:.2f} {unit}² "
                f"({w/sf:.1f} × {h/sf:.1f} {unit})"
            )
        else:
            self._lbl_area_preview.setText("Photo area: —")

    def _apply(self):
        px_dist = self._canvas.pixel_distance()
        if not px_dist or px_dist == 0:
            return
        real_dist = self._spin_dist.value()
        unit = self._combo_unit.currentText()
        sf = px_dist / real_dist
        apply_all = self._chk_apply_all.isChecked()
        self.calibration_applied.emit(sf, unit, apply_all)
        self.accept()
