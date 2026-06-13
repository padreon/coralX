"""Welcome / mode-selection screen shown on app startup."""

from __future__ import annotations

from PyQt6.QtWidgets import (
    QDialog, QVBoxLayout, QHBoxLayout, QPushButton, QLabel, QFrame,
)
from PyQt6.QtCore import Qt
from PyQt6.QtGui import QFont


class WelcomeScreen(QDialog):
    """
    Blocking mode-selector shown before the main window is created.
    Call exec(); if Accepted, read get_mode() to determine which window to open.
    """

    def __init__(self, parent=None):
        super().__init__(parent)
        self.setWindowTitle("coralX")
        self.setMinimumSize(600, 380)
        self.setModal(True)
        self._mode: str = "point_count"
        self._build_ui()

    def get_mode(self) -> str:
        return self._mode

    def _build_ui(self):
        self.setStyleSheet("""
            QDialog { background: #1e1e1e; color: #eee; }
            QLabel  { color: #eee; background: transparent; }
        """)

        layout = QVBoxLayout(self)
        layout.setContentsMargins(40, 30, 40, 30)
        layout.setSpacing(20)
        layout.setAlignment(Qt.AlignmentFlag.AlignCenter)

        title = QLabel("coralX")
        f = QFont()
        f.setPointSize(28)
        f.setBold(True)
        title.setFont(f)
        title.setAlignment(Qt.AlignmentFlag.AlignCenter)
        title.setStyleSheet("color: #4fc3f7; background: transparent;")
        layout.addWidget(title)

        sub = QLabel("Choose a mode to begin")
        sub.setAlignment(Qt.AlignmentFlag.AlignCenter)
        sub.setStyleSheet("color: #888; background: transparent;")
        layout.addWidget(sub)

        cards = QHBoxLayout()
        cards.setSpacing(28)
        cards.setAlignment(Qt.AlignmentFlag.AlignCenter)
        cards.addWidget(self._card(
            "🎯", "Coral Point Count",
            "Random-point sampling\nfor benthic coverage\nestimation",
            "point_count",
        ))
        cards.addWidget(self._card(
            "📏", "Fragment Measurement",
            "Measure coral height &\narea for growth\nmonitoring",
            "measurement",
        ))
        layout.addLayout(cards)

    def _card(self, icon: str, title: str, desc: str, mode: str) -> QFrame:
        frame = QFrame()
        frame.setFixedSize(230, 170)
        frame.setStyleSheet("""
            QFrame {
                border: 2px solid #444;
                border-radius: 10px;
                background: #2a2a2a;
            }
            QFrame:hover {
                border-color: #4fc3f7;
                background: #2e3a42;
            }
            QLabel { background: transparent; color: #eee; }
        """)

        vl = QVBoxLayout(frame)
        vl.setAlignment(Qt.AlignmentFlag.AlignCenter)
        vl.setSpacing(6)

        ico = QLabel(icon)
        ico.setAlignment(Qt.AlignmentFlag.AlignCenter)
        fi = QFont(); fi.setPointSize(32)
        ico.setFont(fi)

        ttl = QLabel(title)
        ttl.setAlignment(Qt.AlignmentFlag.AlignCenter)
        ft = QFont(); ft.setPointSize(11); ft.setBold(True)
        ttl.setFont(ft)

        dsc = QLabel(desc)
        dsc.setAlignment(Qt.AlignmentFlag.AlignCenter)
        dsc.setStyleSheet("color: #aaa; font-size: 10px; background: transparent;")

        btn = QPushButton("Select")
        btn.setStyleSheet("""
            QPushButton {
                background: #4fc3f7; color: #111;
                border: none; border-radius: 4px;
                padding: 4px 16px; font-weight: bold;
            }
            QPushButton:hover { background: #81d4fa; }
        """)
        btn.clicked.connect(lambda: self._select(mode))

        for w in (ico, ttl, dsc, btn):
            vl.addWidget(w)

        return frame

    def _select(self, mode: str):
        self._mode = mode
        self.accept()
