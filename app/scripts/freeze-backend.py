"""Build the self-contained backend using the locked backend environment.

Run `uv sync --frozen` and `uv pip install pyinstaller==6.20.0` in backend first.
No user data or source tree is copied to the installed resource directory.
"""
from pathlib import Path
import subprocess
import shutil

app = Path(__file__).resolve().parents[1]
backend = app / 'backend'
python = backend / '.venv' / ('Scripts/python.exe' if __import__('os').name == 'nt' else 'bin/python')
subprocess.run([str(python), '-c', 'import sherpa_onnx, onnxruntime, sqlite_vec, tokenizers'], check=True)
subprocess.run([str(python), '-m', 'PyInstaller', '--noconfirm', '--clean',
    '--distpath', str(app / 'resources'), '--workpath', str(app / 'target/freeze'),
    '--specpath', str(app / 'target'), '--name', 'momo-backend', '--onedir',
    '--add-data', str(backend / 'resources/soul.md') + __import__('os').pathsep + 'resources',
    '--runtime-hook', str(app / 'scripts/hooks/windows-onnxruntime.py'),
    '--collect-all', 'sherpa_onnx', '--collect-all', 'onnxruntime',
    '--collect-all', 'sqlite_vec', '--collect-all', 'tokenizers',
    '--collect-submodules', 'mcp.client', '--collect-submodules', 'mcp.shared', '--collect-submodules', 'mcp.server', '--collect-submodules', 'uvicorn',
    '--collect-submodules', 'app', '--copy-metadata', 'mcp',
    str(backend / 'desktop.py')], cwd=backend, check=True)
dest = app / 'resources/backend'
if dest.exists():
    shutil.rmtree(dest)
(app / 'resources/momo-backend').rename(dest)
if __import__('os').name == 'nt':
    subprocess.run([str(python), str(app / 'scripts/prepare-windows.py')], check=True)
    subprocess.run([str(python), str(app / 'scripts/audit-windows.py'), str(app / 'resources')], check=True)
print(f'Frozen backend: {dest}')
