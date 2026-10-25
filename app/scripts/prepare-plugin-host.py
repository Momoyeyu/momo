"""Prepare the reviewed Cordis host and pinned, offline macOS Node runtime.

Only package/lock/src/README and production dependencies enter resources. No
developer environment, user data, test fixture or credentials are copied.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import tarfile
import tempfile
import urllib.request

VERSION = '22.23.3'
SHA256 = '23b25245dcfb9af7262f8ff142e9e2e0af025368117329e7a7458a51e5922f53'
APP = Path(__file__).resolve().parents[1]


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--node-archive', type=Path)
    args = parser.parse_args()
    if (platform.system(), platform.machine()) != ('Darwin', 'arm64'):
        raise SystemExit('This validated resource preparation currently supports macOS arm64 only')
    source = APP / 'plugin-host'
    for name in ('package.json', 'package-lock.json', 'src/index.mjs', 'src/host.mjs'):
        if not (source / name).is_file(): raise SystemExit('Promote the pinned Web plugin-host sources before preparing resources')
    cache = APP / 'target/downloads'
    cache.mkdir(parents=True, exist_ok=True)
    archive = args.node_archive or cache / f'node-v{VERSION}-darwin-arm64.tar.gz'
    if not archive.is_file():
        url = f'https://nodejs.org/dist/v{VERSION}/node-v{VERSION}-darwin-arm64.tar.gz'
        temporary = archive.with_suffix('.part')
        try:
            with urllib.request.urlopen(url, timeout=60) as response, temporary.open('wb') as out:
                shutil.copyfileobj(response, out)
            if digest(temporary) != SHA256: raise SystemExit('Downloaded Node archive failed its pinned SHA-256')
            os.replace(temporary, archive)
        finally: temporary.unlink(missing_ok=True)
    if digest(archive) != SHA256: raise SystemExit('Node archive failed its pinned SHA-256')
    resources = APP / 'resources'
    resources.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='.momo-host-', dir=resources) as temporary:
        stage = Path(temporary)
        with tarfile.open(archive) as tar:
            tar.extractall(stage, filter='data')
        node = stage / f'node-v{VERSION}-darwin-arm64'
        # npm is a build tool, not an end-user dependency. The installed app only
        # needs the runtime executable and its license.
        host = stage / 'plugin-host'
        host.mkdir()
        for name in ('package.json', 'package-lock.json', 'README.md'):
            if (source / name).is_file(): shutil.copy2(source / name, host / name)
        project_license = APP.parent / 'LICENSE.md'
        if project_license.is_file(): shutil.copy2(project_license, host / 'LICENSE.md')
        shutil.copytree(source / 'src', host / 'src')
        env = {**os.environ, 'PATH': str(node / 'bin') + os.pathsep + os.environ.get('PATH', ''),
               'NPM_CONFIG_CACHE': str(APP / 'target/npm-cache')}
        subprocess.run([str(node / 'bin/node'), str(node / 'lib/node_modules/npm/bin/npm-cli.js'),
                        'ci', '--omit=dev', '--ignore-scripts', '--no-audit', '--no-fund'], cwd=host, env=env, check=True)
        version = subprocess.check_output([str(node / 'bin/node'), '--version'], text=True).strip()
        if version != 'v' + VERSION: raise SystemExit('Unexpected Node executable version')
        runtime = stage / 'runtime'
        (runtime / 'bin').mkdir(parents=True)
        shutil.copy2(node / 'bin/node', runtime / 'bin/node')
        shutil.copy2(node / 'LICENSE', runtime / 'LICENSE')
        manifest = {'nodeVersion': VERSION, 'nodeArchiveSha256': SHA256, 'platform': 'darwin-arm64',
                    'sourceFiles': {str(path.relative_to(source)): digest(path) for path in sorted((source / 'src').rglob('*')) if path.is_file()},
                    'packageLockSha256': digest(source / 'package-lock.json')}
        (host / 'runtime-manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
        for name, origin in (('plugin-host', host), ('node', runtime)):
            target = resources / name
            if target.exists(): shutil.rmtree(target)
            shutil.move(str(origin), target)
    print(f'Prepared Cordis host and Node {VERSION} in {resources}')


if __name__ == '__main__': main()
