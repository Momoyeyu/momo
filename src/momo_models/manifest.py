"""models/MANIFEST.toml 解析与 sha256 校验。

口径见 momo-docs/02-repos/momo-models.md「模型目录布局与下载清单」：
每条目 `name / repo_id / revision / files[] / dest / notes`，
`files[]` 元素为 `{path, sha256, size?}`。下载由 momo-infra `momoctl pull`
执行；运行时 provider 只读本地路径，启动校验失败 → readyz 拒绝就绪。
"""

import hashlib
import tomllib
from dataclasses import dataclass, field
from pathlib import Path

DEFAULT_MANIFEST = Path(__file__).resolve().parents[2] / "models" / "MANIFEST.toml"

_SHA256_CHUNK = 1 << 20  # 1 MiB


@dataclass(frozen=True)
class ManifestFile:
    path: str
    sha256: str
    size: int | None = None


@dataclass(frozen=True)
class ManifestEntry:
    name: str
    repo_id: str
    revision: str
    dest: str
    files: tuple[ManifestFile, ...] = field(default_factory=tuple)
    notes: str = ""


@dataclass(frozen=True)
class VerifyResult:
    """单条目校验结果。`ok` 为真表示全部文件存在且 sha256 一致。"""

    entry: str
    ok: bool
    missing: tuple[str, ...] = ()
    mismatched: tuple[str, ...] = ()


class ManifestError(ValueError):
    """MANIFEST.toml 结构非法。"""


def sha256_file(path: Path) -> str:
    """流式计算文件 sha256，返回小写 hex。"""
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(_SHA256_CHUNK), b""):
            h.update(chunk)
    return h.hexdigest()


def load_manifest(path: Path = DEFAULT_MANIFEST) -> list[ManifestEntry]:
    """解析 MANIFEST.toml；结构非法抛 ManifestError。"""
    raw = tomllib.loads(path.read_text(encoding="utf-8"))
    entries = []
    for i, item in enumerate(raw.get("model", [])):
        where = f"model[{i}]({item.get('name', '?')})"
        try:
            files = tuple(
                ManifestFile(
                    path=str(f["path"]),
                    sha256=str(f["sha256"]).lower(),
                    size=int(f["size"]) if "size" in f else None,
                )
                for f in item.get("files", [])
            )
            entries.append(
                ManifestEntry(
                    name=str(item["name"]),
                    repo_id=str(item["repo_id"]),
                    revision=str(item["revision"]),
                    dest=str(item["dest"]),
                    files=files,
                    notes=str(item.get("notes", "")),
                )
            )
        except KeyError as e:
            raise ManifestError(f"{where}: 缺少必填字段 {e}") from e
    return entries


def verify_entry(entry: ManifestEntry, models_dir: Path) -> VerifyResult:
    """按 sha256 校验 `models_dir/entry.dest/` 下全部声明文件。

    sha256 为 "TODO" 占位的文件跳过内容校验（清单未补齐阶段不阻塞），
    但文件缺失仍计入 missing。
    """
    missing: list[str] = []
    mismatched: list[str] = []
    root = models_dir / entry.dest
    for f in entry.files:
        fp = root / f.path
        if not fp.is_file():
            missing.append(f.path)
            continue
        if f.sha256 == "todo":
            continue
        if sha256_file(fp) != f.sha256:
            mismatched.append(f.path)
    return VerifyResult(
        entry=entry.name,
        ok=not missing and not mismatched,
        missing=tuple(missing),
        mismatched=tuple(mismatched),
    )


def verify_all(models_dir: Path, path: Path = DEFAULT_MANIFEST) -> list[VerifyResult]:
    """逐条目校验；供服务启动时 readyz 门控与脚本复用。"""
    return [verify_entry(e, models_dir) for e in load_manifest(path)]
