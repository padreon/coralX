from PyQt6.QtWidgets import QWidget, QMenu
from PyQt6.QtCore import Qt, QPoint, QTimer, pyqtSignal
from PyQt6.QtGui import QPainter, QPixmap, QColor, QPen, QFont, QTransform, QPolygon, QImage, QPainterPath
import cv2
import numpy as np

from src.models.project import Point, ImageAnnotation, Measurement
from src.core import measurement_tools

POINT_RADIUS = 8
LABEL_FONT_SIZE = 9
COLOR_UNLABELED = QColor(255, 80, 80, 220)
COLOR_LABELED = QColor(80, 220, 80, 220)
COLOR_SELECTED = QColor(255, 220, 0, 255)


class ImageCanvas(QWidget):
    point_labeled = pyqtSignal(int, str)          # point index, label
    point_selected = pyqtSignal(int)              # emitted when selection changes
    border_defined = pyqtSignal(int, int, int, int)  # x_min, y_min, x_max, y_max
    border_polygon_defined = pyqtSignal(list)     # [[x, y], ...] polygon points
    status_message = pyqtSignal(str)
    measurement_drawn = pyqtSignal(object)        # emits a raw Measurement (no label yet)
    preview_updated = pyqtSignal(dict)            # live stats while drawing/selecting

    def __init__(self, parent=None):
        super().__init__(parent)
        self.setMouseTracking(True)
        self.setFocusPolicy(Qt.FocusPolicy.StrongFocus)

        self._pixmap: QPixmap | None = None
        self._annotation: ImageAnnotation | None = None
        self._coral_codes: dict = {}

        self._zoom = 1.0
        self._offset = QPoint(0, 0)
        self._pan_start: QPoint | None = None
        self._selected_index: int | None = None
        self._border: int = 0
        self._border_rect: tuple | None = None    # (x_min, y_min, x_max, y_max) when set by click
        self._drawn_polygon: list | None = None   # [[x, y], ...] stored polygon
        self._border_mode: str | None = None      # '2point', '4point', or 'polygon'
        self._border_clicks: list = []

        # Keyboard shortcut labeling buffer
        self._key_buffer: str = ""
        self._key_timer = QTimer(self)
        self._key_timer.setSingleShot(True)
        self._key_timer.setInterval(700)
        self._key_timer.timeout.connect(self._clear_key_buffer)

        # Measurement drawing state
        self._measure_mode: str = ""           # "" | "line" | "polyline" | "polygon" | "magic"
        self._measure_clicks: list[tuple[float, float]] = []
        self._measure_tolerance: int = 32
        self._measure_smoothing: int = 0           # 0=detailed, 100=simplified
        self._measurements: list[Measurement] = []  # rendered overlays
        self._magic_preview: list[tuple[float, float]] | None = None  # largest outer (for stats/bbox)
        # All disconnected regions: list of (outer_pts, [hole_pts, ...])
        self._magic_regions: list[tuple[list, list]] = []
        self._magic_mask: "np.ndarray | None" = None  # accumulated flood-fill mask for union
        # Undo snapshots for in-progress magic-wand clicks: (mask_copy, clicks_copy)
        self._magic_history: list[tuple] = []
        # Manual eraser brush (scrubs pixels out of the magic-wand mask by hand)
        self._eraser_active: bool = False
        self._eraser_radius: int = 24              # brush radius, image-space px
        self._erasing: bool = False                # left button held, scrubbing
        self._eraser_cursor: tuple[float, float] | None = None  # brush outline pos
        self._measure_ready: bool = False          # shape drawn, waiting for Enter to confirm

    # ------------------------------------------------------------------ public

    def load_image(self, annotation: ImageAnnotation, coral_codes: dict):
        self._annotation = annotation
        self._coral_codes = coral_codes
        img = cv2.imread(annotation.image_path)
        if img is None:
            self.status_message.emit(f"Cannot load: {annotation.image_path}")
            return
        img_rgb = cv2.cvtColor(img, cv2.COLOR_BGR2RGB)
        h, w, ch = img_rgb.shape
        qimg = QImage(bytes(img_rgb.data), w, h, ch * w, QImage.Format.Format_RGB888)
        self._pixmap = QPixmap.fromImage(qimg)
        self._zoom = 1.0
        self._offset = QPoint(0, 0)
        self._selected_index = None
        self.update()
        self.status_message.emit(f"Loaded: {annotation.image_path}  |  {len(annotation.points)} points")

    def set_zoom(self, factor: float):
        self._zoom = max(0.1, min(10.0, factor))
        self.update()

    def zoom_in(self):
        self.set_zoom(self._zoom * 1.25)

    def zoom_out(self):
        self.set_zoom(self._zoom / 1.25)
    def zoom_fit(self):
        if self._pixmap:
            w_ratio = self.width() / self._pixmap.width()
            h_ratio = self.height() / self._pixmap.height()
            self.set_zoom(min(w_ratio, h_ratio) * 0.95)
            self._offset = QPoint(0, 0)
            self.update()

    def set_border(self, border: int):
        self._border = border
        self.update()

    def set_border_rect(self, rect: tuple | None):
        self._border_rect = rect
        if rect is not None:
            self._drawn_polygon = None
        self.update()

    def set_border_polygon(self, polygon: list | None):
        self._drawn_polygon = polygon
        if polygon is not None:
            self._border_rect = None
        self.update()

    def start_border_drawing(self, mode: str):
        self._border_mode = mode
        self._border_clicks = []
        self.setCursor(Qt.CursorShape.CrossCursor)
        if mode == 'polygon':
            self.status_message.emit("Click 4 points to define polygon border  (ESC to cancel)")
        else:
            required = 2 if mode == '2point' else 4
            self.status_message.emit(f"Click {required} points to define border  (ESC to cancel)")
        self.update()

    def cancel_border_drawing(self):
        self._border_mode = None
        self._border_clicks = []
        self.setCursor(Qt.CursorShape.ArrowCursor)
        self.update()

    # -------------------------------------------------- measurement public API

    def start_measurement(self, mode: str) -> None:
        """Begin a measurement drawing session. mode: 'line'|'polyline'|'polygon'|'magic'."""
        self._measure_mode = mode
        self._measure_clicks = []
        self._magic_history = []
        self._erasing = False
        self._eraser_cursor = None
        self._border_mode = None
        self._border_clicks = []
        self.setCursor(Qt.CursorShape.CrossCursor)
        hints = {
            "line": "Click 2 points to measure length  (ESC to cancel)",
            "polyline": "Click points along coral, double-click to finish  (ESC to cancel)",
            "polygon": "Click to outline coral area, double-click to finish  (ESC to cancel)",
            "magic": "Click on coral to auto-select area  (ESC to cancel)",
        }
        self.status_message.emit(hints.get(mode, "Draw measurement  (ESC to cancel)"))
        self.update()

    def cancel_measurement(self) -> None:
        self._measure_mode = ""
        self._measure_clicks = []
        self._magic_preview = None
        self._magic_regions = []
        self._magic_mask = None
        self._magic_history = []
        self._erasing = False
        self._eraser_cursor = None
        self._measure_ready = False
        self.preview_updated.emit({})
        self.setCursor(Qt.CursorShape.ArrowCursor)
        self.update()

    def set_measure_tolerance(self, val: int) -> None:
        self._measure_tolerance = val

    def set_measure_smoothing(self, val: int) -> None:
        self._measure_smoothing = val

    def set_eraser_active(self, on: bool) -> None:
        """Toggle the manual eraser brush (only meaningful in magic-wand mode)."""
        self._eraser_active = on
        self._erasing = False
        self._eraser_cursor = None
        if self._measure_mode == "magic":
            if on:
                self.status_message.emit(
                    "Eraser on — drag over the selection to scrub it away  "
                    "(toggle off to add again)")
            else:
                self.status_message.emit(
                    "Eraser off — click coral to add / right-click to subtract")
        self.update()

    def set_eraser_radius(self, px: int) -> None:
        self._eraser_radius = max(1, int(px))
        self.update()

    def set_measurements(self, measurements: list[Measurement]) -> None:
        self._measurements = measurements
        self.update()

    def select_point(self, index: int):
        if not self._annotation or not self._annotation.points:
            return
        points = self._annotation.points
        if 0 <= index < len(points):
            self._selected_index = index
            self._center_on_point(points[index])
            self.update()
            self.point_selected.emit(index)

    def set_selected_index(self, index: int):
        """Update selection highlight without panning."""
        self._selected_index = index
        self.update()

    def _center_on_point(self, p: Point):
        if not self._pixmap:
            return
        self._offset = QPoint(
            int(self._zoom * (self._pixmap.width() / 2 - p.x)),
            int(self._zoom * (self._pixmap.height() / 2 - p.y)),
        )

    # ------------------------------------------------------------------ paint

    def paintEvent(self, event):  # pylint: disable=unused-argument
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)

        painter.fillRect(self.rect(), QColor(30, 30, 30))

        if not self._pixmap:
            painter.setPen(QColor(150, 150, 150))
            painter.drawText(self.rect(), Qt.AlignmentFlag.AlignCenter, "Open an image to begin")
            return

        transform = self._build_transform()
        painter.setTransform(transform)
        painter.drawPixmap(0, 0, self._pixmap)

        if self._drawn_polygon and len(self._drawn_polygon) >= 3:
            pts = [QPoint(int(x), int(y)) for x, y in self._drawn_polygon]
            pen = QPen(QColor(255, 200, 0, 200), 2.0 / self._zoom, Qt.PenStyle.DashLine)
            painter.setPen(pen)
            painter.setBrush(QColor(255, 200, 0, 25))
            painter.drawPolygon(QPolygon(pts))
        elif self._border_rect:
            x0, y0, x1, y1 = self._border_rect
            pen = QPen(QColor(255, 200, 0, 200), 2.0 / self._zoom, Qt.PenStyle.DashLine)
            painter.setPen(pen)
            painter.setBrush(QColor(0, 0, 0, 0))
            painter.drawRect(x0, y0, x1 - x0, y1 - y0)
        elif self._border > 0:
            b = self._border
            pen = QPen(QColor(255, 200, 0, 200), 2.0 / self._zoom, Qt.PenStyle.DashLine)
            painter.setPen(pen)
            painter.setBrush(QColor(0, 0, 0, 0))
            painter.drawRect(b, b, self._pixmap.width() - 2 * b, self._pixmap.height() - 2 * b)

        if self._border_clicks:
            r = max(4, int(6.0 / self._zoom))
            for pt in self._border_clicks:
                painter.setBrush(QColor(0, 200, 255, 220))
                painter.setPen(QPen(QColor(0, 0, 0, 180), 1.0 / self._zoom))
                painter.drawEllipse(pt.x() - r, pt.y() - r, r * 2, r * 2)
            if self._border_mode == 'polygon' and len(self._border_clicks) >= 2:
                painter.setBrush(QColor(0, 0, 0, 0))
                painter.setPen(QPen(QColor(0, 200, 255, 200), 2.0 / self._zoom, Qt.PenStyle.SolidLine))
                for i in range(1, len(self._border_clicks)):
                    painter.drawLine(self._border_clicks[i - 1], self._border_clicks[i])
            elif len(self._border_clicks) >= 2:
                xs = [p.x() for p in self._border_clicks]
                ys = [p.y() for p in self._border_clicks]
                painter.setBrush(QColor(0, 200, 255, 30))
                painter.setPen(QPen(QColor(0, 200, 255, 200), 2.0 / self._zoom, Qt.PenStyle.DashLine))
                painter.drawRect(int(min(xs)), int(min(ys)),
                                 int(max(xs) - min(xs)), int(max(ys) - min(ys)))

        if self._measurements:
            self._draw_measurements(painter)

        if self._magic_preview:
            self._draw_magic_preview(painter)

        if self._measure_clicks and not self._magic_preview:
            self._draw_measure_preview(painter)

        if (self._measure_mode == "magic" and self._eraser_active
                and self._eraser_cursor is not None):
            self._draw_eraser_cursor(painter)

        if self._annotation:
            self._draw_points(painter)

    def _build_transform(self) -> QTransform:
        cx = self.width() / 2 + self._offset.x()
        cy = self.height() / 2 + self._offset.y()
        t = QTransform()
        t.translate(cx, cy)
        t.scale(self._zoom, self._zoom)
        if self._pixmap:
            t.translate(-self._pixmap.width() / 2, -self._pixmap.height() / 2)
        return t

    def _adaptive_point_r(self) -> float:
        """Screen-space radius: tapers with sqrt(zoom) so points shrink at overview
        zoom and are full size when zoomed in; also caps by point density."""
        # sqrt(zoom) curve: full POINT_RADIUS at zoom≥1, smooth shrink below
        r = max(2.5, min(float(POINT_RADIUS), POINT_RADIUS * self._zoom ** 0.5))
        # Secondary cap: prevent literal overlap when points are very dense
        ann = self._annotation
        if ann and len(ann.points) > 1 and self._pixmap:
            area = self._pixmap.width() * self._pixmap.height()
            avg_spacing = (area / len(ann.points)) ** 0.5 * self._zoom
            r = min(r, max(2.5, avg_spacing * 0.35))
        return r

    def _draw_points(self, painter: QPainter):
        assert self._annotation is not None
        r_screen = self._adaptive_point_r()
        r = r_screen / self._zoom          # image-space radius
        show_label = r_screen >= 6         # hide text when points are very small

        font_size = max(1, round(r_screen * 1.1 / self._zoom))
        font = QFont("Arial", font_size)
        font.setBold(True)
        painter.setFont(font)

        for p in self._annotation.points:
            is_selected = p.index == self._selected_index
            color = COLOR_SELECTED if is_selected else (COLOR_LABELED if p.label else COLOR_UNLABELED)

            pen = QPen(QColor(0, 0, 0, 180), 1.5 / self._zoom)
            painter.setPen(pen)
            painter.setBrush(color)
            painter.drawEllipse(
                int(p.x - r), int(p.y - r),
                int(r * 2), int(r * 2)
            )

            if p.label and show_label:
                painter.setPen(QPen(QColor(0, 0, 0)))
                painter.drawText(
                    int(p.x + r + 2 / self._zoom),
                    int(p.y + 4 / self._zoom),
                    p.label[:6],
                )

    def _draw_measurements(self, painter: QPainter):
        mcolor = QColor(0, 220, 255, 230)
        mpen = QPen(mcolor, 2.0 / self._zoom)
        font = QFont("Arial", max(1, int(10 / self._zoom)))
        font.setBold(True)
        painter.setFont(font)

        for m in self._measurements:
            pts = [QPoint(int(x), int(y)) for x, y in m.points]
            if not pts:
                continue
            painter.setPen(mpen)
            painter.setBrush(QColor(0, 0, 0, 0))

            if m.type == "line" and len(pts) >= 2:
                painter.drawLine(pts[0], pts[1])
                mid_x = (pts[0].x() + pts[1].x()) // 2
                mid_y = (pts[0].y() + pts[1].y()) // 2
                unit_sq = "²" if m.type == "polygon" else ""
                label = f"{m.label}: {m.value:.2f} {m.unit}{unit_sq}"
                painter.setPen(QPen(QColor(255, 255, 255)))
                painter.drawText(mid_x + int(4 / self._zoom), mid_y, label)
            elif m.type == "polyline" and len(pts) >= 2:
                for i in range(1, len(pts)):
                    painter.drawLine(pts[i - 1], pts[i])
                mid_x = sum(p.x() for p in pts) // len(pts)
                mid_y = sum(p.y() for p in pts) // len(pts)
                painter.setPen(QPen(QColor(255, 255, 255)))
                painter.drawText(mid_x + int(4 / self._zoom), mid_y,
                                 f"{m.label}: {m.value:.2f} {m.unit}")
            elif m.type == "polygon" and len(pts) >= 3:
                painter.setBrush(QColor(0, 220, 255, 35))
                painter.drawPolygon(QPolygon(pts))
                cx = sum(p.x() for p in pts) // len(pts)
                cy = sum(p.y() for p in pts) // len(pts)
                painter.setPen(QPen(QColor(255, 255, 255)))
                painter.drawText(cx + int(4 / self._zoom), cy,
                                 f"{m.label}: {m.value:.2f} {m.unit}²")

    def _draw_measure_preview(self, painter: QPainter):
        pts = [QPoint(int(x), int(y)) for x, y in self._measure_clicks]
        r = max(3, int(5.0 / self._zoom))
        painter.setBrush(QColor(0, 220, 255, 200))
        painter.setPen(QPen(QColor(0, 0, 0, 160), 1.0 / self._zoom))
        for pt in pts:
            painter.drawEllipse(pt.x() - r, pt.y() - r, r * 2, r * 2)
        if len(pts) >= 2:
            painter.setBrush(QColor(0, 0, 0, 0))
            painter.setPen(QPen(QColor(0, 220, 255, 200), 2.0 / self._zoom))
            for i in range(1, len(pts)):
                painter.drawLine(pts[i - 1], pts[i])

        # Bounding box lines shown only when shape is complete (waiting for Enter)
        if self._measure_ready and len(pts) >= 2:
            ann = self._annotation
            unit = ann.scale_unit if ann else "cm"
            scale = ann.scale_factor if ann and ann.scale_factor > 1.0 else 1.0
            if self._measure_mode == "polygon":
                # Area shape → tilt-aware oriented box (height along the lean).
                self._draw_oriented_bbox(painter, self._measure_clicks, scale, unit)
            else:
                w, h = measurement_tools.bounding_box(self._measure_clicks, scale)
                self._draw_bbox_lines(painter, pts, w, h, unit)

    def _draw_eraser_cursor(self, painter: QPainter):
        """Outline of the eraser brush at the cursor (image-space radius)."""
        cx, cy = self._eraser_cursor
        r = max(1, int(self._eraser_radius))
        painter.setBrush(QColor(255, 60, 60, 40))
        painter.setPen(QPen(QColor(255, 70, 70, 230), 1.5 / self._zoom,
                            Qt.PenStyle.DashLine))
        painter.drawEllipse(QPoint(int(cx), int(cy)), r, r)

    def _draw_magic_preview(self, painter: QPainter):
        if not self._magic_preview:
            return
        outer_pts = [QPoint(int(x), int(y)) for x, y in self._magic_preview]
        if len(outer_pts) < 3:
            return

        # Build a QPainterPath with OddEvenFill so that hole subpaths
        # visually punch through the fill — rendering a true donut shape.
        path = QPainterPath()
        path.setFillRule(Qt.FillRule.OddEvenFill)

        # All disconnected regions (each with its own holes)
        for outer, holes in self._magic_regions:
            if len(outer) < 3:
                continue
            path.moveTo(outer[0][0], outer[0][1])
            for x, y in outer[1:]:
                path.lineTo(x, y)
            path.closeSubpath()
            for hole in holes:
                if len(hole) >= 3:
                    path.moveTo(hole[0][0], hole[0][1])
                    for x, y in hole[1:]:
                        path.lineTo(x, y)
                    path.closeSubpath()

        painter.setBrush(QColor(255, 180, 0, 70))
        painter.setPen(QPen(QColor(255, 200, 0, 240), 2.5 / self._zoom))
        painter.drawPath(path)

        # Oriented box: height/width follow the coral's tilt, not the photo axes.
        ann = self._annotation
        unit = ann.scale_unit if ann else "cm"
        scale = ann.scale_factor if ann and ann.scale_factor > 1.0 else 1.0
        self._draw_oriented_bbox(painter, self._magic_preview, scale, unit)

    def _draw_oriented_bbox(self, painter: QPainter,
                            pts: list[tuple[float, float]],
                            scale: float, unit: str) -> None:
        """Draw the min-area rotated box plus tilt-aligned height/width axes."""
        axes = measurement_tools.oriented_axes(pts)
        if not axes:
            return
        w_real, h_real, angle = measurement_tools.oriented_extent(pts, scale)
        unit_str = unit if scale > 1.0 else "px"

        lw = 1.5 / self._zoom
        fs = max(1, int(9 / self._zoom))
        font = QFont("Arial", fs)
        font.setBold(True)
        painter.setFont(font)

        # Faint rotated rectangle outline.
        corners = [QPoint(int(x), int(y)) for x, y in axes["corners"]]
        painter.setBrush(Qt.BrushStyle.NoBrush)
        painter.setPen(QPen(QColor(255, 255, 255, 90), lw, Qt.PenStyle.DashLine))
        painter.drawPolygon(QPolygon(corners))

        def draw_axis(line, color, label, label_end):
            """Draw the axis line + end ticks, and place its label at ONE end
            (offset outward) so the H and W labels never stack at the centre.
            label_end picks which endpoint anchors the label: 'min_y' = topmost,
            'max_x' = rightmost."""
            (x1, y1), (x2, y2) = line
            p1 = QPoint(int(x1), int(y1))
            p2 = QPoint(int(x2), int(y2))
            painter.setPen(QPen(color, lw))
            painter.drawLine(p1, p2)
            # End ticks perpendicular to the axis.
            dx, dy = x2 - x1, y2 - y1
            ln = max(1e-6, (dx * dx + dy * dy) ** 0.5)
            tick = 6 / self._zoom
            nx, ny = -dy / ln * tick, dx / ln * tick
            painter.drawLine(QPoint(int(x1 - nx), int(y1 - ny)),
                             QPoint(int(x1 + nx), int(y1 + ny)))
            painter.drawLine(QPoint(int(x2 - nx), int(y2 - ny)),
                             QPoint(int(x2 + nx), int(y2 + ny)))
            # Anchor the label at one endpoint, pushed outward from the centre.
            if label_end == "min_y":
                ex, ey = (x1, y1) if y1 <= y2 else (x2, y2)
            else:  # "max_x"
                ex, ey = (x1, y1) if x1 >= x2 else (x2, y2)
            cx, cy = (x1 + x2) / 2.0, (y1 + y2) / 2.0
            ox, oy = ex - cx, ey - cy
            oln = max(1e-6, (ox * ox + oy * oy) ** 0.5)
            off = 8 / self._zoom
            painter.drawText(QPoint(int(ex + ox / oln * off + 3 / self._zoom),
                                    int(ey + oy / oln * off)), label)

        # Height = long axis (green), label at the top end; width = short axis
        # (yellow), label at the right end — keeps the two labels apart.
        angle_txt = f"  ({angle:+.0f}°)" if abs(angle) > 1e-6 else ""
        draw_axis(axes["height_line"], QColor(100, 255, 140, 235),
                  f"H: {h_real:.1f} {unit_str}{angle_txt}", "min_y")
        draw_axis(axes["width_line"], QColor(255, 220, 0, 235),
                  f"W: {w_real:.1f} {unit_str}", "max_x")

    def _draw_bbox_lines(self, painter: QPainter,
                         pts: list[QPoint],
                         width_real: float, height_real: float,
                         unit: str) -> None:
        """
        Height: vertical line from the actual bottommost contour point upward.
        Width:  horizontal line at the lower of leftmost/rightmost points;
                dashed connector drawn from the higher one down to that level.
        """
        tick = int(6 / self._zoom)
        lw   = 1.5 / self._zoom
        dash_lw = max(0.8, 0.8 / self._zoom)
        fs   = max(1, int(9 / self._zoom))
        font = QFont("Arial", fs)
        font.setBold(True)
        painter.setFont(font)

        # Extreme actual contour points
        bottom_pt = max(pts, key=lambda p: p.y())   # lowest on screen  (max y)
        top_pt    = min(pts, key=lambda p: p.y())   # highest on screen (min y)
        left_pt   = min(pts, key=lambda p: p.x())
        right_pt  = max(pts, key=lambda p: p.x())

        # ── Height ── green vertical line, from bottommost point straight up
        h_x      = bottom_pt.x()
        h_y_bot  = bottom_pt.y()
        h_y_top  = top_pt.y()
        h_pen = QPen(QColor(100, 255, 140, 230), lw)
        painter.setPen(h_pen)
        painter.drawLine(h_x, h_y_bot, h_x, h_y_top)
        painter.drawLine(h_x - tick, h_y_bot, h_x + tick, h_y_bot)
        painter.drawLine(h_x - tick, h_y_top, h_x + tick, h_y_top)
        # Dashed horizontal connector from topmost point to the measurement line
        if top_pt.x() != h_x:
            painter.setPen(QPen(QColor(100, 255, 140, 140), dash_lw, Qt.PenStyle.DashLine))
            painter.drawLine(top_pt.x(), h_y_top, h_x, h_y_top)
            painter.setPen(h_pen)
        painter.setPen(QPen(QColor(100, 255, 140)))
        painter.drawText(h_x + int(3 / self._zoom),
                         (h_y_bot + h_y_top) // 2 - int(2 / self._zoom),
                         f"H: {height_real:.1f} {unit}")

        # ── Width ── yellow horizontal line at the lower of leftmost/rightmost y
        w_ref_y  = max(left_pt.y(), right_pt.y())   # lower = higher y value
        left_x   = left_pt.x()
        right_x  = right_pt.x()
        w_pen = QPen(QColor(255, 220, 0, 230), lw)
        dash_pen = QPen(QColor(255, 220, 0, 140), dash_lw, Qt.PenStyle.DashLine)

        painter.setPen(w_pen)
        # Dashed connector from the point that sits ABOVE the reference line
        if left_pt.y() < w_ref_y:
            painter.setPen(dash_pen)
            painter.drawLine(left_x, left_pt.y(), left_x, w_ref_y)
            painter.setPen(w_pen)
        if right_pt.y() < w_ref_y:
            painter.setPen(dash_pen)
            painter.drawLine(right_x, right_pt.y(), right_x, w_ref_y)
            painter.setPen(w_pen)

        # Horizontal width line
        painter.drawLine(left_x, w_ref_y, right_x, w_ref_y)
        painter.drawLine(left_x, w_ref_y - tick, left_x, w_ref_y + tick)
        painter.drawLine(right_x, w_ref_y - tick, right_x, w_ref_y + tick)
        painter.setPen(QPen(QColor(255, 220, 0)))
        painter.drawText((left_x + right_x) // 2 + int(3 / self._zoom),
                         w_ref_y + int(12 / self._zoom),
                         f"W: {width_real:.1f} {unit}")

    # ------------------------------------------------------------------ events

    def mouseDoubleClickEvent(self, event):
        if self._measure_mode == "magic":
            if event.button() == Qt.MouseButton.LeftButton:
                # Double-click resets the entire magic wand selection
                self._magic_preview = None
                self._magic_mask = None
                self._magic_regions = []
                self._measure_clicks = []
                self._magic_history = []
                self.preview_updated.emit({})
                self.status_message.emit("Selection reset — click to start again  (ESC to cancel)")
                self.update()
            return

        if self._measure_mode in ("polyline", "polygon"):
            if event.button() == Qt.MouseButton.LeftButton:
                # The second press of the double-click already added a duplicate point
                # via mousePressEvent — remove it so the last vertex isn't repeated.
                if self._measure_clicks:
                    self._measure_clicks.pop()
                min_pts = 2 if self._measure_mode == "polyline" else 3
                if len(self._measure_clicks) >= min_pts:
                    self._measure_ready = True
                    self._emit_preview_stats()
                    label = "Polyline" if self._measure_mode == "polyline" else "Polygon"
                    self.status_message.emit(
                        f"{label} preview — Enter to confirm / ESC to cancel"
                    )
                    self.update()
            return
        super().mouseDoubleClickEvent(event)

    def mousePressEvent(self, event):
        if self._measure_mode:
            img_pos = self._screen_to_image(event.pos())
            pt = (float(img_pos.x()), float(img_pos.y()))

            if event.button() == Qt.MouseButton.LeftButton:
                if self._measure_mode == "magic":
                    if self._eraser_active:
                        # Left-drag: manually scrub pixels out of the selection.
                        if self._magic_mask is not None:
                            self._push_magic_history()
                            self._erasing = True
                            self._erase_at(pt, live=True)
                    else:
                        # Left-click: add (union) to existing selection
                        self._push_magic_history()
                        self._measure_clicks.append(pt)
                        self._run_magic_preview(pt)
                elif self._measure_mode == "line":
                    if not self._measure_ready:
                        self._measure_clicks.append(pt)
                        if len(self._measure_clicks) == 2:
                            self._measure_ready = True
                            self._emit_preview_stats()
                            self.status_message.emit(
                                "Line preview — Enter to confirm / ESC to cancel"
                            )
                        else:
                            self.status_message.emit("Click a second point  (ESC to cancel)")
                        self.update()
                else:  # polyline / polygon
                    if not self._measure_ready:
                        self._measure_clicks.append(pt)
                        remaining = "line" if self._measure_mode == "polyline" else "polygon"
                        self.status_message.emit(
                            f"{len(self._measure_clicks)} point(s) — double-click to finish {remaining}  (ESC to cancel)"
                        )
                        self.update()
            elif event.button() == Qt.MouseButton.RightButton:
                if self._measure_mode == "magic":
                    # Right-click: subtract the clicked area from the selection
                    self._push_magic_history()
                    self._run_magic_subtract(pt)
            return

        if self._border_mode:
            if event.button() == Qt.MouseButton.LeftButton:
                img_pos = self._screen_to_image(event.pos())
                self._border_clicks.append(img_pos)
                if self._border_mode == 'polygon':
                    remaining = 4 - len(self._border_clicks)
                    if remaining > 0:
                        self.status_message.emit(
                            f"Polygon: {remaining} more click(s) remaining  (ESC to cancel)"
                        )
                    else:
                        self._finish_polygon()
                else:
                    required = 2 if self._border_mode == '2point' else 4
                    remaining = required - len(self._border_clicks)
                    if remaining > 0:
                        self.status_message.emit(
                            f"Border: {remaining} more click(s)  (ESC to cancel)"
                        )
                    else:
                        self._finish_border_drawing()
                self.update()
            return

        if event.button() == Qt.MouseButton.MiddleButton:
            self._pan_start = event.pos()
        elif event.button() == Qt.MouseButton.LeftButton:
            img_pos = self._screen_to_image(event.pos())
            hit = self._hit_point(img_pos)
            if hit is not None:
                self._selected_index = hit.index
                self.point_selected.emit(hit.index)
                self._show_label_menu(event.globalPosition().toPoint(), hit)
                self.update()
        elif event.button() == Qt.MouseButton.RightButton:
            img_pos = self._screen_to_image(event.pos())
            hit = self._hit_point(img_pos)
            if hit:
                self._selected_index = hit.index
                self.point_selected.emit(hit.index)
                self._show_label_menu(event.globalPosition().toPoint(), hit)
                self.update()

    def mouseMoveEvent(self, event):
        if self._pan_start and event.buttons() & Qt.MouseButton.MiddleButton:
            delta = event.pos() - self._pan_start
            self._offset += delta
            self._pan_start = event.pos()
            self.update()

        img_pos = self._screen_to_image(event.pos())

        # Eraser brush: show the brush outline, and scrub while dragging.
        if self._measure_mode == "magic" and self._eraser_active:
            self._eraser_cursor = (float(img_pos.x()), float(img_pos.y()))
            if self._erasing and event.buttons() & Qt.MouseButton.LeftButton:
                self._erase_at(self._eraser_cursor, live=True)
            else:
                self.update()

        if self._annotation and self._pixmap:
            hit = self._hit_point(img_pos)
            if hit:
                label_info = f" — {hit.label}" if hit.label else " — unlabeled"
                self.status_message.emit(
                    f"Point #{hit.index + 1}{label_info}"
                    f"  |  x={int(img_pos.x())}, y={int(img_pos.y())}"
                )
            else:
                self.status_message.emit(f"x={int(img_pos.x())}, y={int(img_pos.y())}")

    def mouseReleaseEvent(self, event):
        if event.button() == Qt.MouseButton.MiddleButton:
            self._pan_start = None
        elif event.button() == Qt.MouseButton.LeftButton and self._erasing:
            self._erasing = False
            # Empty selection after scrubbing → reset the preview cleanly.
            if self._magic_mask is None or not self._magic_mask.any():
                self._magic_preview = None
                self._magic_regions = []
                self.preview_updated.emit({})
            self.status_message.emit(
                "Erased — drag to remove more / toggle eraser off to add  "
                "(Ctrl+Z to undo / Enter to confirm)")
            self.update()

    def wheelEvent(self, event):
        delta = event.angleDelta().y()
        if delta == 0:
            return
        factor = 1.15 if delta > 0 else 0.87
        new_zoom = max(0.1, min(10.0, self._zoom * factor))
        if new_zoom == self._zoom or self._pixmap is None:
            return

        # Keep the image point under the cursor fixed while zooming.
        cursor = event.position()          # QPointF in screen space
        inv, ok = self._build_transform().inverted()
        if ok:
            img = inv.map(cursor)          # cursor position in image space
            pw, ph = self._pixmap.width(), self._pixmap.height()
            # screen = (img - pw/2) * zoom + width/2 + offset
            # solve for new_offset so img maps back to cursor at new_zoom:
            self._offset = QPoint(
                int(cursor.x() - self.width()  / 2 - (img.x() - pw / 2) * new_zoom),
                int(cursor.y() - self.height() / 2 - (img.y() - ph / 2) * new_zoom),
            )
        self._zoom = new_zoom
        self.update()

    def _finish_border_drawing(self):
        xs = [p.x() for p in self._border_clicks]
        ys = [p.y() for p in self._border_clicks]
        w = self._pixmap.width() if self._pixmap else 99999
        h = self._pixmap.height() if self._pixmap else 99999
        x_min = max(0, int(min(xs)))
        y_min = max(0, int(min(ys)))
        x_max = min(w, int(max(xs)))
        y_max = min(h, int(max(ys)))
        self._border_rect = (x_min, y_min, x_max, y_max)
        self._drawn_polygon = None
        self._border_mode = None
        self._border_clicks = []
        self.setCursor(Qt.CursorShape.ArrowCursor)
        self.border_defined.emit(x_min, y_min, x_max, y_max)
        self.update()

    def _finish_polygon(self):
        poly = [[p.x(), p.y()] for p in self._border_clicks]
        self._drawn_polygon = poly
        self._border_rect = None
        self._border_mode = None
        self._border_clicks = []
        self.setCursor(Qt.CursorShape.ArrowCursor)
        self.border_polygon_defined.emit(poly)
        self.update()

    def _contours_from_mask(self, mask: np.ndarray) -> list[tuple[
            list[tuple[float, float]],
            list[list[tuple[float, float]]]]]:
        """Return all disconnected regions as [(outer_pts, [hole_pts, ...]), ...].

        Uses RETR_CCOMP so every outer contour keeps its own holes.
        All outer contours are returned — disconnected regions are preserved.
        """
        h, w = mask.shape[0] - 2, mask.shape[1] - 2
        region = mask[1:h + 1, 1:w + 1]
        contours, hierarchy = cv2.findContours(
            region, cv2.RETR_CCOMP, cv2.CHAIN_APPROX_SIMPLE
        )
        if not contours or hierarchy is None:
            return []

        hier = hierarchy[0]  # shape (N, 4): [next, prev, first_child, parent]

        def _approx(c) -> list[tuple[float, float]]:
            perimeter = cv2.arcLength(c, True)
            eps = 0.5 + (self._measure_smoothing / 100.0) * 0.04 * perimeter
            approx = cv2.approxPolyDP(c, eps, True)
            return [(float(p[0][0]), float(p[0][1])) for p in approx]

        results: list[tuple[list, list]] = []
        for i in range(len(contours)):
            if hier[i][3] != -1:          # skip holes (handled under their parent)
                continue
            if cv2.contourArea(contours[i]) < 9:
                continue
            outer = _approx(contours[i])
            if len(outer) < 3:
                continue

            # Collect holes that are direct children of this outer contour
            holes: list[list[tuple[float, float]]] = []
            child = hier[i][2]             # first child
            while child != -1:
                if cv2.contourArea(contours[child]) >= 9:
                    h_pts = _approx(contours[child])
                    if len(h_pts) >= 3:
                        holes.append(h_pts)
                child = hier[child][0]     # next sibling

            results.append((outer, holes))

        return results

    def _run_magic_subtract(self, seed: tuple[float, float]) -> None:
        """Right-click: flood-fill at seed and subtract that region from the mask."""
        ann = self._annotation
        if not ann or self._magic_mask is None:
            return
        self.status_message.emit("Subtracting area…")
        new_mask = measurement_tools.magic_wand_subtract(
            ann.image_path, int(seed[0]), int(seed[1]),
            self._measure_tolerance, self._magic_mask,
        )
        if new_mask is None:
            return
        self._magic_mask = new_mask
        regions = self._contours_from_mask(new_mask)
        if not regions:
            self._magic_preview = None
            self._magic_regions = []
            self.preview_updated.emit({})
            self.status_message.emit(
                "Selection empty — click to add / double-click to reset  (ESC to cancel)"
            )
            self.update()
            return
        self._magic_regions = regions
        # Keep largest outer contour as _magic_preview for stats / bounding box
        self._magic_preview = max(regions, key=lambda r: len(r[0]))[0]
        self._emit_preview_stats()
        self.status_message.emit(
            "Area removed — click to expand / right-click to remove more / "
            "double-click to reset / Enter to confirm / ESC to cancel"
        )
        self.update()

    def _erase_at(self, pt: tuple[float, float], live: bool) -> None:
        """Scrub a circular brush of pixels out of the magic-wand mask by hand.

        Unlike right-click subtract (which removes a whole image feature), this
        is a plain geometric eraser: it clears exactly the brushed disc, letting
        the user clean up stray bits the wand grabbed. `live` recomputes the
        outline/stats for on-the-fly feedback while dragging.
        """
        mask = self._magic_mask
        if mask is None:
            return
        cx, cy = int(round(pt[0])), int(round(pt[1]))
        # mask is padded by 1px, so image (x, y) -> mask (x+1, y+1).
        cv2.circle(mask, (cx + 1, cy + 1), max(1, int(self._eraser_radius)),
                   0, thickness=-1)
        if not live:
            return
        regions = self._contours_from_mask(mask)
        self._magic_regions = regions
        self._magic_preview = (
            max(regions, key=lambda r: len(r[0]))[0] if regions else None)
        if self._magic_preview:
            self._emit_preview_stats()
        else:
            self.preview_updated.emit({})
        self.update()

    def _push_magic_history(self) -> None:
        """Snapshot the magic-wand state before a click so it can be undone."""
        self._magic_history.append((
            self._magic_mask.copy() if self._magic_mask is not None else None,
            list(self._measure_clicks),
        ))

    def undo_last(self) -> bool:
        """Undo the last in-progress drawing step. Returns True if it did.

        Magic wand → revert the last add/subtract click; line/polyline/polygon →
        remove the last placed point. Returns False when nothing is in progress
        (so the caller can fall back to undoing a committed measurement).
        """
        if not self._measure_mode:
            return False

        if self._measure_mode == "magic":
            if not self._magic_history:
                return False
            mask, clicks = self._magic_history.pop()
            self._measure_clicks = clicks
            self._magic_mask = mask
            if mask is None or not mask.any():
                self._magic_mask = None
                self._magic_preview = None
                self._magic_regions = []
                self.preview_updated.emit({})
                self.status_message.emit(
                    "Selection cleared — click coral to start  (ESC to cancel)")
            else:
                regions = self._contours_from_mask(mask)
                self._magic_regions = regions
                self._magic_preview = (
                    max(regions, key=lambda r: len(r[0]))[0] if regions else None)
                self._emit_preview_stats()
                self.status_message.emit("Undid last selection click  (ESC to cancel)")
            self.update()
            return True

        # line / polyline / polygon: pop the last placed point
        if self._measure_clicks:
            self._measure_clicks.pop()
            min_pts = 3 if self._measure_mode == "polygon" else 2
            if len(self._measure_clicks) < min_pts:
                self._measure_ready = False
            if self._measure_ready:
                self._emit_preview_stats()
            else:
                self.preview_updated.emit({})
            self.status_message.emit(
                f"Undid point — {len(self._measure_clicks)} left  (ESC to cancel)")
            self.update()
            return True

        return False

    def _emit_preview_stats(self) -> None:
        """Compute dimensions for current preview shape and emit preview_updated."""
        ann = self._annotation
        scale = ann.scale_factor if ann and ann.scale_factor > 1.0 else 1.0
        unit = ann.scale_unit if ann else "cm"

        if self._measure_mode == "magic" and self._magic_preview:
            pts = self._magic_preview
        elif self._measure_ready and self._measure_clicks:
            pts = self._measure_clicks
        else:
            self.preview_updated.emit({})
            return

        info: dict = {"unit": unit}
        if self._measure_mode in ("polygon", "magic"):
            # Use mask pixel count for magic wand (handles donut shapes accurately)
            if self._measure_mode == "magic" and self._magic_mask is not None:
                info["area"] = measurement_tools.mask_area(self._magic_mask, scale)
            else:
                info["area"] = measurement_tools.polygon_area(pts, scale, unit)
            info["perimeter"] = measurement_tools.contour_perimeter(pts, scale)
        elif self._measure_mode == "line":
            info["length"] = measurement_tools.line_length(pts[0], pts[1], scale, unit)
        elif self._measure_mode == "polyline":
            info["length"] = measurement_tools.polyline_length(pts, scale, unit)

        if self._measure_mode in ("polygon", "magic"):
            # Tilt-aware: height runs along the coral's principal axis.
            w, h, ang = measurement_tools.oriented_extent(pts, scale)
            info["angle"] = ang
        else:
            w, h = measurement_tools.bounding_box(pts, scale)
        info["width"] = w
        info["height"] = h
        self.preview_updated.emit(info)

    def _run_magic_preview(self, seed: tuple[float, float]) -> None:
        ann = self._annotation
        if not ann:
            return
        self.status_message.emit("Running magic wand…")
        contour, mask = measurement_tools.magic_wand_select(
            ann.image_path, int(seed[0]), int(seed[1]),
            self._measure_tolerance, self._measure_smoothing,
            self._magic_mask,
        )
        if contour and len(contour) >= 3:
            self._magic_mask = mask  # accumulate for next union click
            # Rebuild every region (with holes) from the mask — same as the
            # subtract path — so holes and disconnected regions stay visible
            # after an add click instead of collapsing to one solid blob.
            regions = self._contours_from_mask(mask)
            self._magic_regions = regions if regions else [(contour, [])]
            self._magic_preview = (
                max(regions, key=lambda r: len(r[0]))[0] if regions else contour
            )
            scale = ann.scale_factor if ann.scale_factor > 1.0 else 1.0
            unit = ann.scale_unit
            area = measurement_tools.polygon_area(contour, scale, unit)
            unit_str = f"{unit}²" if ann.scale_factor > 1.0 else "px²"
            click_count = len(self._measure_clicks)
            self.status_message.emit(
                f"Preview ({click_count} click{'s' if click_count != 1 else ''}): "
                f"{area:.2f} {unit_str} — "
                f"click to expand / right-click to remove area / double-click to reset / Enter to confirm / ESC to cancel"
            )
            self._emit_preview_stats()
        else:
            self._magic_mask = mask  # keep accumulated mask even if no new contour
            if not self._magic_preview:
                self.status_message.emit(
                    "No region found — try adjusting tolerance or click elsewhere  (ESC to cancel)"
                )
        self.update()

    def _finish_measurement(self) -> None:
        mode = self._measure_mode
        clicks = self._measure_clicks
        ann = self._annotation

        scale = ann.scale_factor if ann else 1.0
        unit = ann.scale_unit if ann else "cm"

        area = 0.0
        perim = 0.0
        auto_w = 0.0
        auto_h = 0.0
        angle = 0.0

        if mode == "line" and len(clicks) >= 2:
            value = measurement_tools.line_length(clicks[0], clicks[1], scale, unit)
            points = clicks[:2]
            auto_w, auto_h = measurement_tools.bounding_box(points, scale)
        elif mode == "polyline" and len(clicks) >= 2:
            value = measurement_tools.polyline_length(clicks, scale, unit)
            points = clicks[:]
            auto_w, auto_h = measurement_tools.bounding_box(points, scale)
        elif mode == "polygon" and len(clicks) >= 3:
            area = measurement_tools.polygon_area(clicks, scale, unit)
            value = area
            points = clicks[:]
            perim = measurement_tools.contour_perimeter(points, scale)
            # Oriented box → height follows the coral's tilt, not the photo axes.
            auto_w, auto_h, angle = measurement_tools.oriented_extent(points, scale)
        elif mode == "magic" and self._magic_preview:
            contour = self._magic_preview
            # Use mask pixel count for area — accurate even for donut shapes
            if self._magic_mask is not None:
                area = measurement_tools.mask_area(self._magic_mask, scale)
            else:
                area = measurement_tools.polygon_area(contour, scale, unit)
            value = area
            points = contour
            mode = "polygon"
            perim = measurement_tools.contour_perimeter(points, scale)
            # Oriented box → height follows the coral's tilt, not the photo axes.
            auto_w, auto_h, angle = measurement_tools.oriented_extent(points, scale)
            self._magic_preview = None
            self._magic_regions = []
            self._magic_mask = None
        else:
            return

        m = Measurement.new(mode, "", points, value, unit,
                            auto_width=auto_w, auto_height=auto_h,
                            area=area, perimeter_len=perim, angle=angle)
        self._measure_mode = ""
        self._measure_clicks = []
        self._magic_preview = None
        self._magic_mask = None
        self._magic_history = []
        self._measure_ready = False
        self.preview_updated.emit({})
        self.setCursor(Qt.CursorShape.ArrowCursor)
        self.update()
        self.measurement_drawn.emit(m)

    def keyPressEvent(self, event):
        key = event.key()

        # ESC always cancels any active drawing mode
        if key == Qt.Key.Key_Escape:
            if self._measure_mode:
                self.cancel_measurement()
            elif self._border_mode:
                self.cancel_border_drawing()
            self._clear_key_buffer()
            return

        # Enter confirms any pending preview
        if key in (Qt.Key.Key_Return, Qt.Key.Key_Enter):
            if self._measure_mode == "magic" and self._magic_preview:
                self._finish_measurement()
                return
            if self._measure_ready:
                self._finish_measurement()
                return

        # Up/Down arrow zoom (active when no points to cycle, e.g. measurement mode)
        no_points = not self._annotation or not self._annotation.points
        if no_points:
            if key == Qt.Key.Key_Up:
                self.zoom_in()
                return
            if key == Qt.Key.Key_Down:
                self.zoom_out()
                return
            super().keyPressEvent(event)
            return

        points = self._annotation.points  # type: ignore[union-attr]
        n = len(points)
        if key == Qt.Key.Key_Right:
            self._clear_key_buffer()
            idx = (self._selected_index + 1) % n if self._selected_index is not None else 0
            self.select_point(idx)
        elif key == Qt.Key.Key_Left:
            self._clear_key_buffer()
            idx = (self._selected_index - 1) % n if self._selected_index is not None else n - 1
            self.select_point(idx)
        elif key in (Qt.Key.Key_Return, Qt.Key.Key_Enter):
            self._clear_key_buffer()
            if self._selected_index is not None:
                center = self.mapToGlobal(self.rect().center())
                self._show_label_menu(center, points[self._selected_index])
        elif key == Qt.Key.Key_Up:
            self.zoom_in()
        elif key == Qt.Key.Key_Down:
            self.zoom_out()
        else:
            text = event.text().upper()
            has_buf = text and text.isprintable()
            if has_buf and self._coral_codes and self._selected_index is not None:
                self._key_buffer += text
                self._key_timer.start(700)
                self._try_shortcut_label()
            else:
                super().keyPressEvent(event)

    def _clear_key_buffer(self):
        self._key_buffer = ""
        self._key_timer.stop()

    def _try_shortcut_label(self):
        if not self._coral_codes or self._selected_index is None or not self._annotation:
            self._clear_key_buffer()
            return
        buf = self._key_buffer
        matches = [c for c in self._coral_codes if c.startswith(buf)]
        if len(matches) == 1:
            code = matches[0]
            point = self._annotation.points[self._selected_index]
            point.label = code
            self._clear_key_buffer()
            self.point_labeled.emit(self._selected_index, code)
            self.update()
        elif len(matches) == 0:
            self._clear_key_buffer()

    # ------------------------------------------------------------------ helpers

    def _screen_to_image(self, pos: QPoint) -> QPoint:
        transform = self._build_transform()
        inv, ok = transform.inverted()
        if ok:
            mapped = inv.map(pos)
            return QPoint(int(mapped.x()), int(mapped.y()))
        return pos

    def _hit_point(self, img_pos: QPoint) -> Point | None:
        if not self._annotation:
            return None
        threshold = (self._adaptive_point_r() + 2.0) / self._zoom
        for p in self._annotation.points:
            if abs(p.x - img_pos.x()) <= threshold and abs(p.y - img_pos.y()) <= threshold:
                return p
        return None

    def _show_label_menu(self, global_pos: QPoint, point: Point):
        menu = QMenu(self)
        menu.setTitle(f"Point #{point.index + 1}")

        if not self._coral_codes:
            no_code_action = menu.addAction("(No coral codes loaded)")
            if no_code_action is not None:
                no_code_action.setEnabled(False)
        else:
            for code, description in self._coral_codes.items():
                action = menu.addAction(f"{code} — {description}")
                if action is not None:
                    action.setData(code)

        menu.addSeparator()
        clear_action = menu.addAction("Clear label")

        chosen = menu.exec(global_pos)
        if chosen:
            if chosen == clear_action:
                point.label = None
                point.category = None
            elif chosen.data():
                point.label = chosen.data()
                self.point_labeled.emit(point.index, chosen.data())
        self.update()
