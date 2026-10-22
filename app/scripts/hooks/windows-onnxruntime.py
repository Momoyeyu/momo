"""Bind the packaged ORT before Windows can select its incompatible system DLL."""
import sys

if sys.platform == 'win32':
    import ctypes
    from pathlib import Path

    # sherpa's extension imports onnxruntime.dll by basename. Windows 11 also
    # ships that name (API17), while sherpa needs API28. The bundled ORT31
    # supports both inference clients; an absolute load pins its provenance.
    _momo_ort = ctypes.WinDLL(str(
        Path(sys._MEIPASS) / 'onnxruntime' / 'capi' / 'onnxruntime.dll'
    ))
