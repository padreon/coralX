import sys
import os

# xcb is required on headless Linux (Codespaces/CI); Windows and macOS
# auto-detect the correct platform plugin so we must not override them.
if sys.platform.startswith("linux"):
    os.environ.setdefault("QT_QPA_PLATFORM", "xcb")

from PyQt6.QtWidgets import QApplication
from PyQt6.QtGui import QFont
from src.core.logger import get_logger, install_excepthook, install_qt_message_handler, setup_logging
from src.ui.main_window import MainWindow
from src.ui.measurement_window import MeasurementWindow
from src.ui.welcome_screen import WelcomeScreen


def main():
    setup_logging()
    install_excepthook()
    log = get_logger("coralX.main")

    app = QApplication(sys.argv)
    app.setApplicationName("coralX")
    app.setOrganizationName("coralX")
    install_qt_message_handler()

    log.info("coralX starting (Python %s)", sys.version.split()[0])

    font = QFont("Segoe UI", 10)
    app.setFont(font)

    welcome = WelcomeScreen()
    main_win: list = []  # mutable container so lambda can capture it

    def _on_mode(mode: str):
        welcome.close()
        if mode == "measurement":
            w = MeasurementWindow()
        else:
            w = MainWindow()
        main_win.append(w)
        w.show()
        w.raise_()
        w.activateWindow()

    welcome.mode_selected.connect(_on_mode)
    welcome.show()

    exit_code = app.exec()
    log.info("coralX exiting (code %d)", exit_code)
    sys.exit(exit_code)


if __name__ == "__main__":
    main()
