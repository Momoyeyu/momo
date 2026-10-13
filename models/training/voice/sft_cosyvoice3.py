#!/usr/bin/env python3
"""CosyVoice3 SFT 驱动脚本（P2 实现，当前 stub）。

口径（momo-docs/02-repos/momo-models.md）：
- 包官方训练脚本调用，4090D 单卡 24h 预算内出 checkpoint。
- 产物拷回 $MOMO_MODELS_DIR/tts/ft-cosyvoice3-<date>/ 并追加 MANIFEST 条目。

用法（实装后）：
    python training/voice/sft_cosyvoice3.py --dataset dataset/ --run ft-YYYYMMDD
"""


def main() -> None:
    raise SystemExit("sft_cosyvoice3: implemented in P2 (see momo-docs/02-repos/momo-models.md)")


if __name__ == "__main__":
    main()
