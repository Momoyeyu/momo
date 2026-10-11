"""SenseVoice-Small via sherpa-onnx（纯 CPU）：实测 RTF 0.040，CPU×4。

zh/en/ja/ko/yue + 自动语言 + 情感/事件标记。注意 onnx 与 tokens.txt 配套；
模型路径 = $MOMO_MODELS_DIR/sherpa-onnx/sense-voice-*/。见 momo-models.md 后端表。
"""

from momo_models.config import Settings
from momo_models.providers.base import ASRProvider, Transcript

_TODO = "sensevoice 接入：P0 实现（sherpa-onnx 轮）"


class SenseVoiceASRProvider(ASRProvider):
    name = "sensevoice"

    def __init__(self, settings: Settings) -> None:
        self._settings = settings
        self.model = settings.asr_model
        self.threads = settings.asr_threads

    async def load(self) -> None:
        raise NotImplementedError(_TODO)

    async def transcribe(self, pcm: bytes, *, lang: str | None) -> Transcript:
        raise NotImplementedError(_TODO)
