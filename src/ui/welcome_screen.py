"""Welcome / mode-selection screen shown on app startup."""

from __future__ import annotations

from PyQt6.QtWidgets import (
    QWidget, QVBoxLayout, QHBoxLayout, QPushButton, QLabel,
    QFrame, QSizePolicy,
)
from PyQt6.QtCore import Qt, pyqtSignal
from PyQt6.QtGui import QFont


class _ModeCard(QFrame):
    """Clickable card that represents one app mode."""

    clicked = pyqtSignal()

    def __init__(self, icon: str, title: str, description: str, parent=None):
        super().__init__(parent)
        self.setFrameShape(QFrame.Shape.StyledPanel)
        self.setLineWidth(2)
        self.setCursor(Qt.CursorShape.PointingHandCursor)
        self.setFixedSize(260, 180)
        self.setStyleSheet("""
            _ModeCard {
                border: 2px solid #444;
                border-radius: 10px;
                background: #2a2a2a;
            }
            _ModeCard:hover {
                border-color: #4fc3f7;
                background: #2e3a42;
            }
        """)

        layout = QVBoxLayout(self)
        layout.setAlignment(Qt.AlignmentFlag.AlignCenter)
        layout.setSpacing(8)

        icon_lbl = QLabel(icon)
        icon_lbl.setAlignment(Qt.AlignmentFlag.AlignCenter)
        icon_font = QFont()
        icon_font.setPointSize(36)
        icon_lbl.setFont(icon_font)

        title_lbl = QLabel(title)
        title_lbl.setAlignment(Qt.AlignmentFlag.AlignCenter)
        title_font = QFont()
        title_font.setPointSize(13)
        title_font.setBold(True)
        title_lbl.setFont(title_font)

        desc_lbl = QLabel(description)
        desc_lbl.setAlignment(Qt.AlignmentFlag.AlignCenter)
        desc_lbl.setWordWrap(True)
        desc_lbl.setStyleSheet("color: #aaa;")

        for w in (icon_lbl, title_lbl, desc_lbl):
            layout.addWidget(w)

    def mousePressEvent(self, event):
        if event.button() == Qt.MouseButton.LeftButton:
            self.clicked.emit()
        super().mousePressEvent(event)

    def enterEvent(self, event):
        self.setStyleSheet("""
            _ModeCard {
                border: 2px solid #4fc3f7;
                border-radius: 10px;
                background: #2e3a42;
            }
        """)
        super().enterEvent(event)

    def leaveEvent(self, event):
        self.setStyleSheet("""
            _ModeCard {
                border: 2px solid #444;
                border-radius: 10px;
                background: #2a2a2a;
            }
        """)
        super().leaveEvent(event)


class WelcomeScreen(QWidget):
    """Mode selector shown on startup. Emits mode_selected('point_count'|'measurement')."""

    mode_selected = pyqtSignal(str)  # 'point_count' or 'measurement'

    def __init__(self, parent=None):
        super().__init__(parent)
        self.setWindowTitle("coralX")
        self.setMinimumSize(640, 400)
        self.setStyleSheet("background: #1e1e1e; color: #eee;")

        layout = QVBoxLayout(self)
        layout.setAlignment(Qt.AlignmentFlag.AlignCenter)
        layout.setSpacing(24)

        # Logo / title
        title_lbl = QLabel("coralX")
        title_font = QFont()
        title_font.setPointSize(28)
        title_font.setBold(True)
        title_lbl.setFont(title_font)
        title_lbl.setAlignment(Qt.AlignmentFlag.AlignCenter)
        title_lbl.setStyleSheet("color: #4fc3f7;")
        layout.addWidget(title_lbl)

        sub_lbl = QLabel("Coral Reef Research Tool — choose a mode to begin")
        sub_lbl.setAlignment(Qt.AlignmentFlag.AlignCenter)
        sub_lbl.setStyleSheet("color: #888;")
        layout.addWidget(sub_lbl)

        # Mode cards
        cards_row = QHBoxLayout()
        cards_row.setAlignment(Qt.AlignmentFlag.AlignCenter)
        cards_row.setSpacing(32)

        card_point = _ModeCard(
            "🎯",
            "Coral Point Count",
            "Random point sampling\nfor benthic coverage\nestimation",
        )
        card_measure = _ModeCard(
            "📏",
            "Fragment Measurement",
            "Measure coral height\nand area for growth\nmonitoring",
        )
        card_point.clicked.connect(lambda: self.mode_selected.emit("point_count"))
        card_measure.clicked.connect(lambda: self.mode_selected.emit("measurement"))

        cards_row.addWidget(card_point)
        cards_row.addWidget(card_measure)
        layout.addLayout(cards_row)

        version_lbl = QLabel("v2.0")
        version_lbl.setAlignment(Qt.AlignmentFlag.AlignCenter)
        version_lbl.setStyleSheet("color: #555; font-size: 10px;")
        layout.addWidget(version_lbl)
