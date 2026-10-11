"""Provider 抽象基类：签名对齐 momo-docs/02-repos/momo-models.md「统一服务接口」。

- LLMProvider 仅供 eval/bench 薄客户端使用；运行时编排器经 MOMO_LLM_ENDPOINT
  直连大脑，不经本层。
- TTSProvider：momo-svc-tts 后端统一契约，输出 PCM 24k mono。
- ASRProvider：momo-svc-asr 后端统一契约，输入 PCM 16k mono。
"""

from abc import ABC, abstractmethod
from collections.abc import AsyncIterator
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, ClassVar

EMOTION_TAGS = (
    "neutral",
    "happy",
    "sad",
    "angry",
    "surprised",
    "thinking",
    "shy",
    "sleepy",
)


# ---------- LLM（eval/bench only）----------


@dataclass
class ChatRequest:
    """OpenAI-compatible 聊天请求（评测口径子集）。"""

    model: str
    messages: list[dict[str, Any]]
    temperature: float | None = None
    max_tokens: int | None = None
    extra: dict[str, Any] = field(default_factory=dict)


@dataclass
class ChatDelta:
    """流式增量。`finish_reason` 非空表示流结束。"""

    text: str = ""
    finish_reason: str | None = None
    usage: dict[str, int] | None = None


class LLMProvider(ABC):
    """仅 eval/bench 用；运行时编排器直连大脑，不经本层。"""

    name: ClassVar[str]

    @abstractmethod
    def chat_stream(self, req: ChatRequest) -> AsyncIterator[ChatDelta]:
        """流式聊天完成；实现为 async generator。"""

    @abstractmethod
    async def list_models(self) -> list[str]: ...

    @abstractmethod
    async def aclose(self) -> None: ...


# ---------- TTS ----------


@dataclass(frozen=True)
class TTSCaps:
    """能力声明：服务层据此决定可否流式/克隆/带情感。"""

    streaming: bool
    languages: tuple[str, ...]
    clone: bool
    emotion: bool


@dataclass(frozen=True)
class VoiceRef:
    """克隆参考声线。`ref_audio` 为空表示使用后端默认音色。"""

    ref_audio: Path | None = None
    ref_text: str = ""
    lang: str = "auto"


class TTSProvider(ABC):
    name: ClassVar[str]

    @abstractmethod
    def capabilities(self) -> TTSCaps:
        """能力集：streaming/languages/clone/emotion。"""

    @abstractmethod
    async def warmup(self) -> None:
        """加载模型并就绪；readyz 以此为据。"""

    @abstractmethod
    def synth_stream(
        self, text: str, *, voice: VoiceRef, emotion: str | None
    ) -> AsyncIterator[bytes]:
        """逐块产出 PCM s16le 24kHz mono；实现为 async generator。"""

    @abstractmethod
    async def cancel(self, req_id: str) -> None:
        """尽力取消在途合成（200ms 内停止产出，协议 §6 control.cancel）。"""


# ---------- ASR ----------


@dataclass(frozen=True)
class Transcript:
    """一个 VAD 段的转写结果（对应协议 input.audio.segment）。"""

    text: str
    lang: str
    duration_ms: int
    speech_emotion: str | None = None
    events: tuple[str, ...] = ()


class ASRProvider(ABC):
    name: ClassVar[str]

    @abstractmethod
    async def load(self) -> None:
        """加载模型；readyz 以此为据。"""

    @abstractmethod
    async def transcribe(self, pcm: bytes, *, lang: str | None) -> Transcript:
        """转写一段 PCM s16le 16kHz mono。"""
