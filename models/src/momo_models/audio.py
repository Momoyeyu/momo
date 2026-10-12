"""音频工具：PCM s16le ↔ numpy、wav 封装、重采样。

momo-ws v2 约定（protocol.md §3/§4）：
- 上行（麦→ASR）：pcm_s16le，16kHz mono，50ms/帧
- 下行（TTS→播放）：pcm_s16le，24kHz mono，50ms/帧
"""

import io
import wave

import numpy as np

UPLINK_SAMPLE_RATE = 16000
DOWNLINK_SAMPLE_RATE = 24000


def s16le_bytes_to_numpy(pcm: bytes) -> np.ndarray:
    """小端 int16 PCM → float32 [-1.0, 1.0]。"""
    return np.frombuffer(pcm, dtype="<i2").astype(np.float32) / 32768.0


def numpy_to_s16le_bytes(samples: np.ndarray) -> bytes:
    """float32 [-1.0, 1.0] → 小端 int16 PCM（截断到合法区间）。"""
    clipped = np.clip(samples, -1.0, 1.0)
    return (clipped * 32767.0).astype("<i2").tobytes()


def wav_wrap(pcm_s16le: bytes, sample_rate: int, channels: int = 1) -> bytes:
    """裸 PCM s16le 包一层 RIFF/WAVE 头，供调试口整段返回。"""
    buf = io.BytesIO()
    with wave.open(buf, "wb") as w:
        w.setnchannels(channels)
        w.setsampwidth(2)
        w.setframerate(sample_rate)
        w.writeframes(pcm_s16le)
    return buf.getvalue()


def wav_unwrap(data: bytes) -> tuple[bytes, int, int]:
    """WAVE → (pcm_bytes, sample_rate, channels)。非 PCM16 编码由调用方拒绝。"""
    with wave.open(io.BytesIO(data), "rb") as w:
        return w.readframes(w.getnframes()), w.getframerate(), w.getnchannels()


def resample(samples: np.ndarray, src_rate: int, dst_rate: int) -> np.ndarray:
    """线性插值重采样（骨架实现；P0 如需更高质量再换 soxr/polyphase）。

    用途对齐文档：24k（TTS 下行）↔ 16k（ASR 上行）等档位转换。
    """
    if src_rate == dst_rate:
        return samples
    duration = len(samples) / src_rate
    out_len = max(1, round(duration * dst_rate))
    x_old = np.linspace(0.0, duration, num=len(samples), endpoint=False)
    x_new = np.linspace(0.0, duration, num=out_len, endpoint=False)
    return np.interp(x_new, x_old, samples).astype(np.float32)
