#!/usr/bin/env python3
"""TTS 盲听 A/B：批量合成 → 打分表 → 汇总（P2 实现，当前 stub）。

口径见 momo-docs/02-repos/momo-models.md「评测脚本」：
给定文本集各候选 backend 合成，汇总"像不像/稳不稳/情感范围"。
链路延迟不在此测（归 momo-infra `momoctl bench`）。
"""


def main() -> None:
    raise SystemExit("tts_ab_test: P2 实现（见 momo-docs/02-repos/momo-models.md）")


if __name__ == "__main__":
    main()
