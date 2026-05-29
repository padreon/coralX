"""
Download a Roboflow object-detection dataset, crop each bounding box into a
per-class image, and train a YOLOv8 classification model ready for coralX.

Quick start
-----------
1. Fill in tools/training_config.yaml with your Roboflow details.
2. Install dependencies:
       pip install roboflow ultralytics opencv-python pyyaml
3. Run:
       python tools/prepare_and_train.py

CLI args override config file values when provided.

Step flags (useful when resuming):
    --skip-download   reuse an existing raw/ folder
    --skip-crop       reuse an existing classify/ folder
"""
from __future__ import annotations

import argparse
import sys
from pathlib import Path

try:
    import cv2  # type: ignore[import-untyped]
except ImportError:
    sys.exit("opencv-python is not installed.  Run: pip install opencv-python")

try:
    import yaml  # type: ignore[import-untyped]
except ImportError:
    sys.exit("pyyaml is not installed.  Run: pip install pyyaml")

_CONFIG_PATH = Path(__file__).parent / "training_config.yaml"


# ---------------------------------------------------------------------------
# Config
# ---------------------------------------------------------------------------

def load_config(path: Path) -> dict:
    """Load training_config.yaml and return a flat dict of settings."""
    if not path.exists():
        return {}
    raw = yaml.safe_load(path.read_text()) or {}
    rf = raw.get("roboflow", {})
    tr = raw.get("training", {})
    aug = raw.get("augmentation", {})
    return {
        "api_key":    rf.get("api_key", ""),
        "workspace":  rf.get("workspace", ""),
        "project":    rf.get("project", ""),
        "version":    rf.get("version", 1),
        "epochs":     tr.get("epochs", 100),
        "imgsz":      tr.get("imgsz", 64),
        "batch":      tr.get("batch", 32),
        "base_model": tr.get("base_model", "yolov8n-cls.pt"),
        "work_dir":   tr.get("work_dir", "training_data"),
        "aug_fliplr":  aug.get("fliplr",  0.5),
        "aug_flipud":  aug.get("flipud",  0.3),
        "aug_degrees": aug.get("degrees", 15.0),
        "aug_hsv_h":   aug.get("hsv_h",   0.015),
        "aug_hsv_s":   aug.get("hsv_s",   0.4),
        "aug_hsv_v":   aug.get("hsv_v",   0.3),
        "aug_blur":    aug.get("blur",    0.1),
    }


# ---------------------------------------------------------------------------
# 1. Download
# ---------------------------------------------------------------------------

def download_dataset(
    api_key: str,
    workspace: str,
    project: str,
    version: int,
    out_dir: Path,
) -> Path:
    """Download a Roboflow project in YOLOv8-detection format."""
    try:
        from roboflow import Roboflow  # type: ignore[import-untyped]
    except ImportError:
        sys.exit("roboflow is not installed.  Run: pip install roboflow")

    print(f"Downloading {workspace}/{project} v{version} from Roboflow…")
    rf = Roboflow(api_key=api_key)
    dataset = (
        rf.workspace(workspace)
        .project(project)
        .version(version)
        .download("yolov8", location=str(out_dir))
    )
    return Path(dataset.location)


# ---------------------------------------------------------------------------
# 2. Crop bounding boxes → per-class folders
# ---------------------------------------------------------------------------

def _read_class_names(dataset_dir: Path) -> list[str]:
    """Read class names from data.yaml."""
    data = yaml.safe_load((dataset_dir / "data.yaml").read_text())
    return data["names"]


def _bbox_to_pixels(
    cx: float, cy: float, bw: float, bh: float, img_w: int, img_h: int
) -> tuple[int, int, int, int]:
    """Convert YOLO normalised bbox to pixel coordinates (x1, y1, x2, y2)."""
    x1 = max(0, int((cx - bw / 2) * img_w))
    y1 = max(0, int((cy - bh / 2) * img_h))
    x2 = min(img_w, int((cx + bw / 2) * img_w))
    y2 = min(img_h, int((cy + bh / 2) * img_h))
    return x1, y1, x2, y2


def _find_image(images_dir: Path, stem: str) -> Path | None:
    """Return the first image file matching stem, or None."""
    for ext in (".jpg", ".jpeg", ".png", ".JPG", ".JPEG", ".PNG"):
        candidate = images_dir / (stem + ext)
        if candidate.exists():
            return candidate
    return None


def _crop_label_file(
    label_file: Path,
    img: "cv2.Mat",
    img_w: int,
    img_h: int,
    class_names: list[str],
    out_dir: Path,
) -> int:
    """Crop all boxes in one label file and return the number saved."""
    saved = 0
    for i, line in enumerate(label_file.read_text().strip().splitlines()):
        parts = line.split()
        if len(parts) < 5:
            continue
        cls_id = int(parts[0])
        x1, y1, x2, y2 = _bbox_to_pixels(
            float(parts[1]), float(parts[2]),
            float(parts[3]), float(parts[4]),
            img_w, img_h,
        )
        crop = img[y1:y2, x1:x2]
        if crop.size == 0:
            continue
        cls_name = class_names[cls_id] if cls_id < len(class_names) else str(cls_id)
        dest = out_dir / cls_name
        dest.mkdir(parents=True, exist_ok=True)
        cv2.imwrite(str(dest / f"{label_file.stem}_{i}.jpg"), crop)
        saved += 1
    return saved


def _crop_split(
    images_dir: Path,
    labels_dir: Path,
    out_dir: Path,
    class_names: list[str],
) -> int:
    """Crop every annotated bounding box and save under out_dir/<class>/."""
    count = 0
    for label_file in sorted(labels_dir.glob("*.txt")):
        img_path = _find_image(images_dir, label_file.stem)
        if img_path is None:
            print(f"  [skip] no image found for {label_file.name}")
            continue
        img = cv2.imread(str(img_path))
        if img is None:
            print(f"  [skip] could not read {img_path.name}")
            continue
        img_h, img_w = img.shape[:2]
        count += _crop_label_file(label_file, img, img_w, img_h, class_names, out_dir)
    return count


def build_classification_dataset(dataset_dir: Path, cls_dir: Path) -> None:
    """Convert a YOLOv8 detection dataset into a classification folder tree."""
    class_names = _read_class_names(dataset_dir)
    print(f"Classes found: {class_names}")
    for split in ("train", "valid", "test"):
        images_dir = dataset_dir / split / "images"
        labels_dir = dataset_dir / split / "labels"
        if not images_dir.exists() or not labels_dir.exists():
            print(f"  [skip] split '{split}' not present")
            continue
        n = _crop_split(images_dir, labels_dir, cls_dir / split, class_names)
        print(f"  {split}: {n} crops saved → {cls_dir / split}")


# ---------------------------------------------------------------------------
# 3. Train YOLOv8 classification
# ---------------------------------------------------------------------------

def train(
    cls_dir: Path,
    epochs: int,
    imgsz: int,
    batch: int,
    base_model: str,
    aug_fliplr: float,
    aug_flipud: float,
    aug_degrees: float,
    aug_hsv_h: float,
    aug_hsv_s: float,
    aug_hsv_v: float,
    aug_blur: float,
) -> Path:
    """Train a YOLOv8 classification model and return the path to best.pt."""
    try:
        from ultralytics import YOLO  # type: ignore[import-untyped]
    except ImportError:
        sys.exit("ultralytics is not installed.  Run: pip install ultralytics")

    print(f"\nStarting training ({epochs} epochs, imgsz={imgsz}, batch={batch})…")
    model = YOLO(base_model)
    model.train(
        task="classify",
        data=str(cls_dir),
        epochs=epochs,
        imgsz=imgsz,
        batch=batch,
        name="coral_classifier",
        exist_ok=True,
        # Augmentation — tuned for underwater coral imagery
        fliplr=aug_fliplr,
        flipud=aug_flipud,
        degrees=aug_degrees,
        hsv_h=aug_hsv_h,
        hsv_s=aug_hsv_s,
        hsv_v=aug_hsv_v,
        blur=aug_blur,
    )
    best = Path("runs/classify/coral_classifier/weights/best.pt")
    if best.exists():
        print(f"\nDone. Model saved to: {best.resolve()}")
        print("Load this file in coralX → AI Auto-Label → Browse…")
    else:
        print("\nTraining finished but best.pt was not found — check the output above.")
    return best


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def _parse_args(cfg: dict) -> argparse.Namespace:
    """Parse CLI arguments, using config file values as defaults."""
    p = argparse.ArgumentParser(
        description=(
            "Download a Roboflow detection dataset, crop bounding boxes, "
            "and train a YOLOv8 classification model for coralX."
        )
    )
    p.add_argument("--api-key",    default=cfg.get("api_key"),    help="Roboflow API key")
    p.add_argument("--workspace",  default=cfg.get("workspace"),  help="Roboflow workspace slug")
    p.add_argument("--project",    default=cfg.get("project"),    help="Roboflow project slug")
    p.add_argument("--version",    default=cfg.get("version", 1), type=int)
    p.add_argument("--work-dir",   default=cfg.get("work_dir", "training_data"))
    p.add_argument("--epochs",     default=cfg.get("epochs", 100),      type=int)
    p.add_argument("--imgsz",      default=cfg.get("imgsz", 64),        type=int)
    p.add_argument("--batch",      default=cfg.get("batch", 32),        type=int)
    p.add_argument("--base-model", default=cfg.get("base_model", "yolov8n-cls.pt"))
    p.add_argument("--aug-fliplr",  default=cfg.get("aug_fliplr",  0.5),  type=float)
    p.add_argument("--aug-flipud",  default=cfg.get("aug_flipud",  0.3),  type=float)
    p.add_argument("--aug-degrees", default=cfg.get("aug_degrees", 15.0), type=float)
    p.add_argument("--aug-hsv-h",   default=cfg.get("aug_hsv_h",   0.015),type=float)
    p.add_argument("--aug-hsv-s",   default=cfg.get("aug_hsv_s",   0.4),  type=float)
    p.add_argument("--aug-hsv-v",   default=cfg.get("aug_hsv_v",   0.3),  type=float)
    p.add_argument("--aug-blur",    default=cfg.get("aug_blur",    0.1),  type=float)
    p.add_argument("--skip-download", action="store_true")
    p.add_argument("--skip-crop",     action="store_true")
    return p.parse_args()


def main() -> None:
    """Entry point."""
    cfg = load_config(_CONFIG_PATH)
    args = _parse_args(cfg)

    missing = [f for f in ("api_key", "workspace", "project") if not getattr(args, f.replace("_", "-"), None) and not cfg.get(f)]
    if missing and not args.skip_download:
        sys.exit(
            f"Missing required fields: {missing}\n"
            f"Fill them in {_CONFIG_PATH} or pass them as CLI arguments."
        )

    work_dir = Path(args.work_dir)
    raw_dir = work_dir / "raw"
    cls_dir = work_dir / "classify"

    if not args.skip_download:
        dataset_dir = download_dataset(
            args.api_key, args.workspace, args.project, args.version, raw_dir
        )
    else:
        candidates = list(raw_dir.glob("*/data.yaml"))
        if not candidates:
            sys.exit(f"--skip-download set but no data.yaml found under {raw_dir}")
        dataset_dir = candidates[0].parent
        print(f"Using existing dataset: {dataset_dir}")

    if not args.skip_crop:
        build_classification_dataset(dataset_dir, cls_dir)
    else:
        print(f"Using existing crops: {cls_dir}")

    train(
        cls_dir,
        args.epochs,
        args.imgsz,
        args.batch,
        args.base_model,
        aug_fliplr=args.aug_fliplr,
        aug_flipud=args.aug_flipud,
        aug_degrees=args.aug_degrees,
        aug_hsv_h=args.aug_hsv_h,
        aug_hsv_s=args.aug_hsv_s,
        aug_hsv_v=args.aug_hsv_v,
        aug_blur=args.aug_blur,
    )


if __name__ == "__main__":
    main()
