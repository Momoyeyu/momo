"""MANIFEST.toml 解析与 sha256 校验（全本地假文件，无网络/GPU）。"""

import hashlib
from pathlib import Path

import pytest

from momo_models.manifest import (
    ManifestError,
    load_manifest,
    sha256_file,
    verify_entry,
)

CONTENT_A = b"hello momo"
SHA_A = hashlib.sha256(CONTENT_A).hexdigest()

MANIFEST_OK = f"""
[[model]]
name = "fake-model"
repo_id = "owner/repo"
revision = "abc123"
dest = "owner/repo"
notes = "test"
files = [
  {{ path = "a.bin", sha256 = "{SHA_A}" }},
  {{ path = "b.bin", sha256 = "TODO" }},
]
"""

MANIFEST_BAD = """
[[model]]
name = "broken"
repo_id = "owner/repo"
# 缺 revision / dest
files = []
"""


def _write_manifest(tmp_path: Path, text: str = MANIFEST_OK) -> Path:
    p = tmp_path / "MANIFEST.toml"
    p.write_text(text, encoding="utf-8")
    return p


def test_load_manifest_fields(tmp_path: Path) -> None:
    entries = load_manifest(_write_manifest(tmp_path))
    assert len(entries) == 1
    e = entries[0]
    assert e.name == "fake-model"
    assert e.repo_id == "owner/repo"
    assert e.revision == "abc123"
    assert e.dest == "owner/repo"
    assert [f.path for f in e.files] == ["a.bin", "b.bin"]
    assert e.files[0].sha256 == SHA_A


def test_load_manifest_missing_field(tmp_path: Path) -> None:
    with pytest.raises(ManifestError):
        load_manifest(_write_manifest(tmp_path, MANIFEST_BAD))


def test_sha256_file(tmp_path: Path) -> None:
    f = tmp_path / "x.bin"
    f.write_bytes(CONTENT_A)
    assert sha256_file(f) == SHA_A


def test_verify_entry_ok_and_todo_skipped(tmp_path: Path) -> None:
    """sha256 匹配 + TODO 占位跳过内容校验 → ok。"""
    entry = load_manifest(_write_manifest(tmp_path))[0]
    root = tmp_path / "models" / "owner" / "repo"
    root.mkdir(parents=True)
    (root / "a.bin").write_bytes(CONTENT_A)
    (root / "b.bin").write_bytes(b"anything")  # TODO 占位不校验内容
    result = verify_entry(entry, tmp_path / "models")
    assert result.ok
    assert not result.missing
    assert not result.mismatched


def test_verify_entry_missing(tmp_path: Path) -> None:
    entry = load_manifest(_write_manifest(tmp_path))[0]
    result = verify_entry(entry, tmp_path / "models")
    assert not result.ok
    assert result.missing == ("a.bin", "b.bin")


def test_verify_entry_mismatch(tmp_path: Path) -> None:
    entry = load_manifest(_write_manifest(tmp_path))[0]
    root = tmp_path / "models" / "owner" / "repo"
    root.mkdir(parents=True)
    (root / "a.bin").write_bytes(b"tampered")
    (root / "b.bin").write_bytes(b"x")
    result = verify_entry(entry, tmp_path / "models")
    assert not result.ok
    assert result.mismatched == ("a.bin",)


def test_real_manifest_parses() -> None:
    """仓内 models/MANIFEST.toml 骨架本身须可被解析。"""
    from momo_models.manifest import DEFAULT_MANIFEST

    entries = load_manifest(DEFAULT_MANIFEST)
    names = {e.name for e in entries}
    assert {"kokoro-82m", "f5-tts-v1", "sense-voice-small-int8"} <= names
    for e in entries:
        assert e.repo_id and e.dest
