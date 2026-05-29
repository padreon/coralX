import os
import sys

# Fallback: add torch/lib to the DLL search path and PATH so Windows can
# find c10.dll / fbgemm.dll. The spec already copies these DLLs to the
# bundle root (where Windows searches first), so this hook is a safety net
# for any DLL that loads torch internals via a non-standard path.
if sys.platform == "win32" and hasattr(sys, "_MEIPASS"):
    torch_lib = os.path.join(sys._MEIPASS, "torch", "lib")
    if os.path.isdir(torch_lib):
        os.environ["PATH"] = torch_lib + os.pathsep + os.environ.get("PATH", "")
        try:
            os.add_dll_directory(torch_lib)
        except Exception:
            pass
