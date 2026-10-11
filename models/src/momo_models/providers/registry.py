"""名称 → provider 类映射；env 选择。

- TTS：`MOMO_TTS_PROVIDER=edge|kokoro|f5|cosyvoice3|sovits`
- ASR：固定 sensevoice 线（`MOMO_ASR_MODEL` 只选模型，不进 registry）
- embed：单实现（P3），无需 registry env
- LLM：无 registry —— 无网关后上游就是 `MOMO_LLM_ENDPOINT`，归 app 侧
"""

from momo_models.config import Settings
from momo_models.providers.asr.sensevoice import SenseVoiceASRProvider
from momo_models.providers.base import ASRProvider, TTSProvider
from momo_models.providers.tts.cosyvoice3 import CosyVoice3Provider
from momo_models.providers.tts.edge import EdgeTTSProvider
from momo_models.providers.tts.f5 import F5TTSProvider
from momo_models.providers.tts.kokoro import KokoroTTSProvider
from momo_models.providers.tts.sovits import SoVITSProvider

TTS_PROVIDERS: dict[str, type[TTSProvider]] = {
    "edge": EdgeTTSProvider,
    "kokoro": KokoroTTSProvider,
    "f5": F5TTSProvider,
    "cosyvoice3": CosyVoice3Provider,
    "sovits": SoVITSProvider,
}


def tts_provider_names() -> list[str]:
    return sorted(TTS_PROVIDERS)


def get_tts_provider_class(name: str) -> type[TTSProvider]:
    try:
        return TTS_PROVIDERS[name]
    except KeyError:
        raise ValueError(
            f"unknown MOMO_TTS_PROVIDER={name!r}; valid: {tts_provider_names()}"
        ) from None


def create_tts_provider(settings: Settings) -> TTSProvider:
    """按 `settings.tts_provider`（MOMO_TTS_PROVIDER）实例化后端。"""
    cls = get_tts_provider_class(settings.tts_provider)
    return cls(settings)


def create_asr_provider(settings: Settings) -> ASRProvider:
    """ASR 固定 sensevoice 线；`settings.asr_model`（MOMO_ASR_MODEL）选模型。"""
    return SenseVoiceASRProvider(settings)
