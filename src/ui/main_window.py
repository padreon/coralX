from PyQt6.QtWidgets import (
    QMainWindow, QWidget, QSplitter, QTreeWidget, QTreeWidgetItem,
    QVBoxLayout, QHBoxLayout, QLabel, QPushButton, QStatusBar,
    QFileDialog, QMessageBox, QSpinBox, QComboBox, QProgressBar, QTableWidget, QTableWidgetItem, QToolBar, QDialog,
    QFormLayout, QLineEdit, QDialogButtonBox, QStyledItemDelegate,
    QAbstractItemView, QAbstractItemDelegate, QCompleter, QScrollArea,
    QMenu, QCheckBox, QTextEdit, QTabWidget, QLayout, QSizePolicy,
)
from PyQt6.QtCore import Qt, QThread, QSize, QRect, QPoint, QSettings, pyqtSignal, QTimer, QUrl
from typing import Callable
from PyQt6.QtGui import QIcon, QImage, QPixmap, QDesktopServices
import os

from src.core.logger import get_logger, log_path
from src.ui.image_canvas import ImageCanvas
from src.ui.calibration_dialog import CalibrationDialog
from src.ui.ai_label_dialog import AILabelDialog, AIProgressDialog
from src.ui.import_dialogs import (
    ImportResultDialog, CoralCodesMergeDialog,
    StationMergeDialog, CpceImportDialog,
)
from src.models.project import Project, ImageAnnotation, Station
from src.core.point_generator import generate_points
from src.core.statistics import project_summary
from src.core.ai_labeler import AILabelWorker
from src.core.analysis import photo_area
from src.core.exporter import export_csv, export_excel, export_coral_codes
from src.core.importer import (
    import_coral_codes,
    import_station_metadata,
    import_labeled_points,
    import_cpce_excel,
    import_cpce_cpc,
)


class FlowLayout(QLayout):
    """Wrapping flow layout — arranges items left-to-right, wrapping to new rows."""

    def __init__(self, parent=None, h_spacing: int = 6, v_spacing: int = 4):
        super().__init__(parent)
        self._items: list = []
        self._h = h_spacing
        self._v = v_spacing

    def addItem(self, item) -> None:
        self._items.append(item)

    def count(self) -> int:
        return len(self._items)

    def itemAt(self, index: int):
        return self._items[index] if 0 <= index < len(self._items) else None

    def takeAt(self, index: int):
        return self._items.pop(index) if 0 <= index < len(self._items) else None

    def expandingDirections(self):
        return Qt.Orientation(0)

    def hasHeightForWidth(self) -> bool:
        return True

    def heightForWidth(self, width: int) -> int:
        return self._do_layout(QRect(0, 0, width, 0), dry_run=True)

    def setGeometry(self, rect: QRect) -> None:
        super().setGeometry(rect)
        self._do_layout(rect, dry_run=False)

    def sizeHint(self) -> QSize:
        return self.minimumSize()

    def minimumSize(self) -> QSize:
        size = QSize()
        for item in self._items:
            size = size.expandedTo(item.minimumSize())
        m = self.contentsMargins()
        return size + QSize(m.left() + m.right(), m.top() + m.bottom())

    def _do_layout(self, rect: QRect, dry_run: bool) -> int:
        m = self.contentsMargins()
        r = rect.adjusted(m.left(), m.top(), -m.right(), -m.bottom())
        x, y, row_h = r.x(), r.y(), 0
        for item in self._items:
            iw = item.sizeHint().width()
            ih = item.sizeHint().height()
            if x + iw > r.right() + 1 and row_h > 0:
                x = r.x()
                y += row_h + self._v
                row_h = 0
            if not dry_run:
                item.setGeometry(QRect(QPoint(x, y), item.sizeHint()))
            x += iw + self._h
            row_h = max(row_h, ih)
        return y + row_h - rect.y() + m.bottom()


class _CodesScrollArea(QScrollArea):
    """Scroll area that forces content width = viewport width to drive FlowLayout wrapping."""

    def updateLayout(self) -> None:
        w = self.widget()
        if w is None or w.layout() is None:
            return
        viewport = self.viewport()
        if viewport is None:
            return
        vw = viewport.width()
        layout = w.layout()
        if layout is None:
            return
        h = layout.heightForWidth(vw)
        w.setFixedSize(vw, max(h if h >= 0 else w.sizeHint().height(), 40))

    def resizeEvent(self, event) -> None:
        super().resizeEvent(event)
        self.updateLayout()


class ThumbnailLoader(QThread):
    """Loads 48x48 thumbnails off the main thread and emits QImage (not QPixmap) per path."""
    thumbnail_ready = pyqtSignal(str, QImage)

    def __init__(self, paths: list[str], parent=None):
        super().__init__(parent)
        self._paths = paths

    def run(self):
        from PIL import Image as PILImage
        SIZE = (48, 48)
        for path in self._paths:
            try:
                img = PILImage.open(path)
                img.thumbnail(SIZE, PILImage.Resampling.LANCZOS)
                img = img.convert("RGB")
                w, h = img.size
                data = img.tobytes("raw", "RGB")
                qimg = QImage(data, w, h, w * 3, QImage.Format.Format_RGB888).copy()
                self.thumbnail_ready.emit(path, qimg)
            except Exception:
                pass


class ManageGroupsDialog(QDialog):
    def __init__(self, coral_codes: dict, coral_groups: list, parent=None):
        super().__init__(parent)
        self.setWindowTitle("Manage Code Groups")
        self.setMinimumWidth(500)
        self.resize(520, 360)

        layout = QVBoxLayout(self)

        self._table = QTableWidget(0, 2)
        self._table.setHorizontalHeaderLabels(["Group Name", "Codes (comma-separated)"])
        _header = self._table.horizontalHeader()
        if _header is not None:
            _header.setStretchLastSection(True)
        self._table.setColumnWidth(0, 150)
        layout.addWidget(self._table)

        for g in coral_groups:
            self._add_row(g["name"], ", ".join(g.get("codes", [])))

        btn_row = QHBoxLayout()
        btn_add = QPushButton("+ Add Group")
        btn_add.clicked.connect(lambda: self._add_row("New Group", ""))
        btn_remove = QPushButton("Remove Selected")
        btn_remove.clicked.connect(self._remove_row)
        btn_row.addWidget(btn_add)
        btn_row.addWidget(btn_remove)
        btn_row.addStretch()
        layout.addLayout(btn_row)

        all_codes = set(coral_codes.keys())
        used: set = set()
        for g in coral_groups:
            used.update(g.get("codes", []))
        ungrouped = all_codes - used
        note = f"Ungrouped codes: {', '.join(sorted(ungrouped))}" if ungrouped else "All codes are assigned to a group."
        lbl = QLabel(note)
        lbl.setWordWrap(True)
        lbl.setStyleSheet("color: #888; font-size: 11px;")
        layout.addWidget(lbl)

        btns = QDialogButtonBox(QDialogButtonBox.StandardButton.Ok | QDialogButtonBox.StandardButton.Cancel)
        btns.accepted.connect(self.accept)
        btns.rejected.connect(self.reject)
        layout.addWidget(btns)

    def _add_row(self, name: str, codes: str):
        row = self._table.rowCount()
        self._table.setRowCount(row + 1)
        self._table.setItem(row, 0, QTableWidgetItem(name))
        self._table.setItem(row, 1, QTableWidgetItem(codes))

    def _remove_row(self):
        row = self._table.currentRow()
        if row >= 0:
            self._table.removeRow(row)

    def get_groups(self) -> list:
        groups = []
        for row in range(self._table.rowCount()):
            name_item = self._table.item(row, 0)
            codes_item = self._table.item(row, 1)
            name = name_item.text().strip() if name_item else ""
            codes_text = codes_item.text().strip() if codes_item else ""
            if name:
                codes = [c.strip().upper() for c in codes_text.split(",") if c.strip()]
                groups.append({"name": name, "codes": codes})
        return groups


class LabelEditor(QLineEdit):
    """QLineEdit with autocomplete and Enter/→ navigation to next point."""

    def __init__(self, codes: list[str], on_navigate: Callable[[], None], parent=None):
        super().__init__(parent)
        self._codes = codes
        self._on_navigate = on_navigate

        completer = QCompleter(codes, self)
        completer.setCaseSensitivity(Qt.CaseSensitivity.CaseInsensitive)
        completer.setCompletionMode(QCompleter.CompletionMode.PopupCompletion)
        completer.setFilterMode(Qt.MatchFlag.MatchStartsWith)
        completer.activated.connect(lambda _text: QTimer.singleShot(0, on_navigate))
        self.setCompleter(completer)
        self.textEdited.connect(self._on_text_edited)

    def _on_text_edited(self, text: str):
        upper = text.upper()
        if upper != text:
            self.setText(upper)
            self.setCursorPosition(len(upper))
            text = upper
        if not text:
            return
        matches = [c for c in self._codes if c.upper().startswith(text)]
        if len(matches) == 1 and matches[0] != text:
            self.setText(matches[0])
            self.setSelection(len(text), len(matches[0]) - len(text))

    def keyPressEvent(self, event):
        popup = self.completer().popup() if self.completer() else None
        popup_visible = popup is not None and popup.isVisible()
        key = event.key()
        if not popup_visible and key in (Qt.Key.Key_Return, Qt.Key.Key_Enter):
            self._on_navigate()
        elif not popup_visible and key == Qt.Key.Key_Right and self.cursorPosition() >= len(self.text()):
            self._on_navigate()
        else:
            super().keyPressEvent(event)


class LabelDelegate(QStyledItemDelegate):
    navigate_next = pyqtSignal()

    def __init__(self, coral_codes: dict, parent=None):
        super().__init__(parent)
        self._codes: dict = coral_codes
        self._current_editor: LabelEditor | None = None

    def update_codes(self, coral_codes: dict):
        self._codes = coral_codes

    def createEditor(self, parent, option, index):
        editor = LabelEditor(list(self._codes.keys()), self._trigger_navigate, parent)
        self._current_editor = editor
        return editor

    def _trigger_navigate(self):
        if self._current_editor:
            self.commitData.emit(self._current_editor)
            self.closeEditor.emit(self._current_editor, QAbstractItemDelegate.EndEditHint.NoHint)
        self.navigate_next.emit()

    def setEditorData(self, editor, index):
        editor.setText(index.data(Qt.ItemDataRole.DisplayRole) or "")
        editor.selectAll()

    def setModelData(self, editor, model, index):
        text = editor.text().strip().upper()
        if text not in self._codes:
            text = ""
        model.setData(index, text, Qt.ItemDataRole.EditRole)


class StationMetadataDialog(QDialog):
    def __init__(self, station: Station, parent=None):
        super().__init__(parent)
        self.setWindowTitle("Edit Station")
        self.setMinimumWidth(360)

        layout = QVBoxLayout(self)
        form = QFormLayout()

        self._name_edit = QLineEdit(station.name)
        form.addRow("Name:", self._name_edit)

        self._depth_edit = QLineEdit()
        self._depth_edit.setPlaceholderText("e.g. 5.0")
        if station.depth_m is not None:
            self._depth_edit.setText(str(station.depth_m))
        form.addRow("Depth (m):", self._depth_edit)

        self._date_edit = QLineEdit(station.date or "")
        self._date_edit.setPlaceholderText("YYYY-MM-DD")
        form.addRow("Date:", self._date_edit)

        self._lat_edit = QLineEdit()
        self._lat_edit.setPlaceholderText("e.g. -8.612345")
        if station.gps_lat is not None:
            self._lat_edit.setText(str(station.gps_lat))
        form.addRow("GPS Lat:", self._lat_edit)

        self._lon_edit = QLineEdit()
        self._lon_edit.setPlaceholderText("e.g. 115.223456")
        if station.gps_lon is not None:
            self._lon_edit.setText(str(station.gps_lon))
        form.addRow("GPS Lon:", self._lon_edit)

        self._notes_edit = QTextEdit(station.notes)
        self._notes_edit.setMaximumHeight(72)
        self._notes_edit.setPlaceholderText("Optional notes…")
        form.addRow("Notes:", self._notes_edit)

        layout.addLayout(form)

        btns = QDialogButtonBox(QDialogButtonBox.StandardButton.Ok | QDialogButtonBox.StandardButton.Cancel)
        btns.accepted.connect(self.accept)
        btns.rejected.connect(self.reject)
        layout.addWidget(btns)

    def get_station_data(self) -> dict:
        def _parse_float(text: str) -> float | None:
            try:
                return float(text.strip())
            except (ValueError, AttributeError):
                return None

        return {
            "name": self._name_edit.text().strip() or "Station",
            "depth_m": _parse_float(self._depth_edit.text()),
            "date": self._date_edit.text().strip() or None,
            "gps_lat": _parse_float(self._lat_edit.text()),
            "gps_lon": _parse_float(self._lon_edit.text()),
            "notes": self._notes_edit.toPlainText(),
        }


class MainWindow(QMainWindow):
    def __init__(self):
        super().__init__()
        self.setWindowTitle("coralX — Coral Point Count")
        self.setMinimumSize(1100, 720)

        self._log = get_logger("coralX.ui")
        self.project: Project | None = None
        self._syncing: bool = False
        self._thumbnail_loader: ThumbnailLoader | None = None
        self._ai_worker: AILabelWorker | None = None

        _s = QSettings("coralX", "coralX")

        self._build_ui()
        self._build_menu()
        self._build_toolbar()
        self._build_statusbar()

        h_state = _s.value("ui/h_splitter")
        if h_state:
            self._h_splitter.restoreState(h_state)
        v_state = _s.value("ui/v_splitter")
        if v_state:
            self._v_splitter.restoreState(v_state)
        del _s

        self._new_project()

    # ------------------------------------------------------------ menu & toolbar

    def _build_menu(self):
        menubar = self.menuBar()

        file_menu = menubar.addMenu("&File")
        file_menu.addAction("New Project", "Ctrl+N", self._new_project)
        file_menu.addAction("Open Project…", "Ctrl+O", self._open_project)
        file_menu.addAction("Save Project", "Ctrl+S", self._save_project)
        file_menu.addSeparator()
        file_menu.addAction("Add Images…", "Ctrl+I", self._add_images)
        file_menu.addSeparator()

        import_menu = file_menu.addMenu("Import")
        import_menu.addAction("Coral Codes… (JSON/CSV)", self._import_coral_codes)
        import_menu.addAction("Station Metadata… (CSV)", self._import_station_metadata)
        import_menu.addAction("Labeled Points… (CSV/Excel)", self._import_labeled_points)
        import_menu.addSeparator()
        import_menu.addAction("From CPCe .cpc File(s)…", self._import_cpce_cpc)
        import_menu.addAction("From CPCe Excel…", self._import_cpce_excel)

        file_menu.addSeparator()
        file_menu.addAction("Export CSV…", self._export_csv)
        file_menu.addAction("Export Excel…", self._export_excel)
        file_menu.addAction("Export Coral Codes…", self._export_coral_codes)
        file_menu.addSeparator()
        file_menu.addAction("Exit", self.close)

        image_menu = menubar.addMenu("&Image")
        image_menu.addAction("Calibrate Scale…", self._calibrate_scale)
        image_menu.addSeparator()
        image_menu.addAction("Generate Points (This Image)", self._generate_points_current)
        image_menu.addAction("Generate Points (All Images)", self._generate_points_all)
        image_menu.addSeparator()
        image_menu.addAction("AI Auto-Label…", self._ai_auto_label)

        view_menu = menubar.addMenu("&View")
        view_menu.addAction("Zoom In", "Ctrl++", self.canvas.zoom_in)
        view_menu.addAction("Zoom Out", "Ctrl+-", self.canvas.zoom_out)
        view_menu.addAction("Fit to Window", "Ctrl+0", self.canvas.zoom_fit)

        help_menu = menubar.addMenu("&Help")
        help_menu.addAction("Open Log File…", self._open_log_file)
        help_menu.addSeparator()
        help_menu.addAction("About", self._show_about)

    def _build_toolbar(self):
        tb = QToolBar("Main", self)
        tb.setMovable(False)
        self.addToolBar(tb)

        tb.addAction("➕ Add Images", self._add_images)
        tb.addSeparator()
        tb.addAction("🎯 Generate Points", self._generate_points_current)
        tb.addAction("🎯 Generate All", self._generate_points_all)
        tb.addSeparator()
        tb.addAction("🔍+", self.canvas.zoom_in)
        tb.addAction("🔍−", self.canvas.zoom_out)
        tb.addAction("⊡ Fit", self.canvas.zoom_fit)
        tb.addSeparator()
        tb.addAction("📏 Calibrate", self._calibrate_scale)
        tb.addSeparator()
        tb.addAction("🤖 AI Label", self._ai_auto_label)
        tb.addSeparator()
        tb.addAction("📊 Stats", self._show_stats)
        tb.addAction("💾 Export Excel", self._export_excel)

        # Spacer pushes the progress section to the right
        spacer = QWidget()
        spacer.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Preferred)
        tb.addWidget(spacer)

        self._progress_scope_combo = QComboBox()
        self._progress_scope_combo.addItems(["Image", "Station", "Project"])
        self._progress_scope_combo.setFixedWidth(80)
        self._progress_scope_combo.setToolTip("Progress scope")
        self._progress_scope_combo.currentIndexChanged.connect(self._on_scope_changed)
        tb.addWidget(self._progress_scope_combo)

        self.progress_bar = QProgressBar()
        self.progress_bar.setRange(0, 100)
        self.progress_bar.setValue(0)
        self.progress_bar.setFixedWidth(190)
        self.progress_bar.setFixedHeight(18)
        self.progress_bar.setTextVisible(True)
        self.progress_bar.setFormat("No image")
        tb.addWidget(self.progress_bar)

    # ------------------------------------------------------------------- UI

    def _build_ui(self):
        central = QWidget()
        self.setCentralWidget(central)
        outer = QVBoxLayout(central)
        outer.setContentsMargins(0, 0, 0, 0)
        outer.setSpacing(0)

        self._v_splitter = QSplitter(Qt.Orientation.Vertical)
        outer.addWidget(self._v_splitter, stretch=1)

        self._h_splitter = QSplitter(Qt.Orientation.Horizontal)
        self._v_splitter.addWidget(self._h_splitter)
        h_splitter = self._h_splitter

        # ---- Left panel: progress + station tree + collapsible settings ----
        left = QWidget()
        left.setMinimumWidth(160)
        left_layout = QVBoxLayout(left)
        left_layout.setContentsMargins(8, 8, 8, 8)
        left_layout.setSpacing(4)

        # Images header row with filter checkbox
        img_header = QHBoxLayout()
        img_header.addWidget(QLabel("Images"))
        img_header.addStretch()
        self._filter_checkbox = QCheckBox("Incomplete only")
        self._filter_checkbox.setStyleSheet("font-size: 10px;")
        self._filter_checkbox.stateChanged.connect(self._apply_image_filter)
        img_header.addWidget(self._filter_checkbox)
        left_layout.addLayout(img_header)

        # Station management buttons
        station_btns = QHBoxLayout()
        station_btns.setSpacing(4)
        btn_add_station = QPushButton("+ Station")
        btn_add_station.setToolTip("Add a new station")
        btn_add_station.setFixedHeight(24)
        btn_add_station.clicked.connect(self._add_station)
        btn_edit_station = QPushButton("Edit")
        btn_edit_station.setToolTip("Edit selected station metadata")
        btn_edit_station.setFixedHeight(24)
        btn_edit_station.clicked.connect(self._edit_station_btn)
        station_btns.addWidget(btn_add_station)
        station_btns.addWidget(btn_edit_station)
        station_btns.addStretch()
        left_layout.addLayout(station_btns)

        self.image_tree = QTreeWidget()
        self.image_tree.setHeaderHidden(True)
        self.image_tree.setColumnCount(1)
        self.image_tree.setIconSize(QSize(48, 48))
        self.image_tree.currentItemChanged.connect(self._on_tree_item_changed)
        self.image_tree.setContextMenuPolicy(Qt.ContextMenuPolicy.CustomContextMenu)
        self.image_tree.customContextMenuRequested.connect(self._on_tree_context_menu)
        left_layout.addWidget(self.image_tree)

        # Collapsible Point Settings
        settings_content = QWidget()
        settings_layout = QFormLayout(settings_content)
        settings_layout.setContentsMargins(4, 4, 4, 4)

        self.spin_points = QSpinBox()
        self.spin_points.setRange(1, 500)
        self.spin_points.setValue(10)
        settings_layout.addRow("Count:", self.spin_points)

        self.combo_dist = QComboBox()
        self.combo_dist.addItems(["random", "stratified", "uniform"])
        settings_layout.addRow("Distribution:", self.combo_dist)

        self.spin_border = QSpinBox()
        self.spin_border.setRange(0, 500)
        self.spin_border.setValue(0)
        self.spin_border.setSuffix(" px")
        self.spin_border.valueChanged.connect(self._on_border_spinbox_changed)
        settings_layout.addRow("Border:", self.spin_border)

        border_draw_widget = QWidget()
        border_draw_layout = QHBoxLayout(border_draw_widget)
        border_draw_layout.setContentsMargins(0, 0, 0, 0)
        btn_border_2pt = QPushButton("✏ 2-pt")
        btn_border_4pt = QPushButton("✏ 4-pt")
        btn_border_clear = QPushButton("✕")
        btn_border_clear.setFixedWidth(28)
        btn_border_clear.setToolTip("Clear custom border")
        btn_border_2pt.setToolTip("Click 2 diagonal corners to set rectangular border")
        btn_border_4pt.setToolTip("Click 4 corners to define a quadrilateral border (auto-closes)")
        btn_border_2pt.clicked.connect(lambda: self.canvas.start_border_drawing('2point'))
        btn_border_4pt.clicked.connect(lambda: self.canvas.start_border_drawing('polygon'))
        btn_border_clear.clicked.connect(self._clear_border_rect)
        border_draw_layout.addWidget(btn_border_2pt)
        border_draw_layout.addWidget(btn_border_4pt)
        border_draw_layout.addWidget(btn_border_clear)
        settings_layout.addRow("Draw:", border_draw_widget)

        left_layout.addWidget(self._make_collapsible("Point Settings", settings_content, "ui/settings_collapsed"))

        h_splitter.addWidget(left)

        # ---- Center: canvas ----
        self.canvas = ImageCanvas()
        self.canvas.point_labeled.connect(self._on_point_labeled)
        self.canvas.point_selected.connect(self._on_canvas_point_selected)
        self.canvas.border_defined.connect(self._on_border_defined)
        self.canvas.border_polygon_defined.connect(self._on_border_polygon_defined)
        self.canvas.status_message.connect(self._set_status)
        h_splitter.addWidget(self.canvas)

        # ---- Right panel: quick stats + points table ----
        right = QWidget()
        right.setMinimumWidth(180)
        right_layout = QVBoxLayout(right)
        right_layout.setContentsMargins(8, 8, 8, 8)
        right_layout.setSpacing(4)

        # Collapsible Quick Stats (moved from left panel)
        stats_content = QWidget()
        stats_inner = QVBoxLayout(stats_content)
        stats_inner.setContentsMargins(4, 4, 4, 4)
        self.stats_label = QLabel("—")
        self.stats_label.setWordWrap(True)
        self.stats_label.setAlignment(Qt.AlignmentFlag.AlignTop)
        stats_inner.addWidget(self.stats_label)
        right_layout.addWidget(self._make_collapsible("Quick Stats", stats_content, "ui/stats_collapsed"))

        right_layout.addWidget(QLabel("Points"))

        self.points_table = QTableWidget(0, 2)
        self.points_table.setHorizontalHeaderLabels(["#", "Label"])
        self.points_table.horizontalHeader().setStretchLastSection(True)
        self.points_table.verticalHeader().setVisible(False)
        self.points_table.setColumnWidth(0, 40)
        self.points_table.setSelectionBehavior(QAbstractItemView.SelectionBehavior.SelectRows)
        self.points_table.setEditTriggers(
            QAbstractItemView.EditTrigger.DoubleClicked | QAbstractItemView.EditTrigger.SelectedClicked
        )
        self._label_delegate = LabelDelegate({}, self)
        self._label_delegate.navigate_next.connect(self._on_navigate_next)
        self.points_table.setItemDelegateForColumn(1, self._label_delegate)
        self.points_table.itemChanged.connect(self._on_table_label_changed)
        self.points_table.currentCellChanged.connect(
            lambda cur_row, _cc, _pr, _pc: self._on_table_row_selected(cur_row)
        )
        right_layout.addWidget(self.points_table)

        h_splitter.addWidget(right)
        h_splitter.setSizes([240, 700, 260])

        # ---- Bottom: coral codes panel ----
        self._v_splitter.addWidget(self._build_codes_panel())
        self._v_splitter.setSizes([560, 160])

    def _build_codes_panel(self) -> QWidget:
        panel = QWidget()
        panel.setMinimumHeight(80)
        panel.setMaximumHeight(220)
        panel_layout = QVBoxLayout(panel)
        panel_layout.setContentsMargins(8, 4, 8, 4)
        panel_layout.setSpacing(4)

        header = QHBoxLayout()
        header.addWidget(QLabel("Coral Codes"))
        header.addStretch()
        header.addWidget(QLabel("Sort:"))
        self._sort_combo = QComboBox()
        self._sort_combo.addItems(["Frequency ↓", "A → Z"])
        self._sort_combo.currentIndexChanged.connect(self._refresh_codes_panel)
        header.addWidget(self._sort_combo)
        btn_add_code = QPushButton("+ Code")
        btn_add_code.clicked.connect(self._add_coral_code)
        header.addWidget(btn_add_code)
        btn_groups = QPushButton("⚙ Groups")
        btn_groups.clicked.connect(self._manage_groups)
        header.addWidget(btn_groups)
        panel_layout.addLayout(header)

        self._codes_scroll = _CodesScrollArea()
        self._codes_scroll.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        self._codes_scroll.setVerticalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAsNeeded)
        self._codes_scroll.setWidgetResizable(False)
        self._codes_scroll.setFrameShape(QScrollArea.Shape.NoFrame)
        panel_layout.addWidget(self._codes_scroll)

        return panel

    def _refresh_codes_panel(self):
        if not self.project:
            return
        codes = self.project.coral_codes
        groups = self.project.coral_groups

        freq: dict[str, int] = {}
        ann = self._current_annotation()
        if ann:
            for p in ann.points:
                if p.label:
                    freq[p.label] = freq.get(p.label, 0) + 1

        sort_az = self._sort_combo.currentText().startswith("A")

        grouped_codes: set[str] = set()
        for g in groups:
            grouped_codes.update(g.get("codes", []))
        ungrouped = [c for c in codes if c not in grouped_codes]

        display_groups = list(groups)
        if ungrouped:
            display_groups.append({"name": "Other", "codes": ungrouped})

        content = QWidget()
        flow = FlowLayout(content, h_spacing=8, v_spacing=4)
        flow.setContentsMargins(4, 2, 4, 2)

        for group in display_groups:
            group_codes = [c for c in group.get("codes", []) if c in codes]
            if not group_codes:
                continue

            if sort_az:
                group_codes = sorted(group_codes)
            else:
                group_codes = sorted(group_codes, key=lambda c: freq.get(c, 0), reverse=True)

            grp_widget = QWidget()
            grp_layout = QVBoxLayout(grp_widget)
            grp_layout.setContentsMargins(0, 0, 0, 0)
            grp_layout.setSpacing(2)

            grp_color = group.get("color", "")
            lbl = QLabel(group["name"])
            if grp_color and len(grp_color) == 6:
                r, g_val, b = int(grp_color[0:2], 16), int(grp_color[2:4], 16), int(grp_color[4:6], 16)
                luminance = 0.299 * r + 0.587 * g_val + 0.114 * b
                txt_color = "#000" if luminance > 140 else "#fff"
                lbl.setStyleSheet(
                    f"font-weight: bold; font-size: 9px; color: {txt_color};"
                    f" background: #{grp_color}; border-radius: 3px; padding: 1px 4px;"
                )
            else:
                lbl.setStyleSheet("font-weight: bold; font-size: 9px; color: #999;")
            lbl.setAlignment(Qt.AlignmentFlag.AlignCenter)
            grp_layout.addWidget(lbl)

            btn_row = QHBoxLayout()
            btn_row.setSpacing(2)
            btn_row.setContentsMargins(0, 0, 0, 0)
            for code in group_codes:
                count = freq.get(code, 0)
                btn = QPushButton(f"{code}\n{count}")
                btn.setMinimumSize(QSize(44, 34))
                btn.setSizePolicy(QSizePolicy.Policy.Preferred, QSizePolicy.Policy.Fixed)
                btn.setToolTip(f"{code} — {codes[code]}")
                btn.setStyleSheet(
                    "QPushButton { font-size: 9px; font-weight: bold; padding: 1px 4px; }"
                    "QPushButton:hover { background: #3a6ea5; color: white; }"
                )
                btn.clicked.connect(lambda checked, c=code: self._label_selected_point(c))
                btn_row.addWidget(btn)

            grp_layout.addLayout(btn_row)
            flow.addWidget(grp_widget)

        self._codes_scroll.setWidget(content)
        QTimer.singleShot(0, self._codes_scroll.updateLayout)

    def _build_statusbar(self):
        self.statusbar = QStatusBar()
        self.setStatusBar(self.statusbar)
        self._set_status("Ready")

    def _make_collapsible(self, title: str, content: QWidget, settings_key: str) -> QWidget:
        """Return a wrapper widget with a clickable header that collapses/expands content."""
        s = QSettings("coralX", "coralX")
        expanded = s.value(settings_key, True, type=bool)

        wrapper = QWidget()
        vbox = QVBoxLayout(wrapper)
        vbox.setContentsMargins(0, 0, 0, 0)
        vbox.setSpacing(0)

        header = QWidget()
        header.setCursor(Qt.CursorShape.PointingHandCursor)
        header.setStyleSheet(
            "QWidget { background: #444; border-radius: 3px; }"
            "QWidget:hover { background: #555; }"
        )
        h_row = QHBoxLayout(header)
        h_row.setContentsMargins(6, 3, 6, 3)
        h_row.setSpacing(4)

        toggle_btn = QPushButton("▼" if expanded else "▶")
        toggle_btn.setFixedSize(16, 16)
        toggle_btn.setFlat(True)
        toggle_btn.setStyleSheet("font-size: 8px; color: #ccc; border: none; background: transparent;")
        h_row.addWidget(toggle_btn)

        lbl = QLabel(title)
        lbl.setStyleSheet("font-weight: bold; font-size: 10px; color: #ddd; background: transparent;")
        h_row.addWidget(lbl)
        h_row.addStretch()

        vbox.addWidget(header)
        vbox.addWidget(content)

        content.setVisible(expanded)

        def _toggle(_=None):
            is_exp = not content.isVisible()
            content.setVisible(is_exp)
            toggle_btn.setText("▼" if is_exp else "▶")
            QSettings("coralX", "coralX").setValue(settings_key, is_exp)

        toggle_btn.clicked.connect(_toggle)
        header.mousePressEvent = _toggle  # type: ignore[method-assign]

        return wrapper

    def closeEvent(self, event) -> None:
        s = QSettings("coralX", "coralX")
        s.setValue("ui/h_splitter", self._h_splitter.saveState())
        s.setValue("ui/v_splitter", self._v_splitter.saveState())
        super().closeEvent(event)

    # ----------------------------------------------------------------- actions

    def _new_project(self):
        self.project = Project(name="Untitled Project")
        self._load_default_codes()
        self.project.stations.append(Station(name="Station 1"))
        self.points_table.setRowCount(0)
        self.setWindowTitle("coralX — Untitled Project")
        self._refresh_image_tree()
        self._refresh_codes_panel()
        self._set_status("New project created")

    def _open_project(self):
        path, _ = QFileDialog.getOpenFileName(self, "Open Project", "", "coralX (*.cpce)")
        if path:
            try:
                self.project = Project.load(path)
            except Exception as exc:
                self._log.exception("Failed to open project: %s", path)
                QMessageBox.critical(self, "Open Failed", f"Could not open project:\n{exc}")
                return
            self._refresh_image_tree()
            self._refresh_codes_panel()
            self._label_delegate.update_codes(self.project.coral_codes)
            self.setWindowTitle(f"coralX — {self.project.name}")
            self._log.info("Opened project: %s", path)
            self._set_status(f"Opened: {path}")

    def _save_project(self):
        if not self.project:
            return
        path = self.project.save_path
        if not path:
            path, _ = QFileDialog.getSaveFileName(
                self, "Save Project", self.project.name, "coralX (*.cpce)"
            )
        if path:
            try:
                self.project.save(path)
            except Exception as exc:
                self._log.exception("Failed to save project: %s", path)
                QMessageBox.critical(self, "Save Failed", f"Could not save project:\n{exc}")
                return
            self._log.info("Saved project: %s", path)
            self._set_status(f"Saved: {path}")

    def _add_images(self):
        if not self.project:
            return
        station = self._current_station()
        if station is None:
            if not self.project.stations:
                station = Station(name="Station 1")
                self.project.stations.append(station)
            else:
                station = self.project.stations[0]
        files, _ = QFileDialog.getOpenFileNames(
            self, "Add Images", "",
            "Images (*.jpg *.jpeg *.png *.tif *.tiff *.bmp)"
        )
        for f in files:
            if not any(a.image_path == f for a in self.project.annotations):
                station.annotations.append(ImageAnnotation(image_path=f))
        self._refresh_image_tree()
        self._set_status(f"Added {len(files)} image(s) to {station.name}")

    def _add_images_to_station(self, station: Station):
        if not self.project:
            return
        files, _ = QFileDialog.getOpenFileNames(
            self, "Add Images", "",
            "Images (*.jpg *.jpeg *.png *.tif *.tiff *.bmp)"
        )
        for f in files:
            if not any(a.image_path == f for a in self.project.annotations):
                station.annotations.append(ImageAnnotation(image_path=f))
        self._refresh_image_tree()
        self._set_status(f"Added {len(files)} image(s) to {station.name}")

    def _generate_points_current(self):
        ann = self._current_annotation()
        if ann is None or not self.project:
            return
        ann.points = generate_points(
            ann.image_width or 1000, ann.image_height or 1000,
            self.spin_points.value(),
            self.combo_dist.currentText(),
            self.spin_border.value(),
            border_rect=self.project.border_rect,
            border_polygon=self.project.border_polygon,
        )
        self._reload_canvas_ann(ann)
        self._set_status(f"Generated {len(ann.points)} points")

    def _generate_points_all(self):
        if not self.project:
            return
        for ann in self.project.annotations:
            ann.points = generate_points(
                ann.image_width or 1000, ann.image_height or 1000,
                self.spin_points.value(),
                self.combo_dist.currentText(),
                self.spin_border.value(),
                border_rect=self.project.border_rect,
                border_polygon=self.project.border_polygon,
            )
        current_ann = self._current_annotation()
        if current_ann:
            self._reload_canvas_ann(current_ann)
        self._refresh_image_tree()
        self._set_status(f"Generated points for all {len(self.project.annotations)} images")

    def _export_csv(self):
        path, _ = QFileDialog.getSaveFileName(self, "Export CSV", "", "CSV (*.csv)")
        if path and self.project:
            export_csv(self.project, path)
            self._set_status(f"Exported CSV: {path}")

    def _export_excel(self):
        path, _ = QFileDialog.getSaveFileName(self, "Export Excel", "", "Excel (*.xlsx)")
        if path and self.project:
            export_excel(self.project, path)
            self._set_status(f"Exported Excel: {path}")

    def _export_coral_codes(self):
        if not self.project or not self.project.coral_codes:
            QMessageBox.information(self, "Export Coral Codes", "No coral codes to export.")
            return
        path, _ = QFileDialog.getSaveFileName(
            self, "Export Coral Codes", f"{self.project.name}_codes",
            "JSON (*.json);;CSV (*.csv);;TSV (*.tsv)"
        )
        if not path:
            return
        try:
            export_coral_codes(self.project, path)
            n = len(self.project.coral_codes)
            self._set_status(f"Exported {n} coral codes to: {path}")
        except Exception as exc:
            QMessageBox.warning(self, "Export Failed", str(exc))

    def _show_stats(self):
        if not self.project:
            return
        stats = project_summary(self.project)
        if not stats:
            QMessageBox.information(self, "Stats", "No labeled points yet.")
            return

        dlg = QDialog(self)
        dlg.setWindowTitle("Project Statistics")
        dlg.setMinimumSize(520, 420)
        outer = QVBoxLayout(dlg)
        tabs = QTabWidget()

        # ---- Tab 1: Overview ----
        tab_overview = QWidget()
        ov_layout = QVBoxLayout(tab_overview)
        ov_text = QTextEdit()
        ov_text.setReadOnly(True)
        lines = [
            f"Project: {self.project.name}",
            f"Total points: {stats['total_points']}",
            f"Labeled points: {stats['labeled_points']}",
            "",
        ]
        # calibration summary
        calibrated = [a for a in self.project.annotations if a.scale_factor > 1.0]
        if calibrated:
            unit = calibrated[0].scale_unit
            total_area = sum(photo_area(a) or 0 for a in calibrated)
            lines.append(f"Calibrated images: {len(calibrated)} / {len(self.project.annotations)}")
            lines.append(f"Total photo area surveyed: {total_area:.2f} {unit}²")
            lines.append("")
        ov_text.setPlainText("\n".join(lines))
        ov_layout.addWidget(ov_text)
        tabs.addTab(tab_overview, "Overview")

        # ---- Tab 2: Diversity Indices ----
        tab_div = QWidget()
        div_layout = QVBoxLayout(tab_div)
        div_table = QTableWidget(0, 2)
        div_table.setHorizontalHeaderLabels(["Index", "Value"])
        div_table.horizontalHeader().setStretchLastSection(True)
        div_table.verticalHeader().setVisible(False)
        div_rows = [
            ("Species richness (S)",    str(stats.get("species_richness", "—"))),
            ("Shannon diversity (H')",  str(stats.get("shannon_diversity", "—"))),
            ("Simpson diversity (1-D)", str(stats.get("simpson_diversity", "—"))),
            ("Pielou evenness (J')",    str(stats.get("pielou_evenness", "—"))),
            ("Margalef richness (d)",   str(stats.get("margalef_richness", "—"))),
            ("Fisher alpha (α)",        str(stats.get("fisher_alpha", "—"))),
        ]
        for name, val in div_rows:
            r = div_table.rowCount()
            div_table.setRowCount(r + 1)
            div_table.setItem(r, 0, QTableWidgetItem(name))
            div_table.setItem(r, 1, QTableWidgetItem(val))
        div_layout.addWidget(div_table)
        tabs.addTab(tab_div, "Diversity")

        # ---- Tab 3: % Cover + CI ----
        tab_cov = QWidget()
        cov_layout = QVBoxLayout(tab_cov)
        cov_table = QTableWidget(0, 4)
        cov_table.setHorizontalHeaderLabels(["Code", "% Cover", "95% CI Low", "95% CI High"])
        cov_table.horizontalHeader().setStretchLastSection(True)
        cov_table.verticalHeader().setVisible(False)
        ci_data = stats.get("coverage_ci", {})
        for code, info in ci_data.items():
            r = cov_table.rowCount()
            cov_table.setRowCount(r + 1)
            cov_table.setItem(r, 0, QTableWidgetItem(code))
            cov_table.setItem(r, 1, QTableWidgetItem(f"{info['pct']}%"))
            cov_table.setItem(r, 2, QTableWidgetItem(f"{info['ci_lower']}%"))
            cov_table.setItem(r, 3, QTableWidgetItem(f"{info['ci_upper']}%"))
        cov_layout.addWidget(QLabel("Coverage with 95% Wilson Confidence Intervals:"))
        cov_layout.addWidget(cov_table)

        # Group coverage sub-section
        grp_cov = stats.get("group_coverage", {})
        if grp_cov:
            cov_layout.addWidget(QLabel("Group-level Coverage:"))
            grp_table = QTableWidget(0, 2)
            grp_table.setHorizontalHeaderLabels(["Group", "% Cover"])
            grp_table.horizontalHeader().setStretchLastSection(True)
            grp_table.verticalHeader().setVisible(False)
            for grp, pct in grp_cov.items():
                r = grp_table.rowCount()
                grp_table.setRowCount(r + 1)
                grp_table.setItem(r, 0, QTableWidgetItem(grp))
                grp_table.setItem(r, 1, QTableWidgetItem(f"{pct}%"))
            cov_layout.addWidget(grp_table)
        tabs.addTab(tab_cov, "% Cover + CI")

        # ---- Tab 4: Per Station ----
        tab_st = QWidget()
        st_layout = QVBoxLayout(tab_st)
        from src.core.statistics import per_station_table
        st_rows = per_station_table(self.project)
        if st_rows:
            st_table = QTableWidget(len(st_rows), len(st_rows[0]))
            headers = list(st_rows[0].keys())
            st_table.setHorizontalHeaderLabels(headers)
            st_table.horizontalHeader().setStretchLastSection(True)
            st_table.verticalHeader().setVisible(False)
            for r, row in enumerate(st_rows):
                for c, key in enumerate(headers):
                    val = row.get(key, "")
                    st_table.setItem(r, c, QTableWidgetItem(str(val) if val is not None else ""))
            st_layout.addWidget(st_table)
        tabs.addTab(tab_st, "Per Station")

        outer.addWidget(tabs)
        close_btn = QDialogButtonBox(QDialogButtonBox.StandardButton.Close)
        close_btn.rejected.connect(dlg.reject)
        outer.addWidget(close_btn)
        dlg.exec()

    def _calibrate_scale(self):
        ann = self._current_annotation()
        if ann is None:
            QMessageBox.information(self, "Calibrate Scale",
                "Please select an image first.")
            return
        dlg = CalibrationDialog(ann, self)
        dlg.calibration_applied.connect(self._on_calibration_applied)
        dlg.exec()

    def _on_calibration_applied(self, scale_factor: float, scale_unit: str, apply_all: bool):
        ann = self._current_annotation()
        if ann is None:
            return
        station = self._current_station()
        targets = station.annotations if (apply_all and station) else [ann]
        for a in targets:
            a.scale_factor = scale_factor
            a.scale_unit = scale_unit
        self._update_quick_stats()
        self._set_status(
            f"Scale set: {scale_factor:.3f} px/{scale_unit} "
            f"({'all images in station' if apply_all else 'this image'})"
        )

    # ─────────────────────────── import handlers ───────────────────────────

    def _import_coral_codes(self):
        if not self.project:
            return
        path, _ = QFileDialog.getOpenFileName(
            self, "Import Coral Codes", "",
            "Supported files (*.txt *.json *.csv *.tsv);;CPCe format (*.txt);;JSON (*.json);;CSV (*.csv *.tsv)"
        )
        if not path:
            return
        codes, groups, result = import_coral_codes(path)
        if not result.success:
            QMessageBox.warning(self, "Import Failed", result.message)
            return
        dlg = CoralCodesMergeDialog(
            len(codes), len(self.project.coral_codes),
            bool(groups), self
        )
        if dlg.exec() != dlg.DialogCode.Accepted:
            return
        if dlg.merge:
            self.project.coral_codes.update(codes)
        else:
            self.project.coral_codes = codes
        if dlg.import_groups and groups:
            self.project.coral_groups = groups
        self._label_delegate.update_codes(self.project.coral_codes)
        self._refresh_codes_panel()
        ImportResultDialog("Import Coral Codes", result.message, result.warnings, self).exec()

    def _import_station_metadata(self):
        if not self.project:
            return
        path, _ = QFileDialog.getOpenFileName(
            self, "Import Station Metadata", "", "CSV (*.csv)"
        )
        if not path:
            return
        incoming, result = import_station_metadata(path)
        if not result.success:
            QMessageBox.warning(self, "Import Failed", result.message)
            return
        existing_names = [s.name for s in self.project.stations]
        dlg = StationMergeDialog(incoming, existing_names, self)
        if dlg.exec() != dlg.DialogCode.Accepted:
            return
        station_map = {s.name: s for s in self.project.stations}
        added = 0
        updated = 0
        for meta in incoming:
            name = meta["name"]
            if name in station_map:
                if dlg.update_existing:
                    st = station_map[name]
                    st.depth_m  = meta["depth_m"]  if meta["depth_m"]  is not None else st.depth_m
                    st.date     = meta["date"]     or st.date
                    st.gps_lat  = meta["gps_lat"]  if meta["gps_lat"]  is not None else st.gps_lat
                    st.gps_lon  = meta["gps_lon"]  if meta["gps_lon"]  is not None else st.gps_lon
                    st.notes    = meta["notes"]    or st.notes
                    updated += 1
            else:
                self.project.stations.append(Station(
                    name=name,
                    depth_m=meta["depth_m"],
                    date=meta["date"],
                    gps_lat=meta["gps_lat"],
                    gps_lon=meta["gps_lon"],
                    notes=meta["notes"] or "",
                ))
                added += 1
        self._refresh_image_tree()
        msg = f"Added {added} station(s), updated {updated} station(s)."
        ImportResultDialog("Import Station Metadata", msg, result.warnings, self).exec()

    def _import_labeled_points(self):
        if not self.project:
            return
        if not self.project.annotations:
            QMessageBox.information(self, "Import Labeled Points",
                "Add images to the project first, then import labels.")
            return
        path, _ = QFileDialog.getOpenFileName(
            self, "Import Labeled Points", "",
            "Supported files (*.csv *.xlsx *.xls);;CSV (*.csv);;Excel (*.xlsx *.xls)"
        )
        if not path:
            return
        result = import_labeled_points(path, self.project)
        if not result.success:
            QMessageBox.warning(self, "Import Failed", result.message)
            return
        # Refresh UI
        ann = self._current_annotation()
        if ann:
            self._reload_canvas_ann(ann)
        self._update_progress(ann)
        self._update_quick_stats()
        self._refresh_image_tree()
        ImportResultDialog("Import Labeled Points", result.message, result.warnings, self).exec()

    def _import_cpce_cpc(self):
        """Import one or more CPCe native .cpc annotation files into the current project."""
        if not self.project:
            QMessageBox.warning(self, "No Project", "Open or create a project first.")
            return

        paths, _ = QFileDialog.getOpenFileNames(
            self, "Import CPCe .cpc File(s)", "",
            "CPCe annotation files (*.cpc)"
        )
        if not paths:
            return

        # Offer image directory resolution if images are likely in a different location
        image_dir = None
        if len(paths) > 0:
            reply = QMessageBox.question(
                self, "Image Directory",
                "Should coralX search a specific folder for the image files?\n\n"
                "(If the images are in the same folder as the .cpc files, click No.)",
                QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.No,
                QMessageBox.StandardButton.No,
            )
            if reply == QMessageBox.StandardButton.Yes:
                image_dir = QFileDialog.getExistingDirectory(
                    self, "Select Image Folder", os.path.dirname(paths[0])
                ) or None

        # Determine target station
        station_names = [s.name for s in self.project.stations]
        target_station: Station
        if not station_names:
            target_station = Station(name="Imported Station")
            self.project.stations.append(target_station)
        elif len(station_names) == 1:
            target_station = self.project.stations[0]
        else:
            from PyQt6.QtWidgets import QInputDialog
            chosen, ok = QInputDialog.getItem(
                self, "Select Station",
                "Add imported annotations to which station?",
                station_names + ["— Create new station —"],
                0, False,
            )
            if not ok:
                return
            if chosen == "— Create new station —":
                name, ok2 = QInputDialog.getText(self, "Station Name", "New station name:")
                if not ok2 or not name.strip():
                    return
                target_station = Station(name=name.strip())
                self.project.stations.append(target_station)
            else:
                target_station = next(s for s in self.project.stations if s.name == chosen)

        # Import each .cpc file
        all_warnings: list[str] = []
        success_count = 0
        total_points = 0

        for cpc_path in paths:
            ann, result = import_cpce_cpc(cpc_path, image_dir=image_dir)
            if not result.success:
                all_warnings.append(f"{os.path.basename(cpc_path)}: {result.message}")
                continue
            target_station.annotations.append(ann)
            success_count += 1
            total_points += len(ann.points)
            all_warnings.extend(
                f"{os.path.basename(cpc_path)}: {w}" for w in result.warnings
            )

        self._refresh_image_tree()
        self._update_quick_stats()

        if success_count == 0:
            msg = "No files were imported.\n\n" + "\n".join(all_warnings)
            QMessageBox.warning(self, "Import Failed", msg)
        else:
            summary = (
                f"Imported {success_count} of {len(paths)} .cpc file(s) — "
                f"{total_points} total points — into station '{target_station.name}'."
            )
            ImportResultDialog("Import CPCe .cpc", summary, all_warnings, self).exec()

    def _import_cpce_excel(self):
        path, _ = QFileDialog.getOpenFileName(
            self, "Import from CPCe Excel", "",
            "Excel files (*.xlsx *.xls)"
        )
        if not path:
            return
        imported_project, result = import_cpce_excel(path)
        if not result.success:
            QMessageBox.warning(self, "Import Failed",
                result.message + ("\n\nWarnings:\n" + "\n".join(result.warnings) if result.warnings else ""))
            return
        n_st  = len(imported_project.stations)
        n_img = sum(len(s.annotations) for s in imported_project.stations)
        n_pts = sum(len(a.points) for a in imported_project.annotations)
        dlg = CpceImportDialog(n_st, n_img, n_pts, self.project is not None, self)
        if dlg.exec() != dlg.DialogCode.Accepted:
            return
        if dlg.open_as_new:
            self.project = imported_project
            self._refresh_image_tree()
            self._refresh_codes_panel()
            self._label_delegate.update_codes(self.project.coral_codes)
            self.setWindowTitle(f"coralX — {self.project.name}")
        else:
            for st in imported_project.stations:
                self.project.stations.append(st)
            self._refresh_image_tree()
        self._update_quick_stats()
        ImportResultDialog("Import from CPCe Excel", result.message, result.warnings, self).exec()

    def _add_coral_code(self):
        dialog = QDialog(self)
        dialog.setWindowTitle("Add Coral Code")
        form = QFormLayout(dialog)
        code_edit = QLineEdit()
        desc_edit = QLineEdit()
        form.addRow("Code:", code_edit)
        form.addRow("Description:", desc_edit)
        btns = QDialogButtonBox(QDialogButtonBox.StandardButton.Ok | QDialogButtonBox.StandardButton.Cancel)
        btns.accepted.connect(dialog.accept)
        btns.rejected.connect(dialog.reject)
        form.addRow(btns)
        if dialog.exec() and code_edit.text():
            code = code_edit.text().strip().upper()
            self.project.coral_codes[code] = desc_edit.text()
            self._label_delegate.update_codes(self.project.coral_codes)
            self._refresh_codes_panel()

    def _manage_groups(self):
        if not self.project:
            return
        dlg = ManageGroupsDialog(self.project.coral_codes, self.project.coral_groups, self)
        if dlg.exec():
            self.project.coral_groups = dlg.get_groups()
            self._refresh_codes_panel()

    def _open_log_file(self) -> None:
        path = log_path()
        if not path.exists():
            QMessageBox.information(
                self, "Log File",
                f"No log file yet.\n\nIt will be created at:\n{path}",
            )
            return
        QDesktopServices.openUrl(QUrl.fromLocalFile(str(path)))

    def _show_about(self):
        QMessageBox.about(self, "coralX",
            "coralX — Coral Point Count\n\n"
            "A modern coral benthic analysis tool built with Python + PyQt6 + OpenCV.\n\n"
            "Features:\n"
            "• Multi-station project organization\n"
            "• Random, stratified, uniform point distribution\n"
            "• Click-to-label with custom coral codes\n"
            "• Shannon-Weaver & Simpson diversity indices\n"
            "• Export to CSV and Excel")

    # --------------------------------------------------------- station management

    def _add_station(self):
        if not self.project:
            return
        n = len(self.project.stations) + 1
        station = Station(name=f"Station {n}")
        self.project.stations.append(station)
        self._refresh_image_tree()
        # Select the new station item
        for i in range(self.image_tree.topLevelItemCount()):
            item = self.image_tree.topLevelItem(i)
            if item.data(0, Qt.ItemDataRole.UserRole) is station:
                self.image_tree.setCurrentItem(item)
                break
        self._set_status(f"Added {station.name}")

    def _edit_station_btn(self):
        station = self._current_station()
        if station:
            self._edit_station_metadata(station)
        else:
            self._set_status("Select a station first")

    def _edit_station_metadata(self, station: Station):
        dlg = StationMetadataDialog(station, self)
        if dlg.exec():
            data = dlg.get_station_data()
            station.name = data["name"]
            station.depth_m = data["depth_m"]
            station.date = data["date"]
            station.gps_lat = data["gps_lat"]
            station.gps_lon = data["gps_lon"]
            station.notes = data["notes"]
            self._refresh_image_tree()

    def _delete_station(self, station: Station):
        n_images = len(station.annotations)
        msg = f"Delete '{station.name}'?"
        if n_images > 0:
            labeled = station.labeled_points()
            msg += f"\n\nThis station has {n_images} image(s) with {labeled} labeled point(s). All data will be lost."
        reply = QMessageBox.question(
            self, "Delete Station", msg,
            QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.Cancel
        )
        if reply == QMessageBox.StandardButton.Yes:
            assert self.project is not None
            self.project.stations.remove(station)
            self._refresh_image_tree()
            ann = self._current_annotation()
            if ann:
                self._reload_canvas_ann(ann)
            else:
                self.points_table.setRowCount(0)
                self._update_progress(None)

    def _move_image_to_station(self, ann: ImageAnnotation, source_station: Station, target_station: Station):
        source_station.annotations.remove(ann)
        target_station.annotations.append(ann)
        self._refresh_image_tree()
        # Re-select the moved image in its new position
        self._select_annotation(ann)

    def _remove_image(self, ann: ImageAnnotation, station: Station):
        reply = QMessageBox.question(
            self, "Remove Image",
            f"Remove '{os.path.basename(ann.image_path)}' from this project?",
            QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.Cancel
        )
        if reply == QMessageBox.StandardButton.Yes:
            station.annotations.remove(ann)
            self._refresh_image_tree()
            new_ann = self._current_annotation()
            if new_ann:
                self._reload_canvas_ann(new_ann)
            else:
                self.points_table.setRowCount(0)
                self._update_progress(None)

    # ----------------------------------------------------------------- helpers

    def _current_annotation(self) -> ImageAnnotation | None:
        item = self.image_tree.currentItem()
        if item is None:
            return None
        data = item.data(0, Qt.ItemDataRole.UserRole)
        return data[0] if isinstance(data, tuple) else None

    def _current_station(self) -> Station | None:
        item = self.image_tree.currentItem()
        if item is None:
            return None
        data = item.data(0, Qt.ItemDataRole.UserRole)
        if isinstance(data, tuple):
            return data[1]
        if isinstance(data, Station):
            return data
        return None

    def _select_annotation(self, target_ann: ImageAnnotation):
        """Walk the tree to find and select the item for target_ann."""
        for i in range(self.image_tree.topLevelItemCount()):
            st_item = self.image_tree.topLevelItem(i)
            for j in range(st_item.childCount()):
                img_item = st_item.child(j)
                data = img_item.data(0, Qt.ItemDataRole.UserRole)
                if isinstance(data, tuple) and data[0] is target_ann:
                    self.image_tree.setCurrentItem(img_item)
                    return

    def _on_tree_item_changed(self, current: QTreeWidgetItem | None, previous: QTreeWidgetItem | None):
        if current is None or not self.project:
            return
        data = current.data(0, Qt.ItemDataRole.UserRole)
        if isinstance(data, tuple):
            ann, station = data
            self._reload_canvas_ann(ann)

    def _on_tree_context_menu(self, pos):
        item = self.image_tree.itemAt(pos)
        if item is None or not self.project:
            return
        data = item.data(0, Qt.ItemDataRole.UserRole)
        menu = QMenu(self)

        if isinstance(data, Station):
            station = data
            menu.addAction("Edit Metadata…", lambda: self._edit_station_metadata(station))
            menu.addAction("Add Images Here", lambda: self._add_images_to_station(station))
            menu.addSeparator()
            delete_action = menu.addAction("Delete Station", lambda: self._delete_station(station))
            if len(self.project.stations) <= 1:
                delete_action.setEnabled(False)
                delete_action.setToolTip("Cannot delete the only station")

        elif isinstance(data, tuple):
            ann, station = data
            if len(self.project.stations) > 1:
                move_menu = menu.addMenu("Move to Station")
                for s in self.project.stations:
                    if s is not station:
                        move_menu.addAction(s.name,
                            lambda checked=False, t=s: self._move_image_to_station(ann, station, t))
            menu.addSeparator()
            menu.addAction("Remove Image", lambda: self._remove_image(ann, station))

        if not menu.isEmpty():
            menu.exec(self.image_tree.mapToGlobal(pos))

    def _reload_canvas_ann(self, ann: ImageAnnotation):
        if not self.project:
            return
        import cv2
        img = cv2.imread(ann.image_path)
        if img is not None:
            ann.image_height, ann.image_width = img.shape[:2]
        else:
            self._log.warning("Could not read image file: %s", ann.image_path)

        if self.project.border_polygon:
            self.canvas.set_border_polygon(self.project.border_polygon)
            self.canvas.set_border_rect(None)
        elif self.project.border_rect:
            self.canvas.set_border_rect(tuple(self.project.border_rect))
            self.canvas.set_border_polygon(None)
        else:
            self.canvas.set_border_rect(None)
            self.canvas.set_border_polygon(None)
            self.canvas.set_border(self.spin_border.value())
        self.canvas.load_image(ann, self.project.coral_codes)
        self._update_progress(ann)
        self._populate_points_table(ann)
        self._refresh_codes_panel()

    def _label_selected_point(self, code: str):
        row = self.points_table.currentRow()
        if row < 0 or not self.project:
            return
        ann = self._current_annotation()
        if ann is None or row >= len(ann.points):
            return
        ann.points[row].label = code
        self._syncing = True
        item = self.points_table.item(row, 1)
        if item:
            item.setText(code)
        self._syncing = False
        self.canvas.set_selected_index(row)
        self.canvas.update()
        self._update_progress(ann)
        self._update_quick_stats()
        self._refresh_codes_panel()
        self._refresh_image_tree()
        next_row = row + 1
        if next_row < len(ann.points):
            self._advance_point(next_row)

    def _advance_point(self, row: int):
        ann = self._current_annotation()
        if ann and row < len(ann.points):
            self.canvas.select_point(row)
            self.points_table.setCurrentCell(row, 0)

    def _on_point_labeled(self, index: int, label: str):
        ann = self._current_annotation()
        if ann is None or not self.project:
            return
        self._syncing = True
        item = self.points_table.item(index, 1)
        if item:
            item.setText(label)
        self.points_table.scrollToItem(self.points_table.item(index, 1))
        self._syncing = False
        self._update_progress(ann)
        self._update_quick_stats()
        self._refresh_codes_panel()
        self._refresh_image_tree()

    def _update_progress(self, ann: ImageAnnotation | None):
        if not self.project or ann is None:
            self.progress_bar.setValue(0)
            self.progress_bar.setFormat("No image")
            return
        scope = self._progress_scope_combo.currentText()
        if scope == "Image":
            total = len(ann.points)
            labeled = ann.labeled_count()
        elif scope == "Station":
            st = self._current_station()
            if st:
                total = st.total_points()
                labeled = st.labeled_points()
            else:
                total = len(ann.points)
                labeled = ann.labeled_count()
        else:  # Project
            total = sum(len(a.points) for a in self.project.annotations)
            labeled = sum(a.labeled_count() for a in self.project.annotations)
        pct = int(labeled / total * 100) if total > 0 else 0
        self.progress_bar.setValue(pct)
        self.progress_bar.setFormat(f"{labeled}/{total}")

    def _on_scope_changed(self):
        ann = self._current_annotation()
        self._update_progress(ann)

    def _update_quick_stats(self):
        if not self.project:
            return
        stats = project_summary(self.project)
        if not stats:
            self.stats_label.setText("—")
            return
        lines = [
            f"S: {stats.get('species_richness', '—')}",
            f"H': {stats['shannon_diversity']}",
            f"J': {stats.get('pielou_evenness', '—')}",
            f"1-D: {stats['simpson_diversity']}",
            "",
        ]
        for label, pct in list(stats["coverage"].items())[:5]:
            lines.append(f"{label}: {pct}%")
        # show calibration status for current image
        ann = self._current_annotation()
        if ann and ann.scale_factor > 1.0:
            p_area = photo_area(ann)
            if p_area:
                lines.append(f"\n📏 {p_area:.1f} {ann.scale_unit}²/photo")
        self.stats_label.setText("\n".join(lines))

    def _refresh_image_tree(self):
        saved_ann = self._current_annotation()
        self.image_tree.blockSignals(True)
        self.image_tree.clear()

        item_to_select = None
        if self.project:
            for station in self.project.stations:
                labeled = station.labeled_points()
                total = station.total_points()
                st_item = QTreeWidgetItem(self.image_tree)
                st_item.setText(0, f"{station.name}  [{labeled}/{total}]")
                font = st_item.font(0)
                font.setBold(True)
                st_item.setFont(0, font)
                st_item.setData(0, Qt.ItemDataRole.UserRole, station)

                for ann in station.annotations:
                    name = os.path.basename(ann.image_path)
                    done = "✓" if ann.is_complete() and ann.points else "○"
                    img_item = QTreeWidgetItem(st_item)
                    img_item.setText(0, f"{done} {name}")
                    img_item.setData(0, Qt.ItemDataRole.UserRole, (ann, station))
                    if saved_ann is not None and ann is saved_ann:
                        item_to_select = img_item

                st_item.setExpanded(True)

        self.image_tree.blockSignals(False)

        if item_to_select:
            self.image_tree.setCurrentItem(item_to_select)

        self._apply_image_filter()
        self._load_thumbnails()

    def _load_thumbnails(self):
        """Start async thumbnail loading for all image items in the tree."""
        if self._thumbnail_loader and self._thumbnail_loader.isRunning():
            self._thumbnail_loader.terminate()
            self._thumbnail_loader.wait()

        paths = []
        for i in range(self.image_tree.topLevelItemCount()):
            st_item = self.image_tree.topLevelItem(i)
            for j in range(st_item.childCount()):
                img_item = st_item.child(j)
                data = img_item.data(0, Qt.ItemDataRole.UserRole)
                if isinstance(data, tuple):
                    paths.append(data[0].image_path)

        if not paths:
            return

        self._thumbnail_loader = ThumbnailLoader(paths, self)
        self._thumbnail_loader.thumbnail_ready.connect(self._on_thumbnail_ready)
        self._thumbnail_loader.start()

    def _on_thumbnail_ready(self, path: str, qimg: QImage):
        """Set thumbnail icon on the matching tree item (runs on main thread via signal)."""
        pixmap = QPixmap.fromImage(qimg)
        icon = QIcon(pixmap)
        for i in range(self.image_tree.topLevelItemCount()):
            st_item = self.image_tree.topLevelItem(i)
            for j in range(st_item.childCount()):
                img_item = st_item.child(j)
                data = img_item.data(0, Qt.ItemDataRole.UserRole)
                if isinstance(data, tuple) and data[0].image_path == path:
                    img_item.setIcon(0, icon)
                    return

    def _apply_image_filter(self):
        incomplete_only = self._filter_checkbox.isChecked()
        for i in range(self.image_tree.topLevelItemCount()):
            st_item = self.image_tree.topLevelItem(i)
            all_complete = True
            for j in range(st_item.childCount()):
                img_item = st_item.child(j)
                data = img_item.data(0, Qt.ItemDataRole.UserRole)
                ann = data[0] if isinstance(data, tuple) else None
                complete = ann is not None and ann.is_complete() and bool(ann.points)
                img_item.setHidden(incomplete_only and complete)
                if not complete:
                    all_complete = False
            st_item.setHidden(incomplete_only and all_complete)

    def _load_default_codes(self):
        import json
        path = os.path.join(os.path.dirname(__file__), "../../data/coral_codes_default.json")
        if os.path.exists(path):
            with open(path) as f:
                data = json.load(f)
            if isinstance(data, dict) and "codes" in data:
                self.project.coral_codes = data["codes"]
                self.project.coral_groups = data.get("groups", [])
            else:
                self.project.coral_codes = data

    def _on_border_defined(self, x_min: int, y_min: int, x_max: int, y_max: int):
        if self.project:
            self.project.border_rect = [x_min, y_min, x_max, y_max]
            self.project.border_polygon = None
        self._set_status(f"Border set: ({x_min}, {y_min}) → ({x_max}, {y_max})")

    def _on_border_polygon_defined(self, poly: list) -> None:
        if self.project:
            self.project.border_polygon = poly
            self.project.border_rect = None
        self._set_status(f"Polygon border set: {len(poly)} points")

    def _clear_border_rect(self):
        if self.project:
            self.project.border_rect = None
            self.project.border_polygon = None
        self.canvas.set_border_rect(None)
        self.canvas.set_border_polygon(None)
        self.canvas.set_border(self.spin_border.value())
        self._set_status("Custom border cleared")

    def _on_border_spinbox_changed(self, value: int):
        if self.project:
            self.project.border_rect = None
        self.canvas.set_border_rect(None)
        self.canvas.set_border(value)

    def _on_navigate_next(self):
        row = self.points_table.currentRow()
        ann = self._current_annotation()
        if ann is None:
            return
        next_row = row + 1
        if next_row < len(ann.points):
            QTimer.singleShot(0, lambda: self._start_edit_row(next_row))

    def _start_edit_row(self, row: int):
        ann = self._current_annotation()
        if ann and row < len(ann.points):
            self.canvas.select_point(row)
            self.points_table.setCurrentCell(row, 1)
            self.points_table.edit(self.points_table.model().index(row, 1))

    def _populate_points_table(self, ann: ImageAnnotation | None):
        self._syncing = True
        self.points_table.setRowCount(0)
        if ann and ann.points:
            self._label_delegate.update_codes(self.project.coral_codes if self.project else {})
            self.points_table.setRowCount(len(ann.points))
            for p in ann.points:
                idx_item = QTableWidgetItem(str(p.index + 1))
                idx_item.setFlags(idx_item.flags() & ~Qt.ItemFlag.ItemIsEditable)
                self.points_table.setItem(p.index, 0, idx_item)
                self.points_table.setItem(p.index, 1, QTableWidgetItem(p.label or ""))
        self._syncing = False

    def _on_table_label_changed(self, item: QTableWidgetItem):
        if self._syncing or not self.project or item.column() != 1:
            return
        ann = self._current_annotation()
        if ann is None:
            return
        row = item.row()
        if row >= len(ann.points):
            return
        label_text = item.text().strip()
        ann.points[row].label = label_text if label_text else None
        self.canvas.update()
        self._update_progress(ann)
        self._update_quick_stats()
        self._refresh_codes_panel()
        self._refresh_image_tree()

    def _on_table_row_selected(self, row: int):
        if self._syncing or row < 0:
            return
        self._syncing = True
        self.canvas.select_point(row)
        self._syncing = False

    def _on_canvas_point_selected(self, index: int):
        if self._syncing:
            return
        self._syncing = True
        self.points_table.setCurrentCell(index, 1)
        self._syncing = False

    def _set_status(self, msg: str):
        if (bar := self.statusBar()) is not None:
            bar.showMessage(msg)

    # ---------------------------------------------------------------- AI label

    def _ai_auto_label(self) -> None:
        if not self.project:
            QMessageBox.information(self, "AI Auto-Label", "Open or create a project first.")
            return

        if self._ai_worker is not None and self._ai_worker.isRunning():
            QMessageBox.information(self, "AI Auto-Label", "Inference is already running.")
            return

        dlg = AILabelDialog(self.project, self)
        if dlg.exec() != QDialog.DialogCode.Accepted:
            return

        scope = dlg.scope()
        if scope == "image":
            ann = self._current_annotation()
            if ann is None:
                QMessageBox.warning(self, "AI Auto-Label", "No image selected.")
                return
            annotations = [ann]
        elif scope == "station":
            station = self._current_station()
            if station is None:
                QMessageBox.warning(self, "AI Auto-Label", "No station selected.")
                return
            annotations = list(station.annotations)
        else:
            annotations = list(self.project.annotations)

        annotations = [a for a in annotations if a.points]
        if not annotations:
            QMessageBox.information(self, "AI Auto-Label", "No points found in the selected scope.")
            return

        overwrite = dlg.overwrite_labeled()
        total_points = sum(
            len([p for p in a.points if overwrite or p.label is None])
            for a in annotations
        )
        if total_points == 0:
            QMessageBox.information(
                self, "AI Auto-Label",
                "All points are already labeled.\n"
                "Uncheck 'Label only unlabeled points' to re-label all.",
            )
            return

        labeler = dlg.labeler
        if labeler is None:
            QMessageBox.warning(
                self, "AI Auto-Label",
                "No model loaded. Please click 'Load Model & Detect Classes' before running.",
            )
            return

        progress_dlg = AIProgressDialog(total_points, self)
        worker = AILabelWorker(
            labeler=labeler,
            annotations=annotations,
            class_mapping=dlg.class_mapping(),
            conf_threshold=dlg.conf_threshold(),
            crop_size=dlg.crop_size(),
            overwrite_labeled=overwrite,
            parent=self,
        )
        self._ai_worker = worker

        worker.progress.connect(progress_dlg.on_progress)
        worker.error.connect(progress_dlg.on_error)
        worker.result_ready.connect(self._on_ai_results_ready)
        worker.finished.connect(progress_dlg.on_finished)
        worker.finished.connect(self._clear_ai_worker)
        worker.finished.connect(worker.deleteLater)
        progress_dlg.cancel_requested.connect(worker.cancel)

        worker.start()
        progress_dlg.exec()
        # _ai_worker is cleared by _clear_ai_worker when finished fires.
        # If the dialog is closed before the worker finishes, the guard stays
        # set until the thread completes, preventing concurrent runs.

    def _on_ai_results_ready(self, results: list) -> None:
        if self.project is None:
            return
        label_map: dict[str, dict[int, str]] = {}
        for r in results:
            if r.mapped_code is not None:
                label_map.setdefault(r.annotation_path, {})[r.point_index] = r.mapped_code

        labeled_count = 0
        for ann in self.project.annotations:
            point_updates = label_map.get(ann.image_path, {})
            for p in ann.points:
                if p.index in point_updates:
                    p.label = point_updates[p.index]
                    labeled_count += 1

        current_ann = self._current_annotation()
        if current_ann:
            self._reload_canvas_ann(current_ann)
        self._update_progress(current_ann)
        self._update_quick_stats()
        self._refresh_codes_panel()
        self._refresh_image_tree()
        self._set_status(f"AI auto-label complete: {labeled_count} point(s) labeled.")

    def _clear_ai_worker(self) -> None:
        self._ai_worker = None
