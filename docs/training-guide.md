# AI Auto-Label — Training Guide

This guide explains how to get or create a `.pt` model file for use with coralX's AI Auto-Label feature.

---

## Table of Contents

1. [Using the Included Model](#1-using-the-included-model)
2. [How the AI Works](#2-how-the-ai-works)
3. [Getting a Dataset from Roboflow](#3-getting-a-dataset-from-roboflow)
4. [Training with Google Colab (Recommended)](#4-training-with-google-colab-recommended)
5. [Training Locally](#5-training-locally)
6. [Using Your Model in coralX](#6-using-your-model-in-coralx)
7. [Class Mapping Reference](#7-class-mapping-reference)

---

## 1. Using the Included Model

coralX ships with a pre-trained model at `data/data-training.pt`.

| Property | Value |
|---|---|
| Architecture | YOLOv8 nano (classification) |
| Classes | 7 coral morphologies |
| Class names | branching, encrusting, foliose, massive, mushroom, submassive, tabulate |
| Training data | LINI Coral Forms dataset (Roboflow) |

To use it:
1. Go to **Image → AI Auto-Label…**
2. Click **Browse…** → select `data/data-training.pt`
3. The model loads automatically and shows the class mapping table
4. Map each class to a coral code (e.g. all 7 → `HC`)
5. Click **Run**

If you work with a standard COREMAP/SIMSE coral code system where all growth forms fall under Hard Coral (`HC`), the included model is sufficient.

---

## 2. How the AI Works

For each sample point on an image, coralX:

1. **Crops** a small patch (default 64×64 px) centred on the point
2. **Feeds** that crop into the YOLOv8 model
3. **Gets back** a predicted class + confidence score
4. **Skips** the point if confidence is below your threshold
5. **Applies** the class → coral code mapping you configured

The model only sees a small crop around each point — it does not analyse the full image at once.

```
Full transect image (e.g. 4000×3000 px)
         │
         ▼
Point (x=1240, y=890)
         │
         ▼
Crop 64×64 px centred on point
         │
         ▼
YOLOv8 classify → "branching" (conf 0.87)
         │
         ▼
Mapping: "branching" → HC
         │
         ▼
Point labeled: HC
```

---

## 3. Getting a Dataset from Roboflow

[Roboflow Universe](https://universe.roboflow.com) hosts thousands of annotated image datasets including coral imagery. You can use any object-detection dataset — coralX's training script automatically crops the bounding boxes into classification training images.

### Step 1 — Find a dataset

Search Roboflow Universe for coral datasets. Look for:
- Object detection format (not classification)
- Good class coverage for your study area
- At least 100–200 annotated images per class

### Step 2 — Get your API key

1. Create a free account at [app.roboflow.com](https://app.roboflow.com)
2. Go to **Settings → API Key**
3. Copy your key

### Step 3 — Configure the training script

Copy the example config:
```bash
cp tools/training_config.yaml.example tools/training_config.yaml
```

Edit `tools/training_config.yaml`:
```yaml
roboflow:
  api_key: "your-key-here"
  workspace: "workspace-slug"   # from the dataset URL
  project: "project-slug"       # from the dataset URL
  version: 1

training:
  epochs: 100
  imgsz: 64        # must match coralX crop_size setting
  batch: 32
  base_model: yolov8n-cls.pt
  work_dir: training_data
```

> **Where to find workspace and project slugs:**
> From the dataset URL `https://universe.roboflow.com/lini-foundation/lini-coral-forms-3.0`:
> - workspace = `lini-foundation`
> - project = `lini-coral-forms-3.0`
> - version = check the **Versions** tab on the dataset page

> **Security:** `training_config.yaml` is in `.gitignore` — your API key will never be committed to git.

---

## 4. Training with Google Colab (Recommended)

Google Colab provides free GPU access, making training 10–20× faster than a typical laptop CPU.

### Step 1 — Open the notebook

Upload `tools/train_colab.ipynb` to [colab.research.google.com](https://colab.research.google.com):
- **File → Upload notebook** → select `train_colab.ipynb`

### Step 2 — Enable GPU

- **Runtime → Change runtime type → T4 GPU → Save**

### Step 3 — Fill in the config form

The second cell is a form — fill in your details directly in Colab:

| Field | Example |
|---|---|
| API_KEY | `HCqAX3nk...` |
| WORKSPACE | `lini-foundation` |
| PROJECT | `lini-coral-forms-3.0` |
| VERSION | `3` |
| EPOCHS | `100` |
| IMGSZ | `64` |

### Step 4 — Configure augmentation (optional)

The form also has augmentation settings pre-tuned for underwater coral imagery:

| Setting | Default | Effect |
|---|---|---|
| AUG_FLIPLR | 0.5 | Horizontal flip (coral has no left/right) |
| AUG_FLIPUD | 0.3 | Vertical flip |
| AUG_DEGREES | 15.0 | Rotation ± 15° (camera tilt) |
| AUG_HSV_H | 0.015 | Hue shift (water colour variation) |
| AUG_HSV_S | 0.4 | Saturation shift (depth variation) |
| AUG_HSV_V | 0.3 | Brightness shift (light conditions) |
| AUG_BLUR | 0.1 | Blur (turbid water simulation) |

### Step 5 — Run all cells

**Runtime → Run all** — the full pipeline takes about 10–30 minutes on a T4 GPU depending on dataset size.

### Step 6 — Download the model

The last cell automatically:
- Saves `best.pt` to your **Google Drive** (under `coralX_models/`)
- Downloads it directly to your computer

---

## 5. Training Locally

If you have a GPU or don't mind slower training:

```bash
# Install dependencies
pip install roboflow ultralytics pyyaml opencv-python

# Run the full pipeline (download + crop + train)
python tools/prepare_and_train.py

# Resume from a specific step
python tools/prepare_and_train.py --skip-download   # already downloaded
python tools/prepare_and_train.py --skip-download --skip-crop  # already cropped
```

What each step does:

| Step | What happens |
|---|---|
| Download | Downloads the dataset from Roboflow in YOLOv8 detection format |
| Crop | Extracts each annotated bounding box as a separate image, organised into per-class folders |
| Train | Trains a YOLOv8 classification model on the cropped images |

Output: `runs/classify/coral_classifier/weights/best.pt`

### Adjusting training parameters

Pass them as arguments to override the config file:

```bash
python tools/prepare_and_train.py \
    --epochs 150 \
    --imgsz 128 \      # larger crop = more context, slower inference
    --batch 16 \       # reduce if you run out of memory
    --base-model yolov8s-cls.pt  # larger model, better accuracy
```

---

## 6. Using Your Model in coralX

Once you have a `best.pt` file:

1. Go to **Image → AI Auto-Label…**
2. Click **Browse…** → select your `.pt` file
3. The model loads and detects its classes automatically
4. Set the **class mapping** (see below)
5. Set **scope** and **confidence threshold**
6. Click **Run**

### Recommended confidence thresholds

| Threshold | Behaviour |
|---|---|
| 0.7+ | Only very confident predictions → few labeled, high accuracy |
| 0.5 | Balanced (default) |
| 0.3 | More points labeled, more errors — review results carefully |

---

## 7. Class Mapping Reference

The AI model outputs class names from its training data. You map these to coralX coral codes in the dialog.

### Example: morphology model → COREMAP codes

| Model class | coralX code | Notes |
|---|---|---|
| branching | HC | Hard coral, branching growth form |
| encrusting | HC | Hard coral, encrusting growth form |
| foliose | HC | Hard coral, foliose/plate growth form |
| massive | HC | Hard coral, massive/boulder growth form |
| mushroom | HC | Fungiidae (free-living) |
| submassive | HC | Hard coral, submassive growth form |
| tabulate | HC | Hard coral, tabulate growth form |
| soft_coral | SC | Soft coral |
| algae | MA | Macro algae |
| rubble | RB | Rubble |
| sand | SD | Sand |
| rock | RK | Rock |
| (skip) | — | Class ignored, point not labeled |

> **Tip:** Set classes to `(skip)` rather than forcing a mapping you're unsure about. It's better to leave some points unlabeled for manual review than to mislabel them.
