"""Copy only a reviewed QQ resource closure, never a live account or runtime config."""
import argparse
import hashlib
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
MANIFEST_SHA256 = '28a3b6868cdd1ab4be884a37fe9a575e8f6ba3636612d551031b6928698db3b1'


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--resources', type=Path, required=True)
    parser.add_argument('--manifest', type=Path, required=True)
    args = parser.parse_args()
    content = args.manifest.read_bytes()
    if hashlib.sha256(content).hexdigest() != MANIFEST_SHA256:
        raise SystemExit('QQ manifest differs from the reviewed release')
    destination = ROOT / 'resources/qq'
    if destination.exists():
        raise SystemExit('QQ bundle already exists; remove this generated resource explicitly to rebuild')
    with tempfile.TemporaryDirectory(dir=destination.parent, prefix='.qq-build-') as temporary:
        staging = Path(temporary)
        shutil.copytree(args.resources, staging / 'resources', symlinks=True)
        (staging / 'manifest.json').write_bytes(content)
        script = """import {readFileSync} from 'node:fs';
const {qualifyQQResources}=await import(process.argv[1]);
await qualifyQQResources({root:process.argv[2],manifest:JSON.parse(readFileSync(process.argv[3],'utf8'))});"""
        subprocess.run([str(ROOT / 'resources/node/bin/node'), '--input-type=module', '-e', script,
                        (ROOT / 'plugin-host/src/qq-resources.mjs').as_uri(),
                        str(staging / 'resources'), str(staging / 'manifest.json')], check=True)
        staging.rename(destination)
    print('Prepared resource-only QQ bundle; no account imported or connected')


if __name__ == '__main__': main()
