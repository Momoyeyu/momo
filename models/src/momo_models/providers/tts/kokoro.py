"""Kokoro-82M（已实测兜底）：首包 ~209ms、RTF 0.023、VRAM 0.76GB @ 3070 Laptop。

`kokoro` pip 包 + torch（版本须与 torchaudio 对齐，ADR-018）；g2p 依赖
espeak-ng/misaki，按语言懒加载。
"""

from collections.abc import AsyncIterator

from momo_models.config import Settings
from momo_models.providers.base import TTSCaps, TTSProvider, VoiceRef

_TODO = "kokoro integration: implement in P0 (docs/02-repos/momo-models.md 'provider 与产物')"


class KokoroTTSProvider(TTSProvider):
    name = "kokoro"

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
