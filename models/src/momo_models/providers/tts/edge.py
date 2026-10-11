"""edge-tts（P0 过渡）：实测首包 ~1937ms，依赖外网，仅作过渡。

接入：`edge-tts` pip 包。见 momo-docs/02-repos/momo-models.md「各后端接入方式与已知坑」。
"""

from collections.abc import AsyncIterator

from momo_models.config import Settings
from momo_models.providers.base import TTSCaps, TTSProvider, VoiceRef

_TODO = "edge-tts 接入：P0 实现（momo-models.md 后端表）"


class EdgeTTSProvider(TTSProvider):
    name = "edge"

    def __init__(self, settings: Settings) -> None:
        self._settings = settings

    def capabilities(self) -> TTSCaps:
        return TTSCaps(
            streaming=True,
            languages=("zh", "en", "ja"),
            clone=False,
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
