from __future__ import annotations

from typing import TYPE_CHECKING

from PyQt6.QtWidgets import (
    QDialog, QVBoxLayout, QHBoxLayout, QGroupBox, QFormLayout,
    QLineEdit, QPushButton, QDoubleSpinBox, QSpinBox, QRadioButton,
    QCheckBox, QTableWidget, QTableWidgetItem, QComboBox, QLabel,
    QProgressBar, QTextEdit, QDialogButtonBox, QMessageBox, QFileDialog,
)
from PyQt6.QtCore import Qt, QSettings, pyqtSignal
from src.ui.progress_dialog import ProgressDialog, WorkerThread

if TYPE_CHECKING:
    from src.models.project import Project
    from src.core.ai_labeler import AILabeler

_SETTINGS_KEY_MODEL = "ai/model_path"
_SETTINGS_KEY_CONF = "ai/conf_threshold"
_SETTINGS_KEY_CROP = "ai/crop_size"
_SETTINGS_KEY_SCOPE = "ai/scope"
_SETTINGS_KEY_OVERWRITE = "ai/overwrite_labeled"


class ClassMappingTable(QTableWidget):
    def __init__(self, coral_codes: dict[str, str], parent=None) -> None:
        super().__init__(0, 2, parent)
        self._coral_codes = coral_codes
        self.setHorizontalHeaderLabels(["Model Class", "Coral Code"])
        header = self.horizontalHeader()
        if header is not None:
            header.setStretchLastSection(True)
        self.setSelectionMode(QTableWidget.SelectionMode.NoSelection)
        self.setEditTriggers(QTableWidget.EditTrigger.NoEditTriggers)

    def populate(self, suggestions: dict[str, str | None]) -> None:
        self.setRowCount(0)
        options = ["(skip)"] + sorted(self._coral_codes.keys())
        for row, (cls_name, suggested) in enumerate(suggestions.items()):
            self.insertRow(row)
            self.setItem(row, 0, QTableWidgetItem(cls_name))
            combo = QComboBox()
            combo.addItems(options)
            if suggested and suggested in options:
                combo.setCurrentText(suggested)
            self.setCellWidget(row, 1, combo)

    def get_mapping(self) -> dict[str, str | None]:
        result: dict[str, str | None] = {}
        for row in range(self.rowCount()):
            item = self.item(row, 0)
            cls_name = item.text() if item is not None else ""
            widget = self.cellWidget(row, 1)
            value = widget.currentText() if isinstance(widget, QComboBox) else "(skip)"
            result[cls_name] = None if value == "(skip)" else value
        return result


class AILabelDialog(QDialog):
    def __init__(self, project: Project, parent=None) -> None:
        super().__init__(parent)
        self.setWindowTitle("AI Auto-Label")
        self.setMinimumWidth(500)
        self._project = project
        self._labeler: AILabeler | None = None
        settings = QSettings("coralX", "coralX")

        layout = QVBoxLayout(self)

        # --- Model group ---
        model_box = QGroupBox("Model")
        model_form = QFormLayout(model_box)

        path_row = QHBoxLayout()
        self._model_path_edit = QLineEdit()
        self._model_path_edit.setPlaceholderText("Path to YOLOv8 .pt classification model…")
        self._model_path_edit.setText(settings.value(_SETTINGS_KEY_MODEL, ""))
        self._model_path_edit.returnPressed.connect(self._load_model)
        path_row.addWidget(self._model_path_edit)
        browse_btn = QPushButton("Browse…")
        browse_btn.clicked.connect(self._browse_model)
        path_row.addWidget(browse_btn)
        model_form.addRow("Model file (.pt):", path_row)

        self._conf_spin = QDoubleSpinBox()
        self._conf_spin.setRange(0.0, 1.0)
        self._conf_spin.setSingleStep(0.05)
        self._conf_spin.setDecimals(2)
        self._conf_spin.setValue(float(settings.value(_SETTINGS_KEY_CONF, 0.5)))
        model_form.addRow("Confidence threshold:", self._conf_spin)

        self._crop_spin = QSpinBox()
        self._crop_spin.setRange(32, 512)
        self._crop_spin.setSingleStep(32)
        self._crop_spin.setValue(int(settings.value(_SETTINGS_KEY_CROP, 64)))
        self._crop_spin.setSuffix(" px")
        model_form.addRow("Crop size:", self._crop_spin)

        layout.addWidget(model_box)

        # --- Scope group ---
        scope_box = QGroupBox("Scope")
        scope_layout = QVBoxLayout(scope_box)
        self._scope_image = QRadioButton("This image only")
        self._scope_station = QRadioButton("This station")
        self._scope_project = QRadioButton("Entire project")
        saved_scope = settings.value(_SETTINGS_KEY_SCOPE, "image")
        if saved_scope == "station":
            self._scope_station.setChecked(True)
        elif saved_scope == "project":
            self._scope_project.setChecked(True)
        else:
            self._scope_image.setChecked(True)
        scope_layout.addWidget(self._scope_image)
        scope_layout.addWidget(self._scope_station)
        scope_layout.addWidget(self._scope_project)
        layout.addWidget(scope_box)

        # --- Overwrite checkbox ---
        self._overwrite_cb = QCheckBox("Label only unlabeled points (uncheck to overwrite all)")
        saved_overwrite = settings.value(_SETTINGS_KEY_OVERWRITE, "true")
        self._overwrite_cb.setChecked(saved_overwrite != "false")
        layout.addWidget(self._overwrite_cb)

        # --- Class mapping group ---
        mapping_box = QGroupBox("Class Mapping")
        mapping_layout = QVBoxLayout(mapping_box)
        self._hint_label = QLabel("Select a .pt model file above to populate this table.")
        self._hint_label.setStyleSheet("color: gray; font-style: italic;")
        mapping_layout.addWidget(self._hint_label)
        self._mapping_table = ClassMappingTable(project.coral_codes)
        self._mapping_table.setMinimumHeight(120)
        self._mapping_table.hide()
        mapping_layout.addWidget(self._mapping_table)
        reload_btn = QPushButton("Load Model")
        reload_btn.clicked.connect(self._load_model)
        mapping_layout.addWidget(reload_btn)
        layout.addWidget(mapping_box)

        # --- Buttons ---
        self._btn_box = QDialogButtonBox()
        run_btn = self._btn_box.addButton("Run", QDialogButtonBox.ButtonRole.AcceptRole)
        assert run_btn is not None
        self._run_btn: QPushButton = run_btn
        self._btn_box.addButton(QDialogButtonBox.StandardButton.Cancel)
        self._run_btn.setEnabled(False)
        self._btn_box.accepted.connect(self.accept)
        self._btn_box.rejected.connect(self.reject)
        layout.addWidget(self._btn_box)

    def _browse_model(self) -> None:
        path, _ = QFileDialog.getOpenFileName(
            self, "Select YOLOv8 Model", "", "YOLO model (*.pt)"
        )
        if path:
            self._model_path_edit.setText(path)
            self._load_model()

    def _load_model(self) -> None:
        from src.core.ai_labeler import AILabeler  # pylint: disable=import-outside-toplevel

        path = self._model_path_edit.text().strip()
        if not path:
            QMessageBox.warning(self, "No model selected", "Please select a .pt model file first.")
            return

        _project = self._project

        def _run(cb):
            cb(0, 0, "Loading AI model…")
            labeler = AILabeler(path)
            cb(0, 0, "Mapping classes to coral codes…")
            suggestions = AILabeler.suggest_mapping(
                labeler.class_names(), _project.coral_codes
            )
            return labeler, suggestions

        def _on_done(result):
            dlg.accept()
            labeler, suggestions = result
            self._mapping_table.populate(suggestions)
            self._hint_label.hide()
            self._mapping_table.show()
            self._labeler = labeler
            self._run_btn.setEnabled(True)
            task_label = "detection" if labeler.task == "detect" else "classification"
            QMessageBox.information(
                self, "Model Loaded",
                f"Model loaded successfully.\n"
                f"Type: {task_label}\n"
                f"Classes ({len(labeler.class_names())}): {', '.join(labeler.class_names())}",
            )

        def _on_error(msg):
            dlg.accept()
            QMessageBox.critical(self, "Failed to Load Model", msg)

        dlg = ProgressDialog("Loading AI Model…", cancellable=False, parent=self)
        dlg.set_indeterminate("Loading model, please wait…")
        worker = WorkerThread(_run, parent=self)
        worker.progress.connect(dlg.update)
        worker.succeeded.connect(_on_done)
        worker.failed.connect(_on_error)
        worker.start()
        dlg.exec()

    def accept(self) -> None:
        settings = QSettings("coralX", "coralX")
        settings.setValue(_SETTINGS_KEY_MODEL, self._model_path_edit.text())
        settings.setValue(_SETTINGS_KEY_CONF, self._conf_spin.value())
        settings.setValue(_SETTINGS_KEY_CROP, self._crop_spin.value())
        settings.setValue(_SETTINGS_KEY_SCOPE, self.scope())
        overwrite_val = "true" if self._overwrite_cb.isChecked() else "false"
        settings.setValue(_SETTINGS_KEY_OVERWRITE, overwrite_val)
        super().accept()

    # --- Public properties ---

    def model_path(self) -> str:
        return self._model_path_edit.text().strip()

    def conf_threshold(self) -> float:
        return self._conf_spin.value()

    def crop_size(self) -> int:
        return self._crop_spin.value()

    def scope(self) -> str:
        if self._scope_station.isChecked():
            return "station"
        if self._scope_project.isChecked():
            return "project"
        return "image"

    def overwrite_labeled(self) -> bool:
        return not self._overwrite_cb.isChecked()

    def class_mapping(self) -> dict[str, str | None]:
        return self._mapping_table.get_mapping()

    @property
    def labeler(self) -> AILabeler | None:
        return self._labeler


class AIProgressDialog(QDialog):
    cancel_requested = pyqtSignal()

    def __init__(self, total_points: int, parent=None) -> None:
        super().__init__(parent)
        self.setWindowTitle("AI Auto-Label — Running")
        self.setMinimumWidth(480)
        self.setMinimumHeight(300)
        self.setWindowModality(Qt.WindowModality.ApplicationModal)

        layout = QVBoxLayout(self)

        layout.addWidget(QLabel("Running AI inference…"))

        self._status_label = QLabel("Starting…")
        layout.addWidget(self._status_label)

        self._progress_bar = QProgressBar()
        self._progress_bar.setRange(0, total_points)
        self._progress_bar.setValue(0)
        layout.addWidget(self._progress_bar)

        self._log = QTextEdit()
        self._log.setReadOnly(True)
        self._log.setMinimumHeight(120)
        font = self._log.font()
        font.setFamily("Monospace")
        font.setPointSize(9)
        self._log.setFont(font)
        layout.addWidget(self._log)

        self._cancel_btn = QPushButton("Cancel")
        self._cancel_btn.clicked.connect(self._on_cancel)
        layout.addWidget(self._cancel_btn, alignment=Qt.AlignmentFlag.AlignRight)

    def _on_cancel(self) -> None:
        if self._cancel_btn.text() == "Cancel":
            self.cancel_requested.emit()
            self._cancel_btn.setEnabled(False)
            self._status_label.setText("Cancelling…")
        else:
            self.accept()

    def on_progress(self, done: int, _total: int, status: str) -> None:
        self._progress_bar.setValue(done)
        self._status_label.setText(status)
        self._log.append(f"  {status}")
        # Auto-scroll to bottom
        scrollbar = self._log.verticalScrollBar()
        if scrollbar is not None:
            scrollbar.setValue(scrollbar.maximum())

    def on_error(self, msg: str) -> None:
        self._log.append(f"<span style='color:red'>ERROR: {msg}</span>")
        self._cancel_btn.setEnabled(True)

    def on_finished(self) -> None:
        self._cancel_btn.setText("Close")
        self._cancel_btn.setEnabled(True)
        self._status_label.setText("Done.")
