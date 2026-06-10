---
sidebar_position: 6
---

# Acknowledgements

coralX stands on the shoulders of decades of coral reef science, open datasets, and open-source software. We gratefully acknowledge the following.

---

## Original Software — CPCe

coralX is a spiritual successor to **CPCe (Coral Point Count with Excel extensions)**, developed by:

> Kohler, K. E., & Gill, S. M. (2006).
> Coral Point Count with Excel extensions (CPCe): A Visual Basic program for the
> determination of coral and substrate coverage using random point count methodology.
> *Computers & Geosciences*, 32(9), 1259–1269.
> https://doi.org/10.1016/j.cageo.2005.11.009

CPCe established the point count workflow that coralX continues and extends.

---

## AI Auto-Labeling — Ultralytics YOLOv8

The AI auto-label feature is powered by **Ultralytics YOLOv8**, an open-source real-time object detection and classification framework.

> Jocher, G., Chaurasia, A., & Qiu, J. (2023).
> Ultralytics YOLO (Version 8.0.0) [Software].
> https://github.com/ultralytics/ultralytics

coralX uses YOLOv8 for per-point crop classification and detection. Users supply their own trained model (`.pt` file); coralX does not bundle or distribute any pre-trained weights.

---

## Training Data Sources

The following datasets were used to train the coral reef classification model bundled with coralX. We thank the institutions and researchers who collected and annotated this data.

### Lini Foundation — Lini Coral Forms 3.0

The primary training dataset is **Lini Coral Forms 3.0**, contributed by the **Lini Foundation** — an Indonesian marine conservation organization dedicated to coral reef research and community-based monitoring.

> Lini Foundation. (2024). *Lini Coral Forms 3.0* [Dataset].
> Roboflow Universe.
> https://universe.roboflow.com/lini-foundation/lini-coral-forms-3.0

Dataset: [universe.roboflow.com/lini-foundation/lini-coral-forms-3.0](https://universe.roboflow.com/lini-foundation/lini-coral-forms-3.0)

We are grateful to the Lini Foundation for making this labeled coral image dataset publicly available, enabling open AI-assisted reef monitoring.

---

## Scientific Python Ecosystem

The analysis engine in coralX relies on the broader scientific Python stack. See [Citations](/citations) for the full list of statistical methods and their references.

Key libraries:

| Library | Use in coralX |
|---|---|
| [NumPy](https://numpy.org) | Array operations, point coordinate math |
| [SciPy](https://scipy.org) | Statistical tests (ANOVA, Kruskal–Wallis, regression) |
| [pandas](https://pandas.pydata.org) | Tabular data, CSV/Excel export |
| [OpenCV](https://opencv.org) | Image loading and per-point crop extraction |
| [ultralytics](https://github.com/ultralytics/ultralytics) | YOLOv8 inference |
| [scikit-learn](https://scikit-learn.org) | PCoA, hierarchical clustering |
| [matplotlib](https://matplotlib.org) | Chart generation |
| [openpyxl](https://openpyxl.readthedocs.io) | Multi-sheet Excel output |
| [PyQt6](https://riverbankcomputing.com/software/pyqt/) | Desktop GUI framework |

---

## Contributing Researchers

We thank all researchers and students who have contributed labeled survey data, bug reports, and feedback during development. If you use coralX in published research, please consider opening a pull request to add your citation to this page.

---

*To add an acknowledgement or correct an attribution, please [open an issue](https://github.com/padreon/coralX/issues) or submit a pull request.*
