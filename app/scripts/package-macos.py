"""Create an installable macOS DMG without requiring Finder/AppleScript."""
from pathlib import Path
import platform
import argparse
import shutil
import subprocess
import tempfile

app = Path(__file__).resolve().parents[1]
bundle = app / 'src-tauri/target/release/bundle'
parser = argparse.ArgumentParser()
parser.add_argument('--app-bundle', type=Path, default=bundle / 'macos/momo.app')
parser.add_argument('--sign-only', action='store_true')
args = parser.parse_args()
source = args.app_bundle.resolve()
if platform.system() != 'Darwin' or not source.is_dir():
    raise SystemExit('Build the macOS app bundle first: pnpm build --bundles app')
# Ad-hoc local test signing seals bundled resources; notarized release signing is separate.
subprocess.run(['codesign', '--force', '--sign', '-', '--entitlements',
    str(app / 'src-tauri/Entitlements.plist'), str(source)], check=True)
subprocess.run(['codesign', '--verify', '--deep', '--strict', str(source)], check=True)
if args.sign_only:
    raise SystemExit(0)
version = __import__('json').loads((app / 'package.json').read_text())['version']
output = bundle / 'dmg' / f'momo_{version}_{platform.machine()}.dmg'
output.parent.mkdir(parents=True, exist_ok=True)
with tempfile.TemporaryDirectory(prefix='momo-dmg-') as staging:
    shutil.copytree(source, Path(staging) / 'momo.app', symlinks=True)
    (Path(staging) / 'Applications').symlink_to('/Applications')
    subprocess.run(['hdiutil', 'create', '-volname', 'momo', '-srcfolder', staging,
        '-ov', '-format', 'UDZO', str(output)], check=True)
print(output)
