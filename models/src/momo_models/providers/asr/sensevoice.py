"""SenseVoice-Small via sherpa-onnx（纯 CPU）：实测 RTF 0.040，CPU×4。

zh/en/ja/ko/yue + 自动语言 + 情感/事件标记。注意 onnx 与 tokens.txt 配套；
模型路径 = $MOMO_MODELS_DIR/sherpa-onnx/sense-voice-*/。见 momo-models.md 后端表。

sherpa-onnx 懒加载：核心依赖不带重库，import 仅在 load()/transcribe() 内发生，
测试通过 sys.modules 注入假 sherpa_onnx。
"""

import asyncio
import re
from pathlib import Path

from momo_models.audio import UPLINK_SAMPLE_RATE, s16le_bytes_to_numpy
from momo_models.config import Settings
from momo_models.providers.base import ASRProvider, Transcript

# 与 models/MANIFEST.toml 中 sense-voice-small-int8 条目的 dest/name 保持一致；
# 权重实体在 $MOMO_MODELS_DIR 下镜像 <owner>/<repo> 布局，不入库。
_MODEL_SUBDIR = Path("sherpa-onnx") / "sense-voice-zh-en-ja-ko-yue-2024-07-17"
_MANIFEST_ENTRY = "sense-voice-small-int8"

# SenseVoice 支持的语言档（sherpa-onnx from_sense_voice(language=...) 口径）。
_VALID_LANGS = ("auto", "zh", "en", "ja", "ko", "yue")

# sherpa-onnx SenseVoice 结果中的元标记：<|zh|> / <|HAPPY|> / <|Speech|> 等。
_TAG_RE = re.compile(r"<\|([^|]*)\|>")
_KNOWN_LANGS = frozenset(_VALID_LANGS[1:])

# SenseVoice 情感标记全集；小写后写入 Transcript.speech_emotion
# （EMOTION_TAGS 是 persona/UI 侧词表，这里保留模型原生判定值）。
_EMOTION_TAGS = frozenset({"neutral", "happy", "sad", "angry", "fearful", "disgusted", "surprised"})


class SenseVoiceError(RuntimeError):
    """模型文件缺失 / checksum 不符 / 未 load 即调用。"""


def _import_sherpa():
    """懒加载 sherpa_onnx（可选依赖，`uv sync --extra asr` 安装）。"""
    try:
        import sherpa_onnx
    except ImportError as e:  # pragma: no cover —— 无重依赖环境兜底
        raise SenseVoiceError(
            "缺少 sherpa-onnx 依赖：请在 models/ 下执行 `uv sync --extra asr`"
        ) from e
    return sherpa_onnx


def _normalize_lang(lang: str | None) -> str:
    """transcribe 语言提示 → sherpa language 档；非法值抛 ValueError。"""
    if lang is None:
        return "auto"
    lowered = lang.strip().lower()
    if lowered in ("", "auto"):
        return "auto"
    if lowered not in _KNOWN_LANGS:
        raise ValueError(f"unsupported asr lang {lang!r}; valid: auto|{'|'.join(_KNOWN_LANGS)}")
    return lowered


def _strip_tag(raw: str) -> str:
    """'<|HAPPY|>' → 'happy'；无包装则原样小写。"""
    m = _TAG_RE.fullmatch(raw.strip())
    return (m.group(1) if m else raw).strip().lower()


def _parse_result(result, hint: str) -> tuple[str, str, str | None, tuple[str, ...]]:
    """从 sherpa OfflineRecognitionResult 拆出 (text, lang, emotion, events)。

    sherpa-onnx ≥1.10 把 SenseVoice 元标记拆进 result.lang/emotion/event
    （形如 '<|zh|>'），text 已是净文本；旧版/异常情况下标记仍残留在
    text 中，此时回退为正则解析并按类别归位。
    """
    raw_text = str(getattr(result, "text", "") or "")
    lang = _strip_tag(str(getattr(result, "lang", "") or ""))
    emotion = _strip_tag(str(getattr(result, "emotion", "") or ""))
    event = _strip_tag(str(getattr(result, "event", "") or ""))

    events: list[str] = []
    if event:
        events.append(event)

    # 净文本：剥掉所有残留的 <|...|> 标记，顺便兜底提取元信息。
    def _collect(m: re.Match) -> str:
        nonlocal lang, emotion
        tag = m.group(1).strip().lower()
        if tag in _KNOWN_LANGS and not lang:
            lang = tag
        elif tag in _EMOTION_TAGS and not emotion:
            emotion = tag
        elif tag and tag not in ("woitn", "withitn", "itn") and tag not in events:
            events.append(tag)
        return ""

    text = _TAG_RE.sub(_collect, raw_text).strip()

    if lang not in _KNOWN_LANGS:
        lang = hint if hint in _KNOWN_LANGS else "unknown"
    return text, lang, emotion or None, tuple(events)


class SenseVoiceASRProvider(ASRProvider):
    """SenseVoice-Small（sherpa-onnx，纯 CPU）→ Transcript。

    - load()：定位 $MOMO_MODELS_DIR 下模型文件，按 MANIFEST 校验 sha256，
      构建 language=auto 的 recognizer。
    - transcribe()：按 lang 提示懒建对应 recognizer（同一 onnx 会话按语言
      分实例缓存），解码在 worker 线程执行以免阻塞事件循环。
    """

    name = "sensevoice"

    def __init__(self, settings: Settings) -> None:
        self._settings = settings
        self.model = settings.asr_model
        self.threads = settings.asr_threads
        self._recognizers: dict[str, object] = {}
        self._model_path: Path | None = None
        self._tokens_path: Path | None = None

    # ---------- 模型定位与校验 ----------

    def _model_dir(self) -> Path:
        return self._settings.models_dir.expanduser() / _MODEL_SUBDIR

    def _resolve_files(self) -> tuple[Path, Path]:
        """定位 onnx 权重（优先 int8）与 tokens.txt；缺失抛 SenseVoiceError。"""
        root = self._model_dir()
        model = root / "model.int8.onnx"
        if not model.is_file():
            model = root / "model.onnx"
        tokens = root / "tokens.txt"
        missing = [p.name for p in (model, tokens) if not p.is_file()]
        if missing:
            raise SenseVoiceError(
                f"sensevoice 模型文件缺失 {missing}（期望目录 {root}）；"
                "请先 `momoctl pull --only sense-voice-small-int8` 或按 MANIFEST 手动下载"
            )
        return model, tokens

    def _verify_manifest(self) -> None:
        """按 MANIFEST.toml 校验 sha256；清单缺失时只做文件存在性检查。"""
        from momo_models.manifest import DEFAULT_MANIFEST, load_manifest, verify_entry

        if not DEFAULT_MANIFEST.is_file():
            return
        entry = next(
            (e for e in load_manifest(DEFAULT_MANIFEST) if e.name == _MANIFEST_ENTRY),
            None,
        )
        if entry is None:
            return
        result = verify_entry(entry, self._settings.models_dir.expanduser())
        if result.mismatched:
            raise SenseVoiceError(
                f"sensevoice 模型 sha256 校验失败 {list(result.mismatched)}；请重新拉取模型文件"
            )

    # ---------- 识别 ----------

    def _build_recognizer(self, language: str):
        sherpa = _import_sherpa()
        assert self._model_path is not None and self._tokens_path is not None
        return sherpa.OfflineRecognizer.from_sense_voice(
            model=str(self._model_path),
            tokens=str(self._tokens_path),
            num_threads=max(1, self.threads),
            sample_rate=UPLINK_SAMPLE_RATE,
            provider="cpu",
            language=language,
            use_itn=True,
            debug=False,
        )

    async def _recognizer(self, language: str):
        """按语言档缓存 recognizer；auto 已在 load() 建好。"""
        rec = self._recognizers.get(language)
        if rec is None:
            rec = await asyncio.to_thread(self._build_recognizer, language)
            self._recognizers[language] = rec
        return rec

    def _decode(self, recognizer, samples) -> object:
        stream = recognizer.create_stream()
        stream.accept_waveform(UPLINK_SAMPLE_RATE, samples)
        recognizer.decode_stream(stream)
        return stream.result

    # ---------- ASRProvider 契约 ----------

    async def load(self) -> None:
        """加载模型；readyz 以此为据。"""
        model, tokens = self._resolve_files()
        self._verify_manifest()
        self._model_path, self._tokens_path = model, tokens
        self._recognizers["auto"] = await asyncio.to_thread(self._build_recognizer, "auto")

    async def transcribe(self, pcm: bytes, *, lang: str | None) -> Transcript:
        """转写一段 PCM s16le 16kHz mono。"""
        hint = _normalize_lang(lang)
        if not self._recognizers:
            raise SenseVoiceError("sensevoice 未 load：先等待 readyz 就绪")

        # int16 对齐防御：奇数字节尾截断，空输入直接短路。
        if len(pcm) % 2:
            pcm = pcm[:-1]
        if not pcm:
            return Transcript(text="", lang=hint if hint != "auto" else "unknown", duration_ms=0)

        samples = s16le_bytes_to_numpy(pcm)
        duration_ms = round(len(samples) / UPLINK_SAMPLE_RATE * 1000)
        recognizer = await self._recognizer(hint)
        result = await asyncio.to_thread(self._decode, recognizer, samples)
        text, detected, emotion, events = _parse_result(result, hint)
        return Transcript(
            text=text,
            lang=detected,
            duration_ms=duration_ms,
            speech_emotion=emotion,
            events=events,
        )
