# coralX

**coralX** is an open-source desktop app for coral reef benthic monitoring using the point count method — a modern, cross-platform replacement for [CPCe (Coral Point Count with Excel extensions)](https://hcas.nova.edu/tools-and-resources/cpce/).

Built with Python, PyQt6, and OpenCV.

→ **[User Guide](docs/user-guide.md)** · **[Training Guide](docs/training-guide.md)** · **[Contributing](docs/contributing.md)**

---

## Why coralX?

CPCe is the de facto standard tool for benthic point count analysis, but it was built in Visual Basic (2006), runs only on Windows, and requires Microsoft Excel. coralX modernizes the workflow:

| | CPCe | coralX |
|---|---|---|
| Platform | Windows only | Windows, macOS, Linux |
| Point distribution | Random only | Random, Stratified, Uniform |
| Export | Excel (requires Office) | CSV + Excel (no Office needed) |
| Diversity indices | Manual calculation | Auto-calculated (Shannon H', Simpson 1-D) |
| Image zoom | Basic | Smooth scroll-to-zoom + pan |
| Project format | Proprietary | Open JSON (`.cpce`) |
| AI auto-label | No | Yes — YOLOv8 per-point prediction |

---

## Features

- Load underwater transect photos and overlay randomly (or uniformly/stratifiedly) distributed sample points
- Click a point → assign a benthic code from a customizable code list
- Keyboard navigation through points (arrow keys + Enter to label)
- Border exclusion — define a region to confine point generation
- Per-image and project-level coverage statistics
- Shannon-Weaver (H') and Simpson (1-D) diversity indices
- Export to CSV or multi-sheet Excel (Summary / Per Image / Raw Points)
- Import existing CPCe projects and labeled data
- Save/load projects as portable `.cpce` JSON files
- **AI auto-label** — run a YOLOv8 classification model to automatically label points

---

## Getting Started

### Requirements

- Python 3.10+
- Linux, macOS, or Windows

### Install

```bash
git clone https://github.com/padreon/coralx
cd coralx
pip install -r requirements.txt
```

### Run

```bash
python -m src.main
```

### GitHub Codespaces

1. Open this repo in Codespaces
2. Wait for `postCreateCommand` to finish
3. Open port **6080** in your browser (password: `coral`) — this is the noVNC desktop
4. In the terminal, run `python -m src.main`

For the full walkthrough, see the **[User Guide](docs/user-guide.md)**.

---

## Project Structure

```
coralX/
├── src/
│   ├── main.py                    # Entry point
│   ├── ui/
│   │   ├── main_window.py         # App shell, menus, panels, file I/O
│   │   ├── image_canvas.py        # Image viewer with point overlay
│   │   ├── ai_label_dialog.py     # AI auto-label dialog + progress
│   │   ├── import_dialogs.py      # CPCe import UI
│   │   └── calibration_dialog.py  # Image calibration
│   ├── core/
│   │   ├── point_generator.py     # Random / stratified / uniform points
│   │   ├── statistics.py          # Coverage %, Shannon H', Simpson 1-D
│   │   ├── exporter.py            # CSV and Excel export
│   │   ├── importer.py            # CPCe / CSV import
│   │   ├── ai_labeler.py          # YOLOv8 wrapper + background worker
│   │   └── analysis.py            # Data analysis helpers
│   └── models/
│       └── project.py             # Data models (Project, Station, ImageAnnotation, Point)
├── data/
│   ├── coral_codes_default.json   # Default benthic codes (COREMAP standard)
│   └── data-training.pt           # Pre-trained coral morphology classifier (7 classes)
├── tools/
│   ├── prepare_and_train.py       # Download Roboflow dataset + train YOLOv8 classifier
│   ├── train_colab.ipynb          # Google Colab notebook for training with augmentation
│   └── training_config.yaml.example
├── docs/
│   ├── user-guide.md              # Step-by-step guide for end users
│   └── contributing.md            # Developer setup and contribution guide
└── requirements.txt
```

---

## Tech Stack

| Library | Purpose |
|---|---|
| PyQt6 | Desktop UI |
| OpenCV | Image loading and processing |
| NumPy | Point generation and statistics |
| Pandas | Data aggregation and export |
| openpyxl | Excel file generation |
| ultralytics *(optional)* | YOLOv8 AI auto-label |

---

## Roadmap

- [ ] Dark mode
- [ ] Thumbnail list in image panel
- [ ] Keyboard shortcut labeling (e.g. `H` = HC)
- [ ] Undo/Redo via `QUndoStack`
- [ ] Batch point generation with progress dialog
- [ ] Coverage pie chart (matplotlib embedded)
- [ ] Image calibration (click two points → real-world distance)
- [ ] Area measurement (trace outline → compute area)
- [ ] Image filter/sort (show only incomplete)
- [x] AI auto-label via YOLOv8 per-point prediction

---

## Contributing

See **[docs/contributing.md](docs/contributing.md)** for the full guide.

---

## License

MIT
