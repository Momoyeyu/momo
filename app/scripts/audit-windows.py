"""Check packaged PE dependency closure and redistributable provenance."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import sys

if sys.platform != 'win32':
    raise SystemExit('Run this audit on native Windows')
import pefile
parser = argparse.ArgumentParser()
parser.add_argument('resources', type=Path)
parser.add_argument('--installed', action='store_true')
args = parser.parse_args()
root = args.resources.resolve()
internal = root / 'backend/_internal'
provenance = json.loads((root / 'backend/licenses/windows-runtime-provenance.json').read_text(encoding='utf-8'))
for record in provenance['files']:
    assert hashlib.sha256((internal / record['name']).read_bytes()).hexdigest() == record['sha256'], record['name']
    assert hashlib.sha256((root / 'backend' / record['name']).read_bytes()).hexdigest() == record['sha256'], record['name']
if args.installed:
    for name in ['vcruntime140.dll', 'vcruntime140_1.dll']:
        assert (root / name).read_bytes() == (internal / name).read_bytes(), f'Main executable lacks its own {name}'
files = [path for path in root.rglob('*') if path.suffix.lower() in {'.dll', '.pyd', '.exe'}]
names = {path.name.lower() for path in files}
system = Path(os.environ['SystemRoot']) / 'System32'
system_names = set('''advapi32 bcrypt bcryptprimitives cabinet cfgmgr32 combase comctl32 comdlg32
crypt32 cryptbase cryptsp d2d1 d3d11 d3d12 d3dcompiler_47 dcomp dbghelp dnsapi dwrite dwmapi dxgi
gdi32 imm32 iphlpapi kernel32 msimg32 msvcrt ncrypt netapi32 normaliz ntdll ole32 oleaut32
opengl32 pdh powrprof propsys psapi rasapi32 rpcrt4 secur32 setupapi shell32 shlwapi user32 userenv
usp10 uxtheme version wevtapi winhttp wininet winmm winspool wldap32 ws2_32 wtsapi32 ucrtbase'''.split())
exports = {}
for record in provenance['files']:
    library = pefile.PE(str(internal / record['name']), fast_load=True)
    library.parse_data_directories(directories=[0])
    symbols = library.DIRECTORY_ENTRY_EXPORT.symbols
    exports[record['name'].lower()] = ({item.name for item in symbols if item.name}, {item.ordinal for item in symbols})
    library.close()
missing = []
missing_symbols = []
count = 0
for path in files:
    try:
        image = pefile.PE(str(path), fast_load=True)
    except pefile.PEFormatError:
        continue
    image.parse_data_directories(directories=[1, 13])
    count += 1
    for entry in [*getattr(image, 'DIRECTORY_ENTRY_IMPORT', []), *getattr(image, 'DIRECTORY_ENTRY_DELAY_IMPORT', [])]:
        name = entry.dll.decode('ascii').lower()
        if name in exports:
            named, ordinals = exports[name]
            for symbol in entry.imports:
                available = symbol.name in named if symbol.name else symbol.ordinal in ordinals
                if not available:
                    label = symbol.name.decode('ascii') if symbol.name else f'ordinal {symbol.ordinal}'
                    missing_symbols.append(f'{path.relative_to(root)} -> {name}!{label}')
        if name in names or name.startswith(('api-ms-', 'ext-ms-')):
            continue
        if name.rsplit('.', 1)[0] in system_names and (system / name).is_file():
            continue
        missing.append(f'{path.relative_to(root)} -> {name}')
    image.close()
assert not missing, '\n'.join(missing)
assert not missing_symbols, '\n'.join(missing_symbols)
print(f'PASS {count} packaged PE dependency/export graphs and canonical MSVC {provenance["version"]} hashes')
