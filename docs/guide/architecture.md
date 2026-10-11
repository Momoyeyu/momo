# 总体架构

三段式：**交互端**（渲染 + 音频 IO）— **编排器**（回合/打断/路由/状态机）— **模型服务**（LLM/ASR/TTS）。进程间走 `momo-ws v1` 与 OpenAI-compatible HTTP；**运维面**（momoctl + Web 控制台）在旁路，不在数据通路上。

<DiagramFrame src="system-main.html" :height="720" />

## 组件职责

| 组件 | 仓库 | 语言 | 职责 | 对外接口 |
|---|---|---|---|---|
| 交互端 | momo-app | TS | Live2D 渲染、表情/口型、麦采集、播放、文字输入 | WS 客户端 → `/ws/ui` |
| 编排器 momod | momo-app | Rust | WS 服务、回合状态机、barge-in、句切、历史、路由 | 监听 `:7800` |
| ASR 服务 | momo-models | Python | silero-vad 分段 + SenseVoice 转写（纯 CPU，实测 RTF 0.040） | WS 服务 `:7810` |
| TTS 服务 | momo-models | Python | 逐句合成 + 情感参数，audio.chunk 回流 | WS 服务 `:7811` |
| 大脑 | 外部 | — | OpenAI-compatible 流式 chat | HTTP `:7820` / 远端 |
| action 提供方 | 第三方 | 任意 | 注册/执行 action（P4） | WS 客户端 → `/ws/action` |
| momoctl | momo-infra | Rust | doctor、模型管理、编排启停、训练 job、Web 控制台 | env / CLI / REST |

## 数据流

**语音回合（主路径）**——详见[协议时序图](/guide/protocol)：

1. 前端麦 PCM → 二进制 `audio.chunk` → 编排器 → 转发 ASR
2. ASR：silero-vad 分段 + SenseVoice 转写 → `input.audio.segment`（含 text/lang）回编排器
3. 编排器建回合、组装历史+人设 → OpenAI SSE 调大脑
4. token 流 → 句切 + 剥 `[emotion]` tag → `output.sentence` 双发（前端字幕/表情 + TTS）
5. TTS 逐句合成 → `audio.chunk` → 前端边下边播
6. 中途用户开口 → `control.barge_in` 三动作（见[打断](/guide/barge-in)）

**训练/产物通路（离线）**：4090D ≤24h 训练 → 产物回传 → 登记 MANIFEST → env 切换激活。不进入运行时数据流。

## 关键决策（摘自 ADR）

- **LLM 不进 momo-ws**（ADR-006）：流式取消=直接 abort HTTP；模型服务换实现不动协议。
- **Rust 编排器**（ADR-002）：打断/取消/背压是并发问题，tokio CancellationToken 语义天然匹配。
- **推理不重写**：Python 调用现有推理库；Rust 只管编排与运维，不碰模型推理。
- **降级只改 env**（ADR-008）：同一套代码跑正常档/降级档。

::: tip 完整版本
[momo-docs/01-architecture/system.md](https://github.com/Momoyeyu/momo-docs/blob/master/01-architecture/system.md) · [boundaries.md](https://github.com/Momoyeyu/momo-docs/blob/master/01-architecture/boundaries.md) · [decisions.md](https://github.com/Momoyeyu/momo-docs/blob/master/00-context/decisions.md)（25 条 ADR）
:::
