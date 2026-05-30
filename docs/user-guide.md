# coralX User Guide

This guide walks you through installing and using coralX to perform benthic point count analysis on underwater transect photos — no programming experience required.

---

## Table of Contents

1. [What is Point Count?](#1-what-is-point-count)
2. [Installation](#2-installation)
   - [Windows](#windows)
   - [macOS](#macos)
   - [Linux (Ubuntu / Debian)](#linux-ubuntu--debian)
   - [Linux (Fedora / RHEL)](#linux-fedora--rhel)
   - [Linux (Arch / Manjaro)](#linux-arch--manjaro)
3. [Creating Your First Project](#3-creating-your-first-project)
4. [Adding Images](#4-adding-images)
5. [Generating Points](#5-generating-points)
6. [Labeling Points](#6-labeling-points)
7. [AI Auto-Label](#7-ai-auto-label)
8. [Viewing Statistics](#8-viewing-statistics)
9. [Exporting Results](#9-exporting-results)
10. [Coral Codes Reference](#10-coral-codes-reference)
11. [Troubleshooting](#11-troubleshooting)

---

## 1. What is Point Count?

The **point count method** (also called Point Intercept Transect) is a standard technique for measuring how much of a reef is covered by corals, algae, rubble, and other organisms.

The process:
1. Take photos along a transect line underwater
2. Place random sample points on each photo
3. Identify what is directly under each point (hard coral, soft coral, algae, etc.)
4. Calculate the percentage coverage of each category

coralX automates steps 2–4 and replaces the legacy CPCe software.

---

## 2. Installation

coralX runs from source code using Python. This section explains everything from scratch — you do not need any prior programming experience.

> **What you need to know about the terminal**
> The "terminal" (on macOS/Linux) or "Command Prompt / PowerShell" (on Windows) is a text-based window where you type commands. You will use it to download and run coralX. Every command in this guide is meant to be typed exactly as shown, then press **Enter** to run it.

---

### Windows

#### Step 1 — Install Python

1. Open your web browser and go to **python.org** → click **Downloads** → click the button for the latest **Python 3.11.x** or **Python 3.12.x** release (either is fine).
2. Run the downloaded `.exe` file.
3. On the first screen of the installer, **check the box that says "Add Python to PATH"** before clicking anything else. This is the most important step — without it, Python will not be found from the terminal.

   ![Add Python to PATH checkbox is at the bottom of the first installer screen]

4. Click **Install Now** and wait for it to finish.
5. At the end, click **"Disable path length limit"** if it appears (recommended).
6. Click **Close**.

**Verify Python is installed:** Open Command Prompt (press `Win + R`, type `cmd`, press Enter) and run:
```
python --version
```
You should see something like `Python 3.11.9`. If you see an error, repeat from step 3 and make sure the PATH checkbox was checked.

#### Step 2 — Install Git

Git is the tool used to download coralX from the internet.

1. Go to **git-scm.com** → click **Download for Windows**.
2. Run the downloaded `.exe` file.
3. Click **Next** through all the installer screens — the default options are fine.
4. Click **Install**, then **Finish**.

**Verify Git is installed:**
```
git --version
```
You should see something like `git version 2.45.2.windows.1`.

> **Alternative — download without Git:** If you prefer not to install Git, go to the coralX GitHub page, click the green **Code** button, then **Download ZIP**. Extract the ZIP file to your Desktop.

#### Step 3 — Download coralX

Open **Command Prompt** and run these commands one at a time:
```
cd %USERPROFILE%\Desktop
git clone https://github.com/padreon/coralx
cd coralx
```

After this, a folder called `coralx` will be on your Desktop.

#### Step 4 — Create a Virtual Environment

A virtual environment keeps coralX's dependencies isolated from the rest of your computer. This prevents conflicts with other software.

```
python -m venv venv
```

Then activate it:
```
venv\Scripts\activate
```

Your prompt will change to show `(venv)` at the beginning — this means the virtual environment is active. **You must activate the virtual environment every time you open a new Command Prompt to run coralX.**

> **PowerShell users:** If you use PowerShell instead of Command Prompt, use `venv\Scripts\Activate.ps1`. If you get an error about execution policy, run `Set-ExecutionPolicy -ExecutionPolicy RemoteSigned -Scope CurrentUser` first.

#### Step 5 — Install Dependencies

With the virtual environment active, run:
```
pip install -r requirements.txt
```

This downloads and installs all the libraries coralX needs. It may take **5–15 minutes** depending on your internet connection. You will see a lot of text — this is normal.

#### Step 6 — Run coralX

```
python -m src.main
```

The coralX window will open. 

> **Every time you want to run coralX in the future:** Open Command Prompt, navigate to the coralx folder (`cd %USERPROFILE%\Desktop\coralx`), activate the virtual environment (`venv\Scripts\activate`), then run `python -m src.main`.

---

### macOS

#### Step 1 — Open the Terminal

Press **Cmd + Space**, type `Terminal`, and press Enter. The Terminal is the command-line interface on macOS.

#### Step 2 — Install Homebrew (recommended)

Homebrew is a package manager that makes it easy to install developer tools on macOS.

Open Terminal and paste this command, then press Enter:
```bash
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
```

Follow the on-screen instructions. You will be asked for your Mac's password (it won't show as you type — this is normal). This may take 5–10 minutes.

> **Apple Silicon Macs (M1/M2/M3/M4):** After Homebrew installs, it will tell you to run two commands to add it to your PATH. Run those commands before continuing.

#### Step 3 — Install Python

With Homebrew installed, run:
```bash
brew install python@3.11
```

> **Alternative (without Homebrew):** Go to python.org → Downloads → download the macOS installer for Python 3.11.x. Run the `.pkg` file and follow the installer.

**Verify Python is installed:**
```bash
python3 --version
```
You should see `Python 3.11.x`.

#### Step 4 — Install Git

Git is usually already on macOS. Check:
```bash
git --version
```

If macOS asks you to install **Command Line Developer Tools**, click **Install** and wait for it to finish. Then run `git --version` again.

With Homebrew you can also run `brew install git` to get the latest version.

#### Step 5 — Download coralX

```bash
cd ~/Desktop
git clone https://github.com/padreon/coralx
cd coralx
```

> **Alternative:** Go to the GitHub page → **Code** → **Download ZIP** → extract to Desktop.

#### Step 6 — Create a Virtual Environment

```bash
python3 -m venv venv
source venv/bin/activate
```

Your prompt will change to show `(venv)`. **Activate the virtual environment every time you open a new Terminal to run coralX.**

#### Step 7 — Install Dependencies

```bash
pip install -r requirements.txt
```

This may take 5–15 minutes.

#### Step 8 — Run coralX

```bash
python -m src.main
```

The coralX window will open.

> **Every time you want to run coralX in the future:** Open Terminal → `cd ~/Desktop/coralx` → `source venv/bin/activate` → `python -m src.main`.

> **macOS security warning:** On first launch macOS may show "coralX cannot be opened because it is from an unidentified developer" — this does not apply to running from source. If you see any other security prompt, go to **System Settings → Privacy & Security** and click **Open Anyway**.

---

### Linux (Ubuntu / Debian)

These steps work on Ubuntu 22.04, Ubuntu 24.04, Debian 12, and similar distributions.

#### Step 1 — Open a Terminal

Press **Ctrl + Alt + T**, or search for "Terminal" in your application menu.

#### Step 2 — Check Your Python Version

```bash
python3 --version
```

coralX requires Python 3.10 or newer. Ubuntu 22.04 comes with Python 3.10; Ubuntu 24.04 comes with Python 3.12. If your version is older than 3.10, follow the instructions below under "Older Ubuntu versions."

#### Step 3 — Install Python, pip, venv, and Git

```bash
sudo apt update
sudo apt install -y python3 python3-pip python3-venv git
```

You will be asked for your password. Type it and press Enter (it won't show — this is normal).

> **Older Ubuntu versions (20.04 or earlier):** Python 3.8 is the default, which is too old. Install a newer version:
> ```bash
> sudo apt install -y software-properties-common
> sudo add-apt-repository ppa:deadsnakes/ppa
> sudo apt update
> sudo apt install -y python3.11 python3.11-venv python3.11-distutils
> ```
> Then use `python3.11` instead of `python3` in the steps below.

#### Step 4 — Install Qt System Libraries

PyQt6 requires these system libraries to display the graphical interface:

```bash
sudo apt install -y \
  libxcb-cursor0 \
  libxcb-icccm4 \
  libxcb-image0 \
  libxcb-keysyms1 \
  libxcb-randr0 \
  libxcb-render-util0 \
  libxcb-shape0 \
  libxcb-xinerama0 \
  libxcb-xkb1 \
  libxkbcommon-x11-0 \
  libgl1
```

> **Why these are needed:** The Qt GUI toolkit (used by coralX) depends on the X11 display system. These packages are the "glue" between Qt and your desktop environment.

#### Step 5 — Download coralX

```bash
cd ~/Desktop
git clone https://github.com/padreon/coralx
cd coralx
```

> **Alternative:** Go to the GitHub page → **Code** → **Download ZIP** → extract to Desktop.

#### Step 6 — Create a Virtual Environment

```bash
python3 -m venv venv
source venv/bin/activate
```

Your prompt will change to show `(venv)`. **Activate the virtual environment every time you open a new Terminal to run coralX.**

#### Step 7 — Install Dependencies

```bash
pip install -r requirements.txt
```

This may take 5–15 minutes.

#### Step 8 — Run coralX

```bash
python -m src.main
```

The coralX window will open.

> **If you see "could not connect to display" or "cannot connect to X server":** Run `export DISPLAY=:0` before the Python command, or check that your desktop environment is running.

> **Every time you want to run coralX in the future:** Open Terminal → `cd ~/Desktop/coralx` → `source venv/bin/activate` → `python -m src.main`.

---

### Linux (Fedora / RHEL)

#### Step 1 — Install Python, pip, and Git

```bash
sudo dnf install -y python3 python3-pip git
```

#### Step 2 — Install Qt System Libraries

```bash
sudo dnf install -y \
  xcb-util-cursor \
  xcb-util-image \
  xcb-util-keysyms \
  xcb-util-renderutil \
  xcb-util-wm \
  libxkbcommon-x11 \
  mesa-libGL
```

#### Step 3 — Download, Set Up, and Run

```bash
cd ~/Desktop
git clone https://github.com/padreon/coralx
cd coralx
python3 -m venv venv
source venv/bin/activate
pip install -r requirements.txt
python -m src.main
```

---

### Linux (Arch / Manjaro)

#### Step 1 — Install Python, pip, and Git

```bash
sudo pacman -S python python-pip git
```

#### Step 2 — Install Qt System Libraries

```bash
sudo pacman -S xcb-util-cursor xcb-util-image xcb-util-keysyms xcb-util-renderutil xcb-util-wm libxkbcommon-x11 mesa
```

#### Step 3 — Download, Set Up, and Run

```bash
cd ~/Desktop
git clone https://github.com/padreon/coralx
cd coralx
python -m venv venv
source venv/bin/activate
pip install -r requirements.txt
python -m src.main
```

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
- **Train your own** — see the [Training Guide](training-guide.md)

### Running AI auto-label

1. Go to **Image → AI Auto-Label…** (or click the **AI Label** button in the toolbar)
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

## 11. Troubleshooting

### coralX won't start / "python not found"

- **Windows:** Make sure Python was installed with "Add Python to PATH" checked. Reinstall Python if needed and check the box.
- **macOS/Linux:** Use `python3` instead of `python`.
- Check which Python you have: `python --version` or `python3 --version`

### "No module named 'PyQt6'" or "No module named 'cv2'"

The virtual environment is not active or dependencies weren't installed. Run:
```bash
# macOS / Linux
source venv/bin/activate
pip install -r requirements.txt

# Windows
venv\Scripts\activate
pip install -r requirements.txt
```

### Blank window or app crashes immediately (Linux)

The Qt system libraries are missing. Install them:
```bash
# Ubuntu / Debian
sudo apt install -y libxcb-cursor0 libxcb-icccm4 libxcb-image0 \
  libxcb-keysyms1 libxcb-randr0 libxcb-render-util0 \
  libxcb-shape0 libxcb-xinerama0 libxcb-xkb1 \
  libxkbcommon-x11-0 libgl1
```

### "cannot connect to X server" (Linux)

The display variable is not set. Run:
```bash
export DISPLAY=:0
python -m src.main
```

### "ultralytics not installed" when using AI auto-label

Install ultralytics in your virtual environment:
```bash
pip install "ultralytics>=8.0.0"
```
Make sure the virtual environment is active first.

### Points disappeared after reopening the project

coralX stores **absolute file paths** to images. If you moved the image files, use **Edit → Relink Images** to update the paths.

### Export produces an empty file

- Make sure at least some points are labeled before exporting.
- Check that you have write permission to the export destination folder.

### pip install is very slow or stalls

- Check your internet connection.
- Try upgrading pip first: `pip install --upgrade pip`
- If a specific package (e.g. `torch`) is slow, this is normal — PyTorch is ~2 GB.
