"""Reusable progress dialog and background worker thread for coralX.

Usage:
    dlg = ProgressDialog("Mengekspor Excel…", total=12, parent=self)
    worker = WorkerThread(lambda cb: do_heavy_work(progress_cb=cb))
    worker.progress.connect(dlg.update)
    worker.succeeded.connect(lambda _: dlg.accept())
    worker.failed.connect(lambda msg: (dlg.reject(), show_error(msg)))
    worker.start()
    dlg.exec()
"""

from __future__ import annotations

from PyQt6.QtCore import QThread, pyqtSignal
from PyQt6.QtWidgets import (
    QDialog, QVBoxLayout, QHBoxLayout,
    QLabel, QProgressBar, QPushButton,
)


class ProgressDialog(QDialog):
    """Modal progress dialog with a status line, counter, and progress bar.

    Call update(done, total, msg) from the main thread (via signal) to refresh.
    """

    cancelled = pyqtSignal()

    def __init__(
        self,
        title: str,
        total: int = 0,
        cancellable: bool = False,
        parent=None,
    ) -> None:
        super().__init__(parent)
        self.setWindowTitle(title)
        self.setMinimumWidth(420)
        # Prevent user from accidentally closing the dialog
        self.setWindowFlag(self.windowFlags().__class__.WindowCloseButtonHint, False)  # type: ignore[arg-type]

        layout = QVBoxLayout(self)
        layout.setSpacing(10)
        layout.setContentsMargins(20, 16, 20, 16)

        self._status = QLabel("Starting…")
        self._status.setWordWrap(True)
        layout.addWidget(self._status)

        self._counter = QLabel("")
        self._counter.setStyleSheet("color: #888; font-size: 11px;")
        layout.addWidget(self._counter)

        self._bar = QProgressBar()
        self._bar.setRange(0, max(total, 1))
        self._bar.setValue(0)
        self._bar.setTextVisible(True)
        layout.addWidget(self._bar)

        if cancellable:
            btn_row = QHBoxLayout()
            btn_row.addStretch()
            self._cancel_btn = QPushButton("Cancel")
            self._cancel_btn.clicked.connect(self._on_cancel)
            btn_row.addWidget(self._cancel_btn)
            layout.addLayout(btn_row)
        else:
            self._cancel_btn = None  # type: ignore[assignment]

        self._total = total
        self._cancelled = False

    def update(self, done: int, total: int, msg: str) -> None:
        """Refresh the dialog — safe to call from the main thread via signal."""
        if total != self._total and total > 0:
            self._total = total
            self._bar.setRange(0, total)
        self._bar.setValue(done)
        self._status.setText(msg)
        if total > 0:
            self._counter.setText(f"{done} / {total}")
        else:
            self._counter.setText("")

    def set_indeterminate(self, msg: str = "") -> None:
        """Switch to a bouncing-bar (unknown duration) mode."""
        self._bar.setRange(0, 0)
        if msg:
            self._status.setText(msg)
        self._counter.setText("")

    def _on_cancel(self) -> None:
        self._cancelled = True
        self.cancelled.emit()
        if self._cancel_btn:
            self._cancel_btn.setEnabled(False)
            self._cancel_btn.setText("Cancelling…")

    @property
    def is_cancelled(self) -> bool:
        return self._cancelled


class WorkerThread(QThread):
    """Run a callable in a background thread and report progress + result.

    The callable receives one argument: a ``progress_cb`` function with
    signature ``progress_cb(done: int, total: int, msg: str)``.

    Example::

        def heavy(cb):
            for i, item in enumerate(items):
                process(item)
                cb(i + 1, len(items), f"Processing {item.name}")

        worker = WorkerThread(heavy)
        worker.progress.connect(dlg.update)
        worker.succeeded.connect(on_done)
        worker.failed.connect(on_error)
        worker.start()
    """

    progress = pyqtSignal(int, int, str)   # done, total, status_msg
    succeeded = pyqtSignal(object)          # result (may be None)
    failed = pyqtSignal(str)               # error message

    def __init__(self, fn, parent=None) -> None:
        super().__init__(parent)
        self._fn = fn

    def _emit_progress(self, done: int, total: int, msg: str = "") -> None:
        self.progress.emit(done, total, msg)

    def run(self) -> None:
        try:
            result = self._fn(self._emit_progress)
            self.succeeded.emit(result)
        except Exception as exc:  # pylint: disable=broad-exception-caught
            self.failed.emit(str(exc))
