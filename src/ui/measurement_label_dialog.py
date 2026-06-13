from PyQt6.QtWidgets import (
    QDialog, QVBoxLayout, QHBoxLayout, QLabel, QLineEdit,
    QPushButton, QDialogButtonBox,
)
from PyQt6.QtCore import Qt

from src.models.project import Measurement


class MeasurementLabelDialog(QDialog):
    """
    Shown after a measurement is drawn.
    User enters a fragment name; on accept the label is set on the Measurement.
    """

    def __init__(self, measurement: Measurement, parent=None):
        super().__init__(parent)
        self.setWindowTitle("Name This Measurement")
        self.setModal(True)
        self.setMinimumWidth(320)
        self._measurement = measurement

        layout = QVBoxLayout(self)
        layout.setSpacing(10)

        # Result display
        type_names = {"line": "Length", "polyline": "Length (polyline)", "polygon": "Area"}
        type_label = type_names.get(measurement.type, measurement.type.capitalize())
        unit_str = f"{measurement.unit}²" if measurement.type == "polygon" else measurement.unit
        result_text = f"<b>{type_label}:</b> {measurement.value:.3f} {unit_str}"
        result_lbl = QLabel(result_text)
        result_lbl.setAlignment(Qt.AlignmentFlag.AlignCenter)
        result_lbl.setStyleSheet("font-size: 14px; padding: 6px;")
        layout.addWidget(result_lbl)

        # Label input
        name_row = QHBoxLayout()
        name_row.addWidget(QLabel("Fragment name:"))
        self._name_edit = QLineEdit()
        self._name_edit.setPlaceholderText("e.g. Frag-01")
        name_row.addWidget(self._name_edit)
        layout.addLayout(name_row)

        # Buttons
        buttons = QDialogButtonBox(
            QDialogButtonBox.StandardButton.Save | QDialogButtonBox.StandardButton.Cancel
        )
        buttons.accepted.connect(self._on_accept)
        buttons.rejected.connect(self.reject)
        layout.addWidget(buttons)

        self._name_edit.setFocus()

    def _on_accept(self):
        label = self._name_edit.text().strip()
        if not label:
            label = f"M-{self._measurement.type[:3].upper()}"
        self._measurement.label = label
        self.accept()

    def measurement(self) -> Measurement:
        return self._measurement
