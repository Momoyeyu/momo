"""Assemble the locked, self-contained macOS plugin host. No user profiles."""
import hashlib
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile
import urllib.request
import sys

ROOT = Path(__file__).resolve().parents[1]
NODE_ARCHIVE = 'node-v24.11.0-darwin-arm64.tar.gz'
NODE_SHA256 = '0be2ab2816a4fa02d1acff014a434f29f56d8d956f5af6a98b70ced6c5f4d201'


def main():
    if sys.platform != 'darwin' or __import__('platform').machine() != 'arm64':
        raise SystemExit('This candidate supports macOS arm64 only')
    resources = ROOT / 'resources'
    resources.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='momo-plugin-build-') as temp:
        temp = Path(temp)
        archive = temp / NODE_ARCHIVE
        urllib.request.urlretrieve('https://nodejs.org/dist/v24.11.0/' + NODE_ARCHIVE, archive)
        if hashlib.sha256(archive.read_bytes()).hexdigest() != NODE_SHA256:
            raise SystemExit('Node archive checksum mismatch')
        with tarfile.open(archive) as package:
            package.extractall(temp, filter='data')
        source = temp / NODE_ARCHIVE.removesuffix('.tar.gz')
        dest = resources / 'node'
        if dest.exists(): shutil.rmtree(dest)
        (dest / 'bin').mkdir(parents=True)
        shutil.copy2(source / 'bin/node', dest / 'bin/node')
        shutil.copy2(source / 'LICENSE', dest / 'LICENSE')
    host = resources / 'plugin-host'
    if host.exists(): shutil.rmtree(host)
    host.mkdir()
    for name in ('src','resources','package.json','package-lock.json'):
        source = ROOT / 'plugin-host' / name
        if source.is_dir(): shutil.copytree(source,host / name,ignore=shutil.ignore_patterns("__pycache__", "*.pyc"))
        else: shutil.copy2(source,host / name)
    mod = ROOT / 'plugin-host/minecraft-fabric/build/libs/momo-fabric-companion-0.1.0.jar'
    if not mod.is_file(): raise SystemExit('Build plugin-host/minecraft-fabric with Gradle before packaging')
    (host / 'resources/minecraft').mkdir(exist_ok=True)
    shutil.copy2(mod, host / 'resources/minecraft' / mod.name)
    subprocess.run(['npm','ci','--omit=dev','--ignore-scripts'],cwd=host,check=True)
    helpers=resources / 'helpers'
    helpers.mkdir(exist_ok=True)
    sdk=subprocess.check_output(['xcrun','--show-sdk-path'],text=True).strip()
    subprocess.run(['swiftc','-sdk',sdk,'-target','arm64-apple-macos14.0',str(host / 'resources/native-control/control.swift'),'-o',str(helpers / 'momo-native-control')],check=True)
    print('Prepared locked Node, plugin host, native helper')


if __name__ == '__main__': main()
