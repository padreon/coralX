"""One-time conversion of the YOLO AI-labeler checkpoint (.pt) to ONNX for the
Rust rewrite's `ai_labeler` module (uses the `ort` crate, not libtorch).

Usage:
    .venv/bin/python tools/export_ai_model_onnx.py data/data-training.pt

Exports at the model's own embedded training resolution (read from
`model.model.transforms`) rather than a hardcoded size — using the wrong
`imgsz` silently changes prediction confidences without changing the top-1
class, so it's easy to miss. See the Rust module's doc comment for how this
was verified against the original PyTorch output.
"""
import sys

from ultralytics import YOLO


def main() -> None:
    if len(sys.argv) != 2:
        print(f"Usage: {sys.argv[0]} <model.pt>")
        raise SystemExit(1)

    model_path = sys.argv[1]
    model = YOLO(model_path)

    imgsz = 224
    transforms = getattr(model.model, "transforms", None)
    if transforms is not None and hasattr(transforms.transforms[0], "size"):
        imgsz = transforms.transforms[0].size
        print(f"Detected training resolution from embedded transforms: {imgsz}px")
    else:
        print(f"Could not detect training resolution; defaulting to {imgsz}px "
              "(check this is correct for a detect-task model).")

    out_path = model.export(format="onnx", imgsz=imgsz, opset=17)
    print(f"Exported: {out_path}")
    print(f"Task: {model.task}, classes: {model.names}")


if __name__ == "__main__":
    main()
