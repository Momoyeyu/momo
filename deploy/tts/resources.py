"""Download explicit, immutable inference resources; never called by /tts."""
from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import tarfile
import tempfile
import urllib.request
import zipfile

DEFAULT_LOCK = Path(__file__).with_name('resources.lock.json')


def digest(path: Path) -> str:
    result = hashlib.sha256()
    with path.open('rb') as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b''):
            result.update(chunk)
    return result.hexdigest()


def safe_path(root: Path, name: str) -> Path:
    path = root / name
    if Path(name).is_absolute() or '..' in Path(name).parts or '\\' in name:
        raise ValueError(f'unsafe resource/archive path: {name}')
    if not path.resolve().is_relative_to(root.resolve()):
        raise ValueError(f'unsafe resource/archive path: {name}')
    return path


def load_lock(path):
    value = json.loads(Path(path).read_text())
    if value.get('version') != 1 or not value.get('files'):
        raise ValueError('unsupported or empty resource manifest')
    for entry in value['files']:
        if len(entry['sha256']) != 64 or any(c not in '0123456789abcdef' for c in entry['sha256']):
            raise ValueError('manifest requires a SHA-256 for every resource')
        if not entry['url'].startswith('https://'):
            raise ValueError('resource URLs must use HTTPS')
    return value['files']


def fetch_resource(url, path):
    endpoint = os.environ.get('TTS_HF_ENDPOINT', 'https://huggingface.co').rstrip('/')
    if endpoint != 'https://huggingface.co':
        if endpoint != 'https://hf-mirror.com':
            raise ValueError('TTS_HF_ENDPOINT must be the official endpoint or https://hf-mirror.com')
        url = url.replace('https://huggingface.co/', endpoint + '/', 1)
    request = urllib.request.Request(url, headers={'User-Agent': 'momo-tts-resource-setup/1'})
    with urllib.request.urlopen(request, timeout=60) as response, path.open('wb') as output:
        shutil.copyfileobj(response, output, length=1024 * 1024)


def receipt_path(root, entry):
    key = hashlib.sha256(json.dumps(entry, sort_keys=True).encode()).hexdigest()
    return root / '.receipts' / (key + '.json')


def unpack(root: Path, archive_path: Path, entry):
    destination = safe_path(root, entry['extract_to'])
    with tempfile.TemporaryDirectory(prefix='.extract-', dir=root) as temporary:
        staging = Path(temporary)
        names = []
        total = 0
        def write(name, size, source):
            nonlocal total
            total += size
            if size > 2 * 1024**3 or total > 3 * 1024**3:
                raise ValueError('archive exceeds extraction size limit')
            target = safe_path(staging, name)
            target.parent.mkdir(parents=True, exist_ok=True)
            with target.open('xb') as output:
                shutil.copyfileobj(source, output)
            names.append(name)
        if entry['archive'] == 'zip':
            with zipfile.ZipFile(archive_path) as archive:
                # Validate every entry before writing any, including directories.
                for info in archive.infolist():
                    safe_path(staging, info.filename)
                    if (info.external_attr >> 16) & 0o170000 == 0o120000:
                        raise ValueError('archive symlinks are not supported')
                for info in archive.infolist():
                    if not info.is_dir():
                        with archive.open(info) as source:
                            write(info.filename, info.file_size, source)
        elif entry['archive'] == 'tar.gz':
            with tarfile.open(archive_path, 'r:gz') as archive:
                for info in archive.getmembers():
                    safe_path(staging, info.name)
                    if not info.isfile() and not info.isdir():
                        raise ValueError('archive links/devices are not supported')
                for info in archive.getmembers():
                    if info.isfile():
                        with archive.extractfile(info) as source:
                            write(info.name, info.size, source)
        else:
            raise ValueError('unsupported resource archive type')
        if not names:
            raise ValueError('resource archive is empty')
        receipt = {}
        for name in names:
            source = staging / name
            target = safe_path(destination, name)
            target.parent.mkdir(parents=True, exist_ok=True)
            receipt[str(target.relative_to(root))] = digest(source)
            os.replace(source, target)
        path = receipt_path(root, entry)
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(receipt, sort_keys=True))


def verify_entry(root, entry):
    path = safe_path(root, entry['path'])
    if not path.is_file():
        raise FileNotFoundError(f'missing {entry["path"]}; run the models setup command')
    if digest(path) != entry['sha256']:
        raise ValueError(f'checksum mismatch: {entry["path"]}; rerun models setup')
    if entry.get('archive'):
        receipt = receipt_path(root, entry)
        if not receipt.is_file():
            raise FileNotFoundError(f'missing extraction receipt for {entry["path"]}; rerun models setup')
        extracted = json.loads(receipt.read_text())
        if not extracted:
            raise ValueError('empty resource extraction receipt')
        for name, expected in extracted.items():
            actual = safe_path(root, name)
            if not actual.is_file():
                raise FileNotFoundError(f'missing extracted resource {name}; rerun models setup')
            if digest(actual) != expected:
                raise ValueError(f'checksum mismatch: {name}; rerun models setup')


def verify_resources(root: Path, manifest=DEFAULT_LOCK):
    for entry in load_lock(manifest):
        verify_entry(Path(root), entry)


def download_resources(root: Path, manifest=DEFAULT_LOCK, *, fetch=fetch_resource):
    root = Path(root); root.mkdir(parents=True, exist_ok=True)
    for entry in load_lock(manifest):
        try:
            verify_entry(root, entry)
            continue
        except (FileNotFoundError, ValueError):
            pass
        path = safe_path(root, entry['path'])
        path.parent.mkdir(parents=True, exist_ok=True)
        if not path.is_file() or digest(path) != entry['sha256']:
            descriptor, temporary = tempfile.mkstemp(suffix='.part', dir=path.parent)
            os.close(descriptor); temporary = Path(temporary)
            try:
                print(f'Downloading {entry["path"]}', flush=True)
                try:
                    fetch(entry['url'], temporary)
                except OSError as error:
                    raise OSError(f'could not download {entry["path"]}: {error}; retry models setup; verified files are retained') from None
                if digest(temporary) != entry['sha256']:
                    raise ValueError(f'checksum mismatch: {entry["path"]}; downloaded file was not installed')
                os.replace(temporary, path)
            finally:
                temporary.unlink(missing_ok=True)
        if entry.get('archive'):
            unpack(root, path, entry)
    verify_resources(root, manifest)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('command', choices=['download', 'verify'])
    parser.add_argument('--root', type=Path, default=Path('/models'))
    parser.add_argument('--manifest', type=Path, default=DEFAULT_LOCK)
    args = parser.parse_args()
    try:
        (download_resources if args.command == 'download' else verify_resources)(args.root, args.manifest)
    except (OSError, ValueError, KeyError, zipfile.BadZipFile, tarfile.TarError) as error:
        parser.exit(1, f'Resource preparation failed: {error}\n')
