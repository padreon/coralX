# coralX

**coralX** is an open-source desktop app for coral reef benthic monitoring using the point count method — a modern, cross-platform replacement for [CPCe (Coral Point Count with Excel extensions)](https://hcas.nova.edu/tools-and-resources/cpce/).

Built with Python, PyQt6, and OpenCV.

→ **[User Guide](docs/user-guide.md)** · **[Training Guide](docs/training-guide.md)** · **[Contributing](docs/contributing.md)**

---

## Background

I started building coralX out of a combination of practical frustration and personal ambition.

The immediate trigger was CPCe itself. It is the field standard, but it was written in Visual Basic in 2006 and it shows — rendering a single image with 50 points can take several seconds, and on larger datasets the lag compounds into something genuinely painful to work through. For anyone doing serious survey work across thousands of transect photos, that time adds up fast.

The second reason was my own workflow. I regularly switch between Windows, macOS, and Linux depending on where I am and what machine I have in front of me. CPCe is Windows-only, so every time I was on a different OS I was blocked. I wanted a tool that simply worked everywhere without compromises.

The third reason — and the most personal one — is that I am preparing to apply for a Master's degree (S2) and need supporting documents from a supervisor. coralX is my first serious software project, and I built it to demonstrate what I am capable of. It is not a finished or perfect piece of work, but it is honest evidence of how I approach a real problem: independently, from scratch, and with the intent to make something genuinely useful for the research community.

If you are a researcher or academic who works in marine biology or related fields and would be open to becoming my supervisor, please do not hesitate to reach out.

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

- Python 3.10 or newer
- Linux, macOS, or Windows
- Git (to download the code)

### Quick Install

```bash
git clone https://github.com/padreon/coralx
cd coralx
python3 -m venv venv
source venv/bin/activate   # Windows: venv\Scripts\activate
pip install -r requirements.txt
python -m src.main
```

For a step-by-step guide covering **Windows, macOS, and Linux** (including installing Python and Git from scratch), see the **[User Guide](docs/user-guide.md)**.

### GitHub Codespaces

1. Open this repo in Codespaces
2. Wait for `postCreateCommand` to finish
3. Open port **6080** in your browser (password: `coral`) — this is the noVNC desktop
4. In the terminal, run `python -m src.main`

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
│   ├── user-guide.md              # Installation + step-by-step guide for end users
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

### Dataset Contribution

The AI auto-label feature is currently trained on a very limited dataset. If you have annotated underwater transect photos — or are willing to contribute coral imagery — I would love to hear from you. More data directly improves the accuracy of the auto-label model for everyone using coralX.

Feel free to reach out if you are interested in contributing data or collaborating on this.

---

## License

MIT
