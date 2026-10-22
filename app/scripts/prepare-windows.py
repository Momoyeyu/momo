"""Stage the Visual C++ redistributables from the installed build toolchain."""
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys

if sys.platform != 'win32':
    raise SystemExit(0)
app = Path(__file__).resolve().parents[1]
backend = app / 'resources/backend'
internal = backend / '_internal'
vswhere = Path(os.environ['ProgramFiles(x86)']) / 'Microsoft Visual Studio/Installer/vswhere.exe'
installation = Path(subprocess.check_output([str(vswhere), '-latest', '-products', '*',
    '-requires', 'Microsoft.VisualStudio.Component.VC.Tools.x86.x64',
    '-property', 'installationPath'], text=True).strip())
versions = [path for path in (installation / 'VC/Redist/MSVC').iterdir()
            if re.fullmatch(r'\d+(?:\.\d+)+', path.name)]
version = max(versions, key=lambda path: tuple(map(int, path.name.split('.'))))
redist = version / 'x64/Microsoft.VC143.CRT'
if not redist.is_dir():
    raise RuntimeError('The x64 Visual C++ redistributable directory is missing')
licenses = backend / 'licenses'
licenses.mkdir(exist_ok=True)
shutil.copy2(Path(sys.base_prefix) / 'LICENSE.txt', licenses / 'Python-LICENSE.txt')
shutil.copy2(installation / 'Licenses/1033/Redist.txt', licenses / 'Visual-Studio-REDIST.txt')
# Windows 10+ supplies API-set contracts; discard legacy SDK/JDK stub copies.
for stub in internal.rglob('*.dll'):
    if stub.name.lower().startswith(('api-ms-', 'ext-ms-')):
        stub.unlink()
records = []
for source in sorted(redist.glob('*.dll')):
    # Replace arbitrary PATH/system copies selected by dependency scanning.
    for old in internal.iterdir():
        if old.name.lower() == source.name.lower():
            old.unlink()
    shutil.copy2(source, internal / source.name)
    # App-local copies also cover dependencies loaded before Python bootstrap.
    shutil.copy2(source, backend / source.name)
    records.append({'name': source.name, 'sha256': hashlib.sha256(source.read_bytes()).hexdigest()})
root_runtime = app / 'resources/windows-runtime'
root_runtime.mkdir(exist_ok=True)
for name in ['vcruntime140.dll', 'vcruntime140_1.dll']:
    shutil.copy2(internal / name, root_runtime / name)

for name in ['vcruntime140.dll', 'vcruntime140_1.dll', 'msvcp140.dll', 'msvcp140_1.dll']:
    if name not in {record['name'].lower() for record in records}:
        raise RuntimeError(f'Build toolchain lacks {name}')
(licenses / 'windows-runtime-provenance.json').write_text(json.dumps({
    'source': 'Microsoft Visual Studio VC/Redist/MSVC/x64/Microsoft.VC143.CRT',
    'version': version.name, 'files': records,
    'notices': ['Python-LICENSE.txt', 'Visual-Studio-REDIST.txt'],
}, indent=2) + '\n', encoding='utf-8')
print(f'Staged {len(records)} Microsoft VC++ runtime files from redistributable {version.name}')
