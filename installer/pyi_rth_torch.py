import os
import sys

# Add torch/lib to the DLL search path on Windows before any torch import.
# Without this, c10.dll and fbgemm.dll fail to find their dependencies
# (WinError 1114) because PyInstaller's extraction dir is not on PATH.
if sys.platform == "win32" and hasattr(sys, "_MEIPASS"):
    torch_lib = os.path.join(sys._MEIPASS, "torch", "lib")
    if os.path.isdir(torch_lib):
        os.add_dll_directory(torch_lib)
