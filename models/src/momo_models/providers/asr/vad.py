"""silero-vad 断句封装：只做断句不做识别。

产出 input.vad{speech_start|speech_end} 边界事件；判停静音时长
MOMO_VAD_SILENCE_MS（默认 250）；>30s 段强制 final（协议 §7）。
接入：onnx via sherpa-onnx 或 `silero-vad` 包。
"""

from collections.abc import Iterator
from dataclasses import dataclass

_TODO = "silero-vad 接入：P0 实现"


@dataclass(frozen=True)
class VadEvent:
    """VAD 边界事件（对应协议 input.vad）。"""

    event: str  # speech_start | speech_end
    at_ms: int
    segment_index: int


class SileroVad:
    """16kHz mono PCM 流 → VAD 边界事件 + 段切分。"""

    def __init__(self, silence_ms: int = 250, sample_rate: int = 16000) -> None:
        self.silence_ms = silence_ms
        self.sample_rate = sample_rate

    def load(self) -> None:
        raise NotImplementedError(_TODO)

    def feed(self, pcm: bytes) -> Iterator[VadEvent]:
        """喂入一个 audio.chunk（50ms/帧），产出 0..n 个边界事件。"""
        raise NotImplementedError(_TODO)
        yield  # pragma: no cover —— 保持 generator 类型
