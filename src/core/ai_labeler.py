from __future__ import annotations

from dataclasses import dataclass
from typing import TYPE_CHECKING

import cv2
import numpy as np

from PyQt6.QtCore import QThread, pyqtSignal

if TYPE_CHECKING:
    from src.models.project import ImageAnnotation

def yolo_available() -> bool:
    """Return True if ultralytics is importable (checked fresh each call)."""
    try:
        import ultralytics  # pylint: disable=import-outside-toplevel
        _ = ultralytics.__version__
        return True
    except ImportError:
        return False


@dataclass
class LabelResult:
    annotation_path: str
    point_index: int
    predicted_class: str
    mapped_code: str | None
    confidence: float


class AILabeler:
    """Wraps a YOLOv8 classification or detection model for per-point coral code prediction."""

    def __init__(self, model_path: str) -> None:
        from ultralytics import YOLO
        self._model = YOLO(model_path)
        self._class_names: dict[int, str] = self._model.names
        self._task: str = getattr(self._model, "task", "classify") or "classify"

    @property
    def task(self) -> str:
        return self._task

    def class_names(self) -> list[str]:
        return list(self._class_names.values())

    def _crop_around(
        self, image: np.ndarray, x: float, y: float, crop_size: int
    ) -> np.ndarray:
        half = crop_size // 2
        h, w = image.shape[:2]
        cx = min(max(int(x), 0), w - 1)
        cy = min(max(int(y), 0), h - 1)
        x0, x1 = max(0, cx - half), min(w, cx + half)
        y0, y1 = max(0, cy - half), min(h, cy + half)
        crop = image[y0:y1, x0:x1]
        pad_t = max(0, half - cy)
        pad_b = max(0, cy + half - h)
        pad_l = max(0, half - cx)
        pad_r = max(0, cx + half - w)
        if pad_t or pad_b or pad_l or pad_r:
            crop = cv2.copyMakeBorder(  # type: ignore[call-overload]  # pylint: disable=no-member
                crop, int(pad_t), int(pad_b), int(pad_l), int(pad_r),
                cv2.BORDER_CONSTANT, value=0,  # pylint: disable=no-member
            )
        return crop

    def predict_point(
        self,
        image: np.ndarray,
        x: float,
        y: float,
        crop_size: int = 64,
    ) -> tuple[str, float]:
        if self._task == "classify":
            return self._predict_classify(image, x, y, crop_size)
        return self._predict_detect(image, x, y, crop_size)

    def _predict_classify(
        self, image: np.ndarray, x: float, y: float, crop_size: int
    ) -> tuple[str, float]:
        crop = self._crop_around(image, x, y, crop_size)
        results = self._model(crop, verbose=False)
        probs = results[0].probs
        if probs is None:
            raise ValueError(
                "Model does not appear to be a classification model. "
                "Train with `yolo task=classify`."
            )
        class_name = self._class_names[int(probs.top1)]
        confidence = float(probs.top1conf)
        return class_name, confidence

    def _predict_detect(
        self, image: np.ndarray, x: float, y: float, crop_size: int
    ) -> tuple[str, float]:
        # Use a larger crop so nearby corals are visible for detection
        detect_size = max(crop_size * 3, 224)
        crop = self._crop_around(image, x, y, detect_size)
        results = self._model(crop, verbose=False)
        boxes = results[0].boxes
        if boxes is None or len(boxes) == 0:
            return "(no detection)", 0.0

        # Find the detection box whose center is closest to the point
        center = detect_size / 2.0
        best_idx = 0
        best_dist = float("inf")
        for i, box in enumerate(boxes.xywh):
            bx, by = float(box[0]), float(box[1])
            dist = ((bx - center) ** 2 + (by - center) ** 2) ** 0.5
            if dist < best_dist:
                best_dist = dist
                best_idx = i

        cls_id = int(boxes.cls[best_idx])
        confidence = float(boxes.conf[best_idx])
        return self._class_names[cls_id], confidence

    @staticmethod
    def suggest_mapping(
        class_names: list[str],
        coral_codes: dict[str, str],
    ) -> dict[str, str | None]:
        mapping: dict[str, str | None] = {}
        for cls in class_names:
            matched: str | None = None
            cls_lower = cls.lower().replace("_", " ")
            for code, desc in coral_codes.items():
                if cls_lower == code.lower() or cls_lower in desc.lower():
                    matched = code
                    break
            mapping[cls] = matched
        return mapping


class AILabelWorker(QThread):
    progress = pyqtSignal(int, int, str)
    result_ready = pyqtSignal(list)
    error = pyqtSignal(str)
    finished = pyqtSignal()

    def __init__(
        self,
        labeler: AILabeler,
        annotations: list[ImageAnnotation],
        class_mapping: dict[str, str | None],
        conf_threshold: float,
        crop_size: int,
        overwrite_labeled: bool,
        parent=None,
    ) -> None:
        super().__init__(parent)
        self._labeler = labeler
        self._annotations = annotations
        self._class_mapping = class_mapping
        self._conf_threshold = conf_threshold
        self._crop_size = crop_size
        self._overwrite_labeled = overwrite_labeled
        self._cancelled = False

    def cancel(self) -> None:
        self._cancelled = True

    def run(self) -> None:
        results: list[LabelResult] = []
        total = sum(
            len([p for p in a.points if self._overwrite_labeled or p.label is None])
            for a in self._annotations
        )
        done = 0

        try:
            current_path: str | None = None
            current_img: np.ndarray | None = None

            for ann in self._annotations:
                if self._cancelled:
                    break

                img_name = ann.image_path.split("/")[-1].split("\\")[-1]

                if ann.image_path != current_path:
                    current_img = cv2.imread(ann.image_path)
                    current_path = ann.image_path

                if current_img is None:
                    self.progress.emit(
                        done, total,
                        f"WARNING: could not read {img_name} — skipping",
                    )
                    continue

                for p in ann.points:
                    if self._cancelled:
                        break
                    if not self._overwrite_labeled and p.label is not None:
                        continue

                    try:
                        predicted_class, confidence = self._labeler.predict_point(
                            current_img, p.x, p.y, self._crop_size
                        )
                        mapped_code: str | None = self._class_mapping.get(predicted_class)
                        if confidence < self._conf_threshold:
                            mapped_code = None
                        results.append(LabelResult(
                            annotation_path=ann.image_path,
                            point_index=p.index,
                            predicted_class=predicted_class,
                            mapped_code=mapped_code,
                            confidence=confidence,
                        ))
                        point_status = (
                            f"{img_name} — Point #{p.index + 1}: "
                            f"{predicted_class} → {mapped_code or '(skip)'} "
                            f"({confidence:.1%})"
                        )
                    except Exception as exc:  # noqa: BLE001  # pylint: disable=broad-exception-caught
                        self.error.emit(f"{img_name} — Point #{p.index + 1}: {exc}")
                        point_status = f"{img_name} — Point #{p.index + 1}: (error)"

                    done += 1
                    self.progress.emit(done, total, point_status)

        except Exception as exc:  # pylint: disable=broad-exception-caught
            self.error.emit(str(exc))

        self.result_ready.emit(results)
        self.finished.emit()
