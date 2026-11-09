"""Assemble the locked, self-contained platform plugin host. No user profiles."""
import hashlib
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile
import urllib.request
import sys
import platform
import zipfile
import argparse

ROOT = Path(__file__).resolve().parents[1]
NODE_ARCHIVE = 'node-v24.11.0-darwin-arm64.tar.gz'
NODE_SHA256 = '0be2ab2816a4fa02d1acff014a434f29f56d8d956f5af6a98b70ced6c5f4d201'


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--node-archive", type=Path)
    args = parser.parse_args()
    targets = {
        ('darwin', 'arm64'): (NODE_ARCHIVE, NODE_SHA256),
        ('linux', 'x86_64'): ('node-v24.11.0-linux-x64.tar.gz', 'b3c071cdf47aab867c3b2aa287257df12ec5d7c962bf922b32fd33226c4295fd'),
        ('win32', 'amd64'): ('node-v24.11.0-win-x64.zip', '1054540bce22b54ec7e50ebc078ec5d090700a77657607a58f6a64df21f49fdd'),
    }
    target = targets.get((sys.platform, platform.machine().lower()))
    if not target: raise SystemExit('Unsupported platform/architecture')
    archive_name, checksum = target
    resources = ROOT / 'resources'
    resources.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='momo-plugin-build-') as temp:
        temp = Path(temp)
        archive = temp / archive_name
        if args.node_archive: shutil.copyfile(args.node_archive, archive)
        else: urllib.request.urlretrieve('https://nodejs.org/dist/v24.11.0/' + archive_name, archive)
        if hashlib.sha256(archive.read_bytes()).hexdigest() != checksum:
            raise SystemExit('Node archive checksum mismatch')
        if archive_name.endswith('.zip'):
            with zipfile.ZipFile(archive) as package: package.extractall(temp)
        else:
            with tarfile.open(archive) as package: package.extractall(temp, filter='data')
        source = temp / archive_name.removesuffix('.tar.gz').removesuffix('.zip')
        dest = resources / 'node'
        if dest.exists(): shutil.rmtree(dest)
        (dest / 'bin').mkdir(parents=True)
        shutil.copy2(source / ('node.exe' if sys.platform == 'win32' else 'bin/node'), dest / ('node.exe' if sys.platform == 'win32' else 'bin/node'))
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
    subprocess.run([shutil.which('npm.cmd' if sys.platform == 'win32' else 'npm') or 'npm','ci','--omit=dev','--ignore-scripts'],cwd=host,check=True)
    helpers=resources / 'helpers'
    helpers.mkdir(exist_ok=True)
    if sys.platform != 'darwin':
        print('Prepared Node and plugin host; native desktop control unavailable on this platform')
        return
    sdk=subprocess.check_output(['xcrun','--show-sdk-path'],text=True).strip()
    subprocess.run(['swiftc','-sdk',sdk,'-target','arm64-apple-macos14.0',str(host / 'resources/native-control/control.swift'),'-o',str(helpers / 'momo-native-control')],check=True)
    print('Prepared locked Node, plugin host, native helper')


if __name__ == '__main__': main()
