# Contributing to coralX

Thank you for your interest in contributing! This guide covers how to set up the development environment, the code conventions we follow, and how to submit changes.

---

## Table of Contents

1. [Development Setup](#1-development-setup)
2. [Project Architecture](#2-project-architecture)
3. [Code Conventions](#3-code-conventions)
4. [Running the App](#4-running-the-app)
5. [Linting and Type Checking](#5-linting-and-type-checking)
6. [Submitting a Pull Request](#6-submitting-a-pull-request)
7. [Areas Open for Contribution](#7-areas-open-for-contribution)

---

## 1. Development Setup

```bash
# Fork and clone
git clone https://github.com/YOUR_USERNAME/coralx
cd coralx

# Create a virtual environment
python -m venv venv
source venv/bin/activate      # macOS / Linux
venv\Scripts\activate         # Windows

# Install all dependencies including optional ones
pip install -r requirements.txt
pip install ruff mypy pylint   # linting tools
```

---

## 2. Project Architecture

```
src/
├── main.py               Entry point — sets QT_QPA_PLATFORM, starts QApplication
├── ui/
│   ├── main_window.py    App shell: menus, toolbar, panels, all file/export actions
│   ├── image_canvas.py   Image viewer: QPainter overlay, mouse events, signals
│   ├── ai_label_dialog.py  AI auto-label configuration + progress dialog
│   ├── import_dialogs.py   CPCe import UI
│   └── calibration_dialog.py
├── core/
│   ├── point_generator.py  Generates list[Point] — random, stratified, uniform
│   ├── statistics.py       Coverage %, Shannon H', Simpson 1-D
│   ├── exporter.py         CSV and Excel export via pandas + openpyxl
│   ├── importer.py         CPCe / CSV import
│   ├── ai_labeler.py       YOLOv8 wrapper (AILabeler) + QThread worker (AILabelWorker)
│   └── analysis.py
└── models/
    └── project.py          Dataclasses: Point, ImageAnnotation, Station, Project
```

### Data flow

```
User opens image
  → ImageAnnotation created (image_path, width, height)
  → generate_points() → list[Point] (x, y, index, label=None)
  → ImageCanvas renders image + points via QPainter over QPixmap
  → User clicks point → QMenu → selects coral code
  → Point.label set → point_labeled signal emitted
  → MainWindow updates progress bar + quick stats
  → Export → exporter.py reads Project.annotations → writes CSV / Excel
```

### Key design decisions

- **Coordinates** — `Point.x` / `Point.y` are image-space pixels (not screen pixels). `ImageCanvas` uses `QTransform` to map between spaces. Hit-testing in `_hit_point()` converts screen coords back to image space before comparing.
- **Signals over direct calls** — `ImageCanvas` never calls `MainWindow` methods directly. It emits signals (`point_labeled`, `point_selected`, `border_defined`, `status_message`). `MainWindow` connects to these.
- **No controller layer** — `MainWindow` acts as both view and controller. This is intentional — the app is small enough that a separate controller would add complexity without benefit.
- **Project persistence** — `.cpce` files are plain JSON. `image_path` is stored as an absolute path.
- **AI labeling** — `AILabeler` wraps a YOLOv8 model. `AILabelWorker` runs inference in a `QThread` to keep the UI responsive. The worker emits `progress`, `error`, `result_ready`, and `finished` signals.

---

## 3. Code Conventions

### Must follow

- **Type hints required** on all functions and methods
- **Dataclasses** for all data models — never plain dicts
- **PyQt6 signals** for cross-widget communication — never call widget methods directly across component boundaries
- Use `.exec()` not `.exec_()` (PyQt6, not PyQt5)
- `snake_case` for files/folders, `PascalCase` for classes, `SCREAMING_SNAKE` for module-level UI constants

### Comments

Write comments only when the **why** is non-obvious. Do not describe what the code does — well-named identifiers already do that.

```python
# Good — explains a non-obvious constraint
# Manhattan distance is intentional here: Euclidean is slower and the
# visual difference at these sizes is imperceptible.
if abs(dx) + abs(dy) <= threshold:

# Bad — just restates the code
# Check if the distance is within the threshold
if abs(dx) + abs(dy) <= threshold:
```

### No speculative code

Do not add error handling, fallbacks, or abstractions for scenarios that cannot currently happen. Trust internal code and framework guarantees. Only validate at system boundaries (user input, external APIs).

---

## 4. Running the App

```bash
python -m src.main
```

In **GitHub Codespaces** (headless Linux):
- The app requires a display. Open port **6080** in the browser first (noVNC desktop).
- `QT_QPA_PLATFORM=xcb` is set automatically in `src/main.py`.

---

## 5. Linting and Type Checking

All three tools must pass before a PR can be merged.

```bash
# Style and imports
ruff check src/

# Static type checking
python -m mypy src/ --ignore-missing-imports

# Code quality (errors and warnings only)
pylint src/ --disable=all --enable=E,W
```

Run all three at once:
```bash
ruff check src/ && python -m mypy src/ --ignore-missing-imports && pylint src/ --disable=all --enable=E,W
```

There are no automated tests yet. If you are adding a testable unit (e.g. a pure function in `core/`), adding a test is encouraged but not required.

---

## 6. Submitting a Pull Request

1. **Fork** the repo and create a branch from `master`:
   ```bash
   git checkout -b feature/my-feature
   ```
2. Make your changes
3. Run the linters (see above) and fix any issues
4. Commit with a clear message explaining *why* the change was made, not just what
5. Push and open a PR against `master`
6. In the PR description, include:
   - What the change does
   - Why it was needed
   - How to test it manually (since there is no test suite yet)

For **larger features or breaking changes**, open an issue first to discuss the approach before writing code.

### Commit message style

```
Short summary in imperative mood (under 72 chars)

Optional longer explanation of why this change was needed.
Focus on motivation and context, not what lines changed.
```

---

## 7. Areas Open for Contribution

The following items are on the roadmap and would be welcome contributions:

| Feature | Difficulty | Notes |
|---|---|---|
| Dark mode | Low | PyQt6 palette + stylesheet |
| Thumbnail list in image panel | Medium | Already have `ThumbnailLoader` thread |
| Keyboard shortcut labeling | Low | e.g. `H` = HC — configurable per project |
| Undo/Redo | Medium | `QUndoStack` — affects point labeling and generation |
| Coverage pie chart | Medium | matplotlib embedded in PyQt6 widget |
| Image calibration | Medium | Click two points → input real-world distance |
| Area measurement | High | Trace polygon → compute area using calibration scale |
| Test suite | Any | `pytest` + `pytest-qt` — any coverage is useful |
| Batch point generation | Low | Progress dialog already exists as a pattern in `AIProgressDialog` |

If you're unsure where to start, **keyboard shortcut labeling** or **dark mode** are good first contributions.

---

## Questions

Open a [GitHub Issue](https://github.com/padreon/coralx/issues) or start a Discussion. We're happy to help.
