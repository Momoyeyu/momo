"""F5-TTS v1 零样本克隆（已实测）：RTF 0.535、VRAM 0.8GB。

接入：`f5-tts` pip 包，零样本传参考音频（MOMO_TTS_VOICE_REF）。
已知坑：参考音频需干净单人声。见 docs/02-repos/momo-models.md「provider 与产物」。
"""

from collections.abc import AsyncIterator

from momo_models.config import Settings
from momo_models.providers.base import TTSCaps, TTSProvider, VoiceRef

_TODO = "f5-tts integration: implement in P0/P2 (docs/02-repos/momo-models.md 'provider 与产物')"


class F5TTSProvider(TTSProvider):
    name = "f5"

    def __init__(self, settings: Settings) -> None:
        self._settings = settings

    def capabilities(self) -> TTSCaps:
        return TTSCaps(
            streaming=False,
            languages=("zh", "en"),
            clone=True,
            emotion=False,
        )

    async def warmup(self) -> None:
        raise NotImplementedError(_TODO)

    async def synth_stream(
        self, text: str, *, voice: VoiceRef, emotion: str | None
    ) -> AsyncIterator[bytes]:
        raise NotImplementedError(_TODO)
        yield  # pragma: no cover —— 保持 async generator 类型

    async def cancel(self, req_id: str) -> None:
        raise NotImplementedError(_TODO)
