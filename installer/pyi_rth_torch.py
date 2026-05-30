import os
import sys

# Fix WinError 1114 (DLL init failure) for PyInstaller + torch on Windows.
#
# Problem: c10.dll and friends live in torch/lib/ inside the bundle. Their
# static-link dependencies are resolved by the OS loader before Python even
# calls os.add_dll_directory(), so they never see the DLL search path we set.
#
# Fix: register both the MEIPASS root and torch/lib as DLL directories, then
# pre-load the core torch DLLs via ctypes so they are already in the process
# module list before torch's C extension tries to import them.
if sys.platform == "win32" and hasattr(sys, "_MEIPASS"):
    _meipass: str = sys._MEIPASS
    _torch_lib: str = os.path.join(_meipass, "torch", "lib")

    for _d in (_meipass, _torch_lib):
        if os.path.isdir(_d):
            try:
                os.add_dll_directory(_d)
            except OSError:
                pass
            os.environ["PATH"] = _d + os.pathsep + os.environ.get("PATH", "")

    if os.path.isdir(_torch_lib):
        import ctypes

        # Load in dependency order: lowest-level first.
        for _dll in (
            "c10.dll",
            "asmjit.dll",
            "libiomp5md.dll",
            "uv.dll",
            "fbgemm.dll",
            "torch_cpu.dll",
            "torch_cuda.dll",
        ):
            _p = os.path.join(_torch_lib, _dll)
            if os.path.isfile(_p):
                try:
                    ctypes.WinDLL(_p)
                except OSError:
                    pass
