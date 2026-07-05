# coralX (Rust rewrite) — experimental

An in-progress Rust port of coralX, replacing Python/PyQt6/OpenCV with a
single native binary: [egui](https://github.com/emilk/egui)/eframe for the
GUI, pure-Rust image processing (`image`/`imageproc`/`geo`, no OpenCV), and
[`ort`](https://github.com/pykeio/ort) (ONNX Runtime) for the AI labeler
instead of libtorch. See the commit history on `feat/rust-rewrite` for the
module-by-module port notes and the tradeoffs behind each dependency choice.

**Status:** every module has a working Rust equivalent and the whole thing
compiles and runs, but the interactive UI flows haven't been exercised by
hand yet — see "Known gaps" below before relying on this for real survey
data. The Python app in `src/` is still the production version.

## Prerequisites

- Rust (stable), via [rustup](https://rustup.rs)
- On Linux, the same X11/GUI system libraries the Python/PyQt6 build needs
  (see `../setup.sh`) plus a few winit/rfd needs:
  ```
  sudo apt-get install -y libxcursor1 libxrandr2 libxi6 libgtk-3-dev
  ```
  `libgtk-3-dev` is for native file dialogs (open/save project, add images,
  every export flow). rfd's default Linux backend talks to the XDG Desktop
  Portal over D-Bus, which needs a full desktop session (dbus + a portal
  backend like `xdg-desktop-portal-gtk`) to work — that's not present on a
  minimal window-manager-only setup like this devcontainer's Fluxbox, so
  every file dialog would silently do nothing. This project pins rfd to its
  GTK3 backend instead (see `Cargo.toml`), which talks to GTK directly with
  no D-Bus/portal service required.

No Python, OpenCV, or libtorch is required to build or run this — those
were the whole point of the rewrite. Python is only needed once, separately,
if you retrain the AI-labeler model and need to re-export it (see below).

## Running

```bash
cd rust
cargo run
```

First build pulls a fairly large dependency tree (egui/eframe's windowing
stack, ONNX Runtime binaries) — expect a few minutes on a cold cache, then
normal incremental-build speed after.

## Re-exporting the AI-labeler model

The AI labeler loads `data/data-training.onnx`, which is already committed
(converted from the existing `data/data-training.pt`). If you retrain the
model, regenerate the ONNX file with:

```bash
.venv/bin/python tools/export_ai_model_onnx.py data/data-training.pt
```

This reads the model's *actual* training resolution from its embedded
transforms rather than assuming a fixed size — using the wrong size silently
changes prediction confidences without changing the top-1 class, so it's an
easy mistake to make by hand.

## Known gaps

- No async thumbnail loading in the image tree (text + checkmark only).
- No splitter-size persistence across sessions.
- Magic-wand hole punch-through (donut-shaped selections) renders solid
  instead of with a visible hole — contour hierarchy isn't ported.
- The AI labeler's detect-model code path follows the documented Ultralytics
  export contract but is unvalidated — no detect-task model exists in this
  repo to test against (the shipped model is classify-only, and *that* path
  is cross-checked against the original PyTorch output).
- Interactive workflows (point labeling, station management, drawing tools,
  import merges) are implemented and compile clean but haven't been driven
  by hand end-to-end — do a real pass over each screen before trusting it
  with real data.
