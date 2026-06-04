---
slug: /
sidebar_position: 1
---

# coralX

**coralX** is an open-source desktop application for coral reef benthic monitoring using the point count method — a modern, cross-platform replacement for [CPCe (Coral Point Count with Excel extensions)](https://nsuworks.nova.edu/software/2/).

Built with **Python**, **PyQt6**, and **OpenCV**. Optional AI auto-labeling via **YOLOv8**.

## Why coralX?

CPCe is the de facto standard tool for benthic point count analysis, but it was built in Visual Basic (2006), runs only on Windows, and requires Microsoft Excel. coralX modernizes the workflow:

| | CPCe | coralX |
|---|---|---|
| **Platform** | Windows only | Windows, macOS, Linux |
| **Point distribution** | Random only | Random, Stratified, Uniform |
| **Export** | Excel (requires Office) | CSV + Excel (no Office needed) |
| **Diversity indices** | Manual calculation | Auto-calculated (Shannon H', Simpson 1-D) |
| **Image zoom** | Basic | Smooth scroll-to-zoom + pan |
| **Project format** | Proprietary | Open JSON (`.cpce`) |
| **AI auto-label** | No | Yes — YOLOv8 per-point prediction |

## Features

- Load underwater transect photos and overlay distributed sample points (random / stratified / uniform)
- Click a point → assign a benthic code from a customizable code list
- Keyboard navigation through points (arrow keys + Enter to label)
- Border exclusion — define a region to confine point generation
- Per-image and project-level coverage statistics
- Shannon-Weaver (H') and Simpson (1-D) diversity indices
- Export to CSV or multi-sheet Excel (Summary / Per Image / Raw Points)
- Import existing CPCe projects and labeled data
- AI auto-label via YOLOv8 per-point classification
- Save/load as portable `.cpce` JSON files

## Quick Start

Head to the [User Guide](/user-guide) to get started with installation and your first project.

## Source Code

[github.com/padreon/coralX](https://github.com/padreon/coralX)
