from PyQt6.QtWidgets import (
    QDialog, QVBoxLayout, QHBoxLayout, QLabel, QLineEdit,
    QPushButton, QDialogButtonBox, QComboBox, QFrame,
)
from PyQt6.QtCore import Qt

from src.models.project import Measurement


class MeasurementLabelDialog(QDialog):
    """
    Shown after a measurement is drawn.
    User enters a fragment name and optionally a species/genus.
    The species list grows as new names are typed.
    """

    def __init__(self, measurement: Measurement,
                 species_list: list[str],
                 parent=None):
        super().__init__(parent)
        self.setWindowTitle("Name This Measurement")
        self.setModal(True)
        self.setMinimumWidth(360)
        self._measurement = measurement
        self._species_list = species_list  # shared list — we append to it

        layout = QVBoxLayout(self)
        layout.setSpacing(10)

        # Result summary
        type_names = {"line": "Length", "polyline": "Length (polyline)", "polygon": "Area"}
        type_label = type_names.get(measurement.type, measurement.type.capitalize())
        unit_str = f"{measurement.unit}²" if measurement.type == "polygon" else measurement.unit
        result_text = f"<b>{type_label}:</b> {measurement.value:.3f} {unit_str}"
        if measurement.type == "polygon" and measurement.auto_width > 0:
            result_text += (
                f"<br><small>Width: {measurement.auto_width:.2f} {measurement.unit} &nbsp; "
                f"Height: {measurement.auto_height:.2f} {measurement.unit} &nbsp; "
                f"Perimeter: {measurement.perimeter_len:.2f} {measurement.unit}</small>"
            )
        result_lbl = QLabel(result_text)
        result_lbl.setAlignment(Qt.AlignmentFlag.AlignCenter)
        result_lbl.setStyleSheet("font-size: 13px; padding: 8px; background: #2a2a2a; border-radius: 6px;")
        result_lbl.setTextFormat(Qt.TextFormat.RichText)
        layout.addWidget(result_lbl)

        sep = QFrame()
        sep.setFrameShape(QFrame.Shape.HLine)
        layout.addWidget(sep)

        # Fragment name
        name_row = QHBoxLayout()
        name_row.addWidget(QLabel("Fragment name:"))
        self._name_edit = QLineEdit()
        self._name_edit.setPlaceholderText("e.g. Frag-01")
        name_row.addWidget(self._name_edit)
        layout.addLayout(name_row)

        # Species / genus — editable combo that grows with new entries
        species_row = QHBoxLayout()
        species_row.addWidget(QLabel("Spesies/Genus:"))
        self._species_combo = QComboBox()
        self._species_combo.setEditable(True)
        self._species_combo.setInsertPolicy(QComboBox.InsertPolicy.NoInsert)
        self._species_combo.addItem("")           # blank = not specified
        for s in species_list:
            self._species_combo.addItem(s)
        self._species_combo.setCurrentIndex(0)
        self._species_combo.lineEdit().setPlaceholderText("Type or select species…")
        species_row.addWidget(self._species_combo)
        layout.addLayout(species_row)

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

        species = self._species_combo.currentText().strip()
        self._measurement.species = species
        # Add to shared list if new and non-empty
        if species and species not in self._species_list:
            self._species_list.append(species)

        self.accept()

    def measurement(self) -> Measurement:
        return self._measurement
