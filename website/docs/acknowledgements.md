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

The following publicly available datasets are commonly used to train coral reef classification models compatible with coralX. We thank the institutions and researchers who maintain them.

### CoralNet

**CoralNet** is the primary community platform for benthic image annotation and provides thousands of labeled coral reef images from reef monitoring programs worldwide.

> Beijbom, O., Edmunds, P. J., Rosman, J. H., Tor, D. G., & Kriegman, D. J. (2015).
> Towards automated annotation of benthic survey images: Variability of human experts
> and operational modes of automation.
> *PLOS ONE*, 10(7), e0130312.
> https://doi.org/10.1371/journal.pone.0130312

Website: [https://coralnet.ucsd.edu](https://coralnet.ucsd.edu)

### NOAA National Coral Reef Monitoring Program (NCRMP)

The **NOAA NCRMP** provides standardized benthic survey data and imagery from U.S. coral reef jurisdictions, including the Florida Reef Tract, Hawaii, and the U.S. Pacific territories.

> National Oceanic and Atmospheric Administration (NOAA). National Coral Reef Monitoring Program (NCRMP).
> https://www.coris.noaa.gov/monitoring/

### ReefCloud

**ReefCloud** is an open platform by the Australian Institute of Marine Science (AIMS) for storing, analysing, and sharing reef image data, including AI-assisted annotation.

> Australian Institute of Marine Science. (2022). ReefCloud — AI-assisted coral reef monitoring.
> https://reefcloud.ai

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
