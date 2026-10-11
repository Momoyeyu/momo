# 声线克隆轨道

目标：momo 拥有一条稳定、低延迟、有辨识度的克隆声线。**不依赖代码里程碑，素材收集本周即可开始。**

<DiagramFrame src="voice-clone.html" :height="660" />

## 素材分级与质量门槛

| 级别 | 时长 | 用途 |
|---|---|---|
| T0 | 30s-2min | 零样本试听下限 |
| T1 | 2-10min | 优选零样本 / 多参考音轮换 |
| T2 | 10-60min | SFT 级（CosyVoice3 / GPT-SoVITS） |

每一级都必须：单人声、无 BGM/音效、低底噪、WAV 单声道 ≥24kHz、约 -20~-16 LUFS、覆盖平静/开心/抱怨三类语气。**只克隆有权使用的声音**，授权记录写入 `voice-materials/manifest.md`。

## 基座选择

| | Fun-CosyVoice3-0.5B（首选） | GPT-SoVITS（备选） | IndexTTS-2（评估项） |
|---|---|---|---|
| 语言 | 9 语言含中日英 | 中英日韩粤 | 中英为主 |
| 流式 | 双向流式，标称首包 150ms | 分块输出 | 待实测 |
| 情感 | instruct 文本指令 | 参考音迁移+文本 | 情感向量 |
| 生态 | 官方训练脚本 | 微调工具链最成熟 | 训练链待评估 |

选择原则：**盲听说话**——质量并列时按 流式 > 情感控制 > 依赖风险 > 训练支持 排序。

## 评估协议

- 固定 3 段参考音（平静/开心/抱怨）+ 固定 15 句测试集（含 code-switch 混句）
- 盲听 A/B：≥2 人、样本乱序去标签、4 维打分（相似度×2 + 自然度 + 情感 + 稳定性）
- 达标线：**加权分 ≥3.5 且无任一句 ≤2**

## SFT 路径（零样本不达标时）

VAD 切片 3-10s → 清洗 → SenseVoice 标注 + 人工抽检 ≥10% → 4090D 单卡 ≤24h（checkpoint 每 2h）→ **结束前 ≥1h rsync 回传** → 盲听复测 → `MOMO_TTS_MODEL_DIR` env 切换灰度上线，N-1 版本保留可回滚。

::: info 权威来源
[momo-docs/04-tracks/voice-clone.md](https://github.com/Momoyeyu/momo-docs/blob/master/04-tracks/voice-clone.md) — 完整打分表模板、回传路径、风险对策表。
:::
