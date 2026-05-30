from PyQt6.QtWidgets import QWidget, QMenu
from PyQt6.QtCore import Qt, QPoint, QTimer, pyqtSignal
from PyQt6.QtGui import QPainter, QPixmap, QColor, QPen, QFont, QTransform, QPolygon, QImage
import cv2

from src.models.project import Point, ImageAnnotation

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

    # ------------------------------------------------------------------ events

    def mousePressEvent(self, event):
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

    def keyPressEvent(self, event):
        if not self._annotation or not self._annotation.points:
            if event.key() == Qt.Key.Key_Escape and self._border_mode:
                self.cancel_border_drawing()
            else:
                super().keyPressEvent(event)
            return
        points = self._annotation.points
        key = event.key()

        if key == Qt.Key.Key_Escape:
            if self._border_mode:
                self.cancel_border_drawing()
            self._clear_key_buffer()
            return

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
        threshold = self._adaptive_point_r() / self._zoom + 2
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
