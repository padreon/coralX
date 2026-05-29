# -*- mode: python ; coding: utf-8 -*-
"""
PyInstaller spec for coralX.

Build:
    pip install pyinstaller
    pyinstaller coralx.spec

Output:
    dist/coralX/         (Linux / macOS)
    dist/coralX/         (Windows — contains coralX.exe)

Expected sizes after optimization:
    Windows:  ~80–100 MB  (down from ~338 MB)
    macOS:    ~70–90 MB
    Linux:    ~70–90 MB
"""

import sys
from pathlib import Path

ROOT = Path(SPECPATH)

block_cipher = None

# ---------------------------------------------------------------------------
# Qt modules coralX does NOT use — excluding these cuts ~150–200 MB
# ---------------------------------------------------------------------------
_EXCLUDED_TORCH = [
    # PyTorch native DLLs — excluded because ultralytics/torch are optional
    # and UPX compression corrupts them, causing WinError 1114 at runtime.
    "c10", "torch", "libtorch", "fbgemm", "asmjit",
    "torch_cpu", "torch_cuda", "torch_python",
    "caffe2", "shm", "libgomp",
]

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
    binaries=[],
    datas=[
        (str(ROOT / "data" / "coral_codes_default.json"), "data"),
        (str(ROOT / "data" / "data-training.pt"),         "data"),
    ],
    hiddenimports=[
        "PyQt6.QtPrintSupport",
        "PyQt6.sip",
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
        # AI / ML stack — optional, user installs separately
        "ultralytics", "torch", "torchvision", "torchaudio",
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
    ],
    win_no_prefer_redirects=False,
    win_private_assemblies=False,
    cipher=block_cipher,
    noarchive=False,
)

# ---------------------------------------------------------------------------
# Strip unused Qt shared libraries from the collected binaries
# This is the most effective size reduction step (~150 MB on Windows)
# ---------------------------------------------------------------------------
def _should_exclude(name: str) -> bool:
    name_lower = name.lower()
    return (
        any(excl.lower() in name_lower for excl in _EXCLUDED_QT)
        or any(excl.lower() in name_lower for excl in _EXCLUDED_TORCH)
    )

a.binaries = [
    (name, path, typecode)
    for name, path, typecode in a.binaries
    if not _should_exclude(name)
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
    strip=sys.platform != "win32",  # strip debug symbols on Linux/macOS
    upx=True,
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
    upx=True,
    upx_exclude=["vcruntime140.dll", "python3*.dll", "c10.dll", "torch*.dll", "fbgemm.dll"],
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
