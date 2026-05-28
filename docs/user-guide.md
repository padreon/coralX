# coralX User Guide

This guide walks you through installing and using coralX to perform benthic point count analysis on underwater transect photos — no programming experience required.

---

## Table of Contents

1. [What is Point Count?](#1-what-is-point-count)
2. [Installation](#2-installation)
3. [Creating Your First Project](#3-creating-your-first-project)
4. [Adding Images](#4-adding-images)
5. [Generating Points](#5-generating-points)
6. [Labeling Points](#6-labeling-points)
7. [AI Auto-Label](#7-ai-auto-label)
8. [Viewing Statistics](#8-viewing-statistics)
9. [Exporting Results](#9-exporting-results)
10. [Coral Codes Reference](#10-coral-codes-reference)

---

## 1. What is Point Count?

The **point count method** (also known as Point Intercept Transect) is a standard technique for measuring the coverage of different benthic organisms (corals, algae, rubble, etc.) on a reef.

The process:
1. Take photos along a transect line underwater
2. Place random sample points on each photo
3. Identify what is directly under each point (hard coral, soft coral, algae, etc.)
4. Calculate the percentage coverage of each category

coralX automates steps 2–4 and replaces the legacy CPCe software.

---

## 2. Installation

### Option A — Run Locally (Recommended)

**Requirements:** Python 3.10 or newer. Download from [python.org](https://python.org).

```bash
# 1. Download coralX
git clone https://github.com/padreon/coralx
cd coralx

# 2. (Recommended) Create a virtual environment
python -m venv venv
source venv/bin/activate        # macOS / Linux
venv\Scripts\activate           # Windows

# 3. Install dependencies
pip install -r requirements.txt

# 4. Run
python -m src.main
```

### Option B — GitHub Codespaces (No local install needed)

1. Click **Code → Open with Codespaces** on the GitHub repo page
2. Wait for the environment to set up (about 1–2 minutes)
3. In the **Ports** tab, open port **6080** in your browser
4. Enter the password: `coral`
5. In the terminal, run: `python -m src.main`

The app will appear in the browser window.

---

## 3. Creating Your First Project

1. Open coralX
2. Go to **File → New Project**
3. Enter a project name (e.g. `Reef Survey 2024`)
4. Choose where to save the `.cpce` project file
5. The project is created with one default **Station** — you can add more via the left panel

> **What is a Station?**
> A station represents one survey location (e.g. a specific site or depth). Each station contains multiple transect photos.

---

## 4. Adding Images

1. In the left panel, select a station
2. Click **Add Images** (or go to **File → Add Images**)
3. Select one or more photos (JPG, PNG, or TIFF)

Images appear in the list on the left. Click any image to view it on the canvas.

---

## 5. Generating Points

Before you can label, you need to place sample points on each image.

1. Select an image from the list
2. In the left panel, configure:
   - **Number of points** — typically 50–100 per photo
   - **Distribution method:**
     - *Random* — fully random placement
     - *Stratified* — random within a grid (more even coverage)
     - *Uniform* — evenly spaced grid
   - **Border exclusion** — optionally exclude a margin around the edge (useful if the image has a ruler or scale bar)
3. Click **Generate Points**

Points appear as coloured dots on the image. Unlabeled points are shown in red/yellow; labeled points turn green.

---

## 6. Labeling Points

### Manual labeling

- **Click** any point on the image → a menu appears with all available coral codes
- Select a code to assign it to that point
- **Arrow keys** cycle through points (the selected point is highlighted)
- **Enter** opens the label menu for the currently selected point

### Keyboard tips

| Key | Action |
|---|---|
| ← → ↑ ↓ | Move to next/previous point |
| Enter | Open label menu for selected point |
| Scroll wheel | Zoom in/out |
| Middle mouse drag | Pan the image |

### Customizing coral codes

Go to **Edit → Coral Codes** to add, remove, or edit the codes used in your project. You can also organize codes into groups for easier navigation.

---

## 7. AI Auto-Label

coralX includes an AI feature that can automatically label points using a YOLOv8 machine learning model. This can significantly speed up annotation, especially for large datasets.

### What you need

A trained YOLOv8 `.pt` model file. Two options:

- **Use the included model** — `data/data-training.pt` (7 coral morphology classes: branching, encrusting, foliose, massive, mushroom, submassive, tabulate)
- **Train your own** — see the [Training Your Own Model](#training-your-own-model) section below

### Running AI auto-label

1. Go to **Image → AI Auto-Label…** (or click the **🤖 AI Label** button in the toolbar)
2. Click **Browse…** and select your `.pt` model file
   - The model loads automatically and the class mapping table appears
3. Set the **scope**:
   - *This image only* — label points on the current image
   - *This station* — label all images in the current station
   - *Entire project* — label everything
4. Set the **confidence threshold** (default: 0.5)
   - Points where the model is less confident than this will be skipped (not labeled)
   - Lower = more points labeled but less accurate; Higher = fewer but more reliable
5. Configure the **class mapping** — map each model class to a coral code in your project:
   - Example: `branching` → `HC`, `foliose` → `HC`, etc.
   - Set to `(skip)` to ignore a class
6. Check **Label only unlabeled points** to skip points you've already labeled manually
7. Click **Run**

A progress dialog shows each point being processed. You can cancel at any time.

> **Note:** The AI is a helper, not a replacement for expert identification. Always review AI-labeled points, especially at lower confidence thresholds.

### Training your own model

See the full **[Training Guide](training-guide.md)** for step-by-step instructions including Roboflow dataset setup, Google Colab training (free GPU), and local training.

---

## 8. Viewing Statistics

The **right panel** shows live statistics as you label:

- **Progress bar** — percentage of points labeled for the current image
- **Quick stats** — breakdown of labeled categories for the current image

For full project statistics:

- Go to **View → Statistics** (or the Statistics tab)
- See per-image coverage percentages
- Shannon-Weaver diversity index (H')
- Simpson diversity index (1-D)

---

## 9. Exporting Results

Go to **File → Export**:

| Format | Contents |
|---|---|
| **CSV** | One row per image, coverage % per category |
| **Excel** | Three sheets: Summary, Per Image, Raw Points |

The Raw Points sheet contains every individual point with its coordinates and label — useful for further analysis in R or Python.

---

## 10. Coral Codes Reference

coralX ships with the **COREMAP** standard benthic codes by default:

| Code | Description |
|---|---|
| HC | Hard Coral |
| SC | Soft Coral |
| MA | Macro Algae |
| TA | Turf Algae |
| CCA | Crustose Coralline Algae |
| SP | Sponge |
| ZO | Zoanthid |
| RB | Rubble |
| SD | Sand |
| SI | Silt |
| RK | Rock |
| OT | Other |

You can add your own codes or modify these in **Edit → Coral Codes**.

---

## Troubleshooting

**The app doesn't start / blank screen**
- Make sure you're using Python 3.10 or newer: `python --version`
- On Linux, ensure the display is set: `export DISPLAY=:0` before running
- In Codespaces, open port 6080 in the browser before running the app

**"ultralytics not installed" when using AI auto-label**
- Install ultralytics in the same Python environment as coralX:
  ```bash
  pip install "ultralytics>=8.0.0"
  ```
- Make sure you activate your virtual environment first if you use one

**Points disappeared after reopening project**
- Check that the image files are still at the same path where they were when the project was saved
- coralX stores absolute file paths — if you moved the images, use **Edit → Relink Images**

**Export produces an empty file**
- Make sure at least some points are labeled before exporting
- Check that you have write permission to the export destination folder
