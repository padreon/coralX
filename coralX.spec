# -*- mode: python ; coding: utf-8 -*-
# PyInstaller spec for coralX — optimized for minimal binary size.
# Build: pyinstaller coralX.spec

import sys
from PyInstaller.utils.hooks import collect_data_files, collect_submodules

block_cipher = None

# ── Modules to exclude ──────────────────────────────────────────────────────
# Unused PyQt6 modules (saves ~100–150 MB from PyQt6's 219 MB footprint)
_qt_exclude = [
    'PyQt6.QtBluetooth',
    'PyQt6.QtDBus',
    'PyQt6.QtDesigner',
    'PyQt6.QtHelp',
    'PyQt6.QtLocation',
    'PyQt6.QtMultimedia',
    'PyQt6.QtMultimediaWidgets',
    'PyQt6.QtNetwork',
    'PyQt6.QtNetworkAuth',
    'PyQt6.QtNfc',
    'PyQt6.QtOpenGL',
    'PyQt6.QtOpenGLWidgets',
    'PyQt6.QtPositioning',
    'PyQt6.QtPrintSupport',
    'PyQt6.QtQml',
    'PyQt6.QtQuick',
    'PyQt6.QtQuick3D',
    'PyQt6.QtQuickControls2',
    'PyQt6.QtQuickWidgets',
    'PyQt6.QtRemoteObjects',
    'PyQt6.QtSensors',
    'PyQt6.QtSerialPort',
    'PyQt6.QtSql',
    'PyQt6.QtSvg',
    'PyQt6.QtSvgWidgets',
    'PyQt6.QtTest',
    'PyQt6.QtTextToSpeech',
    'PyQt6.QtWebChannel',
    'PyQt6.QtWebEngineCore',
    'PyQt6.QtWebEngineQuick',
    'PyQt6.QtWebEngineWidgets',
    'PyQt6.QtWebSockets',
    'PyQt6.QtXml',
]

# Unused scipy submodules (only scipy.optimize + scipy.stats are used)
_scipy_exclude = [
    'scipy.signal',
    'scipy.ndimage',
    'scipy.spatial',
    'scipy.interpolate',
    'scipy.linalg',
    'scipy.fft',
    'scipy.io',
    'scipy.cluster',
    'scipy.integrate',
    'scipy.sparse',
    'scipy.odr',
    'scipy.datasets',
    'scipy._lib.doccer',
]

# Unused stdlib / dev tools
_stdlib_exclude = [
    'tkinter',
    'unittest',
    'doctest',
    'pdb',
    'pydoc',
    'distutils',
    'setuptools',
    'pkg_resources',
    'email',
    'html',
    'http',
    'xml',
    'xmlrpc',
    'ftplib',
    'imaplib',
    'smtplib',
    'poplib',
    'telnetlib',
    'nntplib',
    'mailbox',
    'difflib',
    'curses',
    'lib2to3',
    'test',
    'IPython',
    'jupyter',
    'notebook',
    'matplotlib',
    'cv2',         # removed; Pillow is used instead
]

excludes = _qt_exclude + _scipy_exclude + _stdlib_exclude

# ── Hidden imports (modules PyInstaller misses) ──────────────────────────────
hiddenimports = [
    'PyQt6.QtWidgets',
    'PyQt6.QtCore',
    'PyQt6.QtGui',
    'scipy.optimize._brentq',
    'scipy.stats._continuous_distns',
    'openpyxl',
    'openpyxl.styles',
    'PIL.Image',
    'PIL.JpegImagePlugin',
    'PIL.PngImagePlugin',
    'PIL.TiffImagePlugin',
    'PIL.BmpImagePlugin',
]

# ── Data files ───────────────────────────────────────────────────────────────
datas = [
    ('data/coral_codes_default.json', 'data'),
]

a = Analysis(
    ['src/main.py'],
    pathex=['.'],
    binaries=[],
    datas=datas,
    hiddenimports=hiddenimports,
    hookspath=[],
    hooksconfig={},
    runtime_hooks=[],
    excludes=excludes,
    win_no_prefer_redirects=False,
    win_private_assemblies=False,
    cipher=block_cipher,
    noarchive=False,
)

pyz = PYZ(a.pure, a.zipped_data, cipher=block_cipher)

exe = EXE(
    pyz,
    a.scripts,
    a.binaries,
    a.zipfiles,
    a.datas,
    [],
    name='coralX',
    debug=False,
    bootloader_ignore_signals=False,
    strip=True,        # strip debug symbols
    upx=True,          # UPX compression
    upx_exclude=[
        # These crash or gain nothing from UPX compression
        'vcruntime140.dll',
        'python3*.dll',
        'libpython*.so*',
    ],
    runtime_tmpdir=None,
    console=False,     # GUI app, no console window
    disable_windowed_traceback=False,
    argv_emulation=False,
    target_arch=None,
    codesign_identity=None,
    entitlements_file=None,
    # icon='data/icon.ico',  # uncomment and add icon file if available
)
