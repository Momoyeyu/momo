#!/usr/bin/env python3
"""参考音频 → 训练集：切分/去噪/响度归一/标注（P2 实现，当前 stub）。

口径（docs/02-repos/momo-models.md「训练边界」）：
- 零样本只需 10-30s 干净参考音；SFT 目标 10-60min。
- 产出：单人声、无 BGM、响度归一、24k mono wav + 文本标注。

用法（实装后）：
    python training/voice/prepare_dataset.py --input raw/ --output dataset/
"""

import argparse


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", required=True, help="原始素材目录")
    parser.add_argument("--output", required=True, help="训练集输出目录")
    parser.parse_args()
    raise SystemExit("prepare_dataset: P2 实现（见 momo-docs/02-repos/momo-models.md）")


if __name__ == "__main__":
    main()
