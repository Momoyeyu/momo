"""Fun-CosyVoice3-0.5B（首选目标）：instruct 控情感，官方标称双向流式 150ms。

接入：CosyVoice 官方推理栈（不发 PyPI，源码部署）。依赖重：pynini 在
py3.12/WSL 历史经验需折腾；显存需求待实测，预留 kokoro 兜底。
见 momo-docs/02-repos/momo-models.md 后端表。
"""

from collections.abc import AsyncIterator

from momo_models.config import Settings
from momo_models.providers.base import TTSCaps, TTSProvider, VoiceRef

_TODO = "cosyvoice3 接入：P2 实现（官方推理栈部署需求待实测）"


class CosyVoice3Provider(TTSProvider):
    name = "cosyvoice3"

    def __init__(self, settings: Settings) -> None:
        self._settings = settings

    def capabilities(self) -> TTSCaps:
        return TTSCaps(
            streaming=True,
            languages=("zh", "en", "ja", "ko", "yue"),
            clone=True,
            emotion=True,
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
