# -*- mode: python ; coding: utf-8 -*-
"""
PyInstaller spec for coralX (includes ultralytics + PyTorch for AI auto-label).

Build:
    pip install pyinstaller "ultralytics>=8.0.0"
    pyinstaller coralx.spec

Output:
    dist/coralX/         (Linux / macOS)
    dist/coralX/         (Windows — contains coralX.exe)

Expected sizes (uncompressed):
    Windows:  ~1.5–2 GB  (Inno Setup LZMA2 compresses installer to ~500–700 MB)
    macOS:    ~1.2–1.6 GB
    Linux:    ~1.2–1.6 GB
"""

import sys
from pathlib import Path

ROOT = Path(SPECPATH)

block_cipher = None

# ---------------------------------------------------------------------------
# Collect ultralytics and its data files / hidden imports
# ---------------------------------------------------------------------------
try:
    from PyInstaller.utils.hooks import collect_all
    _ult_datas, _ult_binaries, _ult_hiddenimports = collect_all("ultralytics")
except Exception:
    _ult_datas, _ult_binaries, _ult_hiddenimports = [], [], []

# ---------------------------------------------------------------------------
# Qt modules coralX does NOT use — excluding these cuts ~150–200 MB
# ---------------------------------------------------------------------------
_EXCLUDED_QT = [
    "Qt6WebEngine", "Qt6WebEngineCore", "Qt6WebEngineWidgets",
    "Qt6WebEngineQuick", "Qt6WebView",
    "Qt6Multimedia", "Qt6MultimediaWidgets", "Qt6MultimediaQuick",
    "Qt6Quick", "Qt6QuickWidgets", "Qt6QuickControls2",
    "Qt6Qml", "Qt6QmlModels", "Qt6QmlWorkerScript",
    "Qt6Network",
    "Qt6Sql",
    "Qt6Test",
    "Qt6Bluetooth",
    "Qt6SerialPort",
    "Qt6Location",
    "Qt6Positioning",
    "Qt6Charts",
    "Qt6DataVisualization",
    "Qt63DCore", "Qt63DRender", "Qt63DInput", "Qt63DLogic", "Qt63DAnimation",
    "Qt6VirtualKeyboard",
    "Qt6Pdf", "Qt6PdfWidgets",
    # macOS / Linux equivalents (no "6" suffix variant)
    "QtWebEngine", "QtWebEngineCore", "QtWebEngineWidgets",
    "QtMultimedia", "QtMultimediaWidgets",
    "QtQuick", "QtQml", "QtNetwork", "QtSql", "QtTest",
    "QtBluetooth", "QtCharts",
]

a = Analysis(
    [str(ROOT / "src" / "main.py")],
    pathex=[str(ROOT)],
    binaries=[*_ult_binaries],
    datas=[
        (str(ROOT / "data" / "coral_codes_default.json"), "data"),
        (str(ROOT / "data" / "data-training.pt"),         "data"),
        *_ult_datas,
    ],
    hiddenimports=[
        "PyQt6.QtPrintSupport",
        "PyQt6.sip",
        *_ult_hiddenimports,
    ],
    hookspath=[],
    hooksconfig={},
    runtime_hooks=[],
    excludes=[
        # Qt modules not used by coralX
        "PyQt6.QtWebEngine", "PyQt6.QtWebEngineWidgets", "PyQt6.QtWebEngineCore",
        "PyQt6.QtMultimedia", "PyQt6.QtMultimediaWidgets",
        "PyQt6.QtQuick", "PyQt6.QtQuickWidgets",
        "PyQt6.QtQml", "PyQt6.QtQmlModels",
        "PyQt6.QtNetwork",
        "PyQt6.QtSql",
        "PyQt6.QtTest",
        "PyQt6.QtBluetooth",
        "PyQt6.QtCharts",
        "PyQt6.QtDataVisualization",
        "PyQt6.Qt3DCore", "PyQt6.Qt3DRender",
        # Dev tools
        "ruff", "mypy", "pylint",
        # Unused scientific stack
        "matplotlib", "tkinter", "wx",
        # Notebook / IPython
        "IPython", "ipykernel", "jupyter", "nbformat",
        # Unused scipy submodules (keep scipy.stats, scipy.spatial, scipy.optimize)
        "scipy.io", "scipy.signal",
        "scipy.integrate", "scipy.interpolate", "scipy.linalg",
        "scipy.fft", "scipy.ndimage",
        # NOTE: torch / ultralytics are intentionally NOT excluded here
    ],
    win_no_prefer_redirects=False,
    win_private_assemblies=False,
    cipher=block_cipher,
    noarchive=False,
)

# Strip unused Qt shared libraries only (keep all torch/ultralytics binaries)
a.binaries = [
    (name, path, typecode)
    for name, path, typecode in a.binaries
    if not any(excl.lower() in name.lower() for excl in _EXCLUDED_QT)
]

pyz = PYZ(a.pure, a.zipped_data, cipher=block_cipher)

exe = EXE(
    pyz,
    a.scripts,
    [],
    exclude_binaries=True,
    name="coralX",
    debug=False,
    bootloader_ignore_signals=False,
    strip=sys.platform != "win32",
    upx=False,  # UPX disabled — corrupts PyTorch DLLs (WinError 1114)
    console=False,
    disable_windowed_traceback=False,
    target_arch=None,
    codesign_identity=None,
    entitlements_file=None,
    icon=str(ROOT / "assets" / ("icon.icns" if sys.platform == "darwin" else "icon.ico")),
)

coll = COLLECT(
    exe,
    a.binaries,
    a.zipfiles,
    a.datas,
    strip=sys.platform != "win32",
    upx=False,  # UPX disabled — corrupts PyTorch DLLs (WinError 1114)
    name="coralX",
)

if sys.platform == "darwin":
    app = BUNDLE(
        coll,
        name="coralX.app",
        icon=str(ROOT / "assets" / "icon.icns"),
        bundle_identifier="com.coralx.app",
        info_plist={
            "NSHighResolutionCapable": True,
            "CFBundleShortVersionString": "1.0.0",
        },
    )
