"""Logging setup and crash-handler utilities for coralX."""
from __future__ import annotations

import logging
import logging.handlers
import os
import platform
import sys
import threading
import traceback
from pathlib import Path
from types import TracebackType

try:
    from PyQt6.QtCore import QMessageLogContext, QtMsgType, qInstallMessageHandler
    from PyQt6.QtWidgets import QApplication, QMessageBox
    _QT_AVAILABLE = True
except ImportError:
    _QT_AVAILABLE = False


class _State:
    log_file_path: Path | None = None
    initialized: bool = False


def _log_dir() -> Path:
    system = platform.system()
    if system == "Windows":
        base = Path(os.environ.get("APPDATA", str(Path.home())))
    elif system == "Darwin":
        base = Path.home() / "Library" / "Logs"
    else:
        base = Path(os.environ.get("XDG_DATA_HOME", str(Path.home() / ".local" / "share")))
    return base / "coralX"


def log_path() -> Path:
    """Return the path to the active log file."""
    return _State.log_file_path if _State.log_file_path is not None else _log_dir() / "coralX.log"


def setup_logging(level: int = logging.DEBUG) -> None:
    """Configure root logger with a rotating file handler plus a stderr handler for WARNING+."""
    if _State.initialized:
        return

    fmt = logging.Formatter(
        "%(asctime)s [%(levelname)-8s] %(name)s: %(message)s",
        datefmt="%Y-%m-%d %H:%M:%S",
    )

    root = logging.getLogger()
    root.setLevel(level)

    # File handler — best-effort; fall back to stderr-only on permission/IO errors.
    try:
        log_dir = _log_dir()
        log_dir.mkdir(parents=True, exist_ok=True)
        _State.log_file_path = log_dir / "coralX.log"
        fh = logging.handlers.RotatingFileHandler(
            _State.log_file_path, maxBytes=1_000_000, backupCount=3, encoding="utf-8"
        )
        fh.setLevel(logging.DEBUG)
        fh.setFormatter(fmt)
        root.addHandler(fh)
    except Exception:  # pylint: disable=broad-exception-caught
        print(
            f"[coralX] WARNING: could not create log file at "
            f"{_log_dir() / 'coralX.log'} — logging to stderr only.",
            file=sys.stderr,
        )

    sh = logging.StreamHandler(sys.stderr)
    sh.setLevel(logging.WARNING)
    sh.setFormatter(fmt)
    root.addHandler(sh)

    _State.initialized = True


def get_logger(name: str) -> logging.Logger:
    """Return a named logger, creating it if necessary."""
    return logging.getLogger(name)


def install_excepthook() -> None:
    """Log unhandled exceptions from the main thread and worker threads.

    sys.excepthook covers the main thread; threading.excepthook covers
    threading.Thread instances. Qt dialog is only shown from the main thread
    because Qt UI calls from other threads are unsafe.
    """
    _orig_sys = sys.excepthook
    _orig_thread = getattr(threading, "excepthook", None)
    _crash_log = logging.getLogger("coralX.crash")

    def _log_and_show(
        exc_type: type[BaseException],
        exc_value: BaseException,
        exc_tb: TracebackType | None,
        thread_name: str | None = None,
    ) -> None:
        prefix = f" in thread '{thread_name}'" if thread_name else ""
        _crash_log.critical(
            "Unhandled exception%s", prefix, exc_info=(exc_type, exc_value, exc_tb)
        )
        # Qt UI calls are only safe from the main thread.
        if thread_name is None:
            try:
                if _QT_AVAILABLE and QApplication.instance() is not None:
                    tb_text = "".join(traceback.format_exception(exc_type, exc_value, exc_tb))
                    msg = QMessageBox()
                    msg.setWindowTitle("coralX — Unexpected Error")
                    msg.setIcon(QMessageBox.Icon.Critical)
                    msg.setText(
                        "An unexpected error occurred.\n\n"
                        f"<b>{exc_type.__name__}:</b> {exc_value}"
                    )
                    msg.setInformativeText(f"Full details saved to:\n{log_path()}")
                    msg.setDetailedText(tb_text)
                    msg.exec()
            except Exception:  # pylint: disable=broad-exception-caught
                pass

    def _hook(
        exc_type: type[BaseException],
        exc_value: BaseException,
        exc_tb: TracebackType | None,
    ) -> None:
        if issubclass(exc_type, KeyboardInterrupt):
            _orig_sys(exc_type, exc_value, exc_tb)
            return
        _log_and_show(exc_type, exc_value, exc_tb, thread_name=None)
        _orig_sys(exc_type, exc_value, exc_tb)

    def _thread_hook(args: threading.ExceptHookArgs) -> None:
        if args.exc_type is None or issubclass(args.exc_type, KeyboardInterrupt):
            if _orig_thread is not None:
                _orig_thread(args)
            return
        exc_value = args.exc_value
        if exc_value is None:
            return
        # Python 3.13 renamed exc_tb → exc_traceback; support both.
        exc_tb: TracebackType | None = getattr(
            args, "exc_traceback", getattr(args, "exc_tb", None)
        )
        name = args.thread.name if args.thread is not None else "unknown"
        _log_and_show(args.exc_type, exc_value, exc_tb, thread_name=name)
        if _orig_thread is not None:
            _orig_thread(args)

    sys.excepthook = _hook
    threading.excepthook = _thread_hook


def install_qt_message_handler() -> None:
    """Forward Qt debug/warning/critical messages into the Python logger."""
    _qt_log = logging.getLogger("coralX.qt")
    _level_map = {
        QtMsgType.QtDebugMsg: logging.DEBUG,
        QtMsgType.QtInfoMsg: logging.INFO,
        QtMsgType.QtWarningMsg: logging.WARNING,
        QtMsgType.QtCriticalMsg: logging.ERROR,
        QtMsgType.QtFatalMsg: logging.CRITICAL,
    }

    def _handler(msg_type: QtMsgType, context: QMessageLogContext, message: str | None) -> None:
        level = _level_map.get(msg_type, logging.WARNING)
        loc = f"{context.file or '?'}:{context.line or 0}"
        _qt_log.log(level, "%s  (%s)", message or "", loc)

    qInstallMessageHandler(_handler)
