import hashlib
import io
import json
import zipfile
from pathlib import Path

import pytest

from resources import download_resources, verify_resources


def lock(tmp_path, data=b'weights', **extra):
    path = tmp_path / 'lock.json'
    value = {'version': 1, 'files': [{'path': 'pretrained/model.bin', 'url': 'https://example.invalid/model', 'sha256': hashlib.sha256(data).hexdigest(), **extra}]}
    path.write_text(json.dumps(value));return path


def test_download_is_checked_and_cached_then_corruption_is_detected(tmp_path):
    root = tmp_path / 'models'; calls=[]
    def fetch(url, path): calls.append(url);path.write_bytes(b'weights')
    manifest=lock(tmp_path)
    download_resources(root, manifest, fetch=fetch)
    verify_resources(root, manifest)
    download_resources(root, manifest, fetch=fetch)
    assert len(calls)==1
    (root/'pretrained/model.bin').write_bytes(b'broken')
    with pytest.raises(ValueError,match='checksum'):verify_resources(root,manifest)


def test_wrong_hash_is_not_installed(tmp_path):
    root=tmp_path/'models'
    with pytest.raises(ValueError,match='checksum'):
        download_resources(root,lock(tmp_path),fetch=lambda url,path:path.write_bytes(b'wrong'))
    assert not (root/'pretrained/model.bin').exists()
    assert not list(root.rglob('*.part'))


def test_missing_resource_reports_actionable_path(tmp_path):
    with pytest.raises(FileNotFoundError,match='pretrained/model.bin'):
        verify_resources(tmp_path/'models',lock(tmp_path))


def test_malicious_archive_cannot_escape_model_volume(tmp_path):
    data=io.BytesIO()
    with zipfile.ZipFile(data,'w') as archive:archive.writestr('../escape',b'invalid')
    raw=data.getvalue();manifest=lock(tmp_path,raw,extract_to='dict',archive='zip')
    with pytest.raises(ValueError,match='archive'):
        download_resources(tmp_path/'models',manifest,fetch=lambda url,path:path.write_bytes(raw))
    assert not (tmp_path/'escape').exists()


def test_extracted_resource_receipt_detects_corruption(tmp_path):
    data=io.BytesIO()
    with zipfile.ZipFile(data,'w') as archive:archive.writestr('dictionary/data',b'dict')
    raw=data.getvalue();manifest=lock(tmp_path,raw,extract_to='language',archive='zip')
    root=tmp_path/'models'
    download_resources(root,manifest,fetch=lambda url,path:path.write_bytes(raw))
    verify_resources(root,manifest)
    (root/'language/dictionary/data').write_bytes(b'corrupt')
    with pytest.raises(ValueError,match='checksum'):verify_resources(root,manifest)
