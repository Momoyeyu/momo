# 项目介绍

momo 是一个本地可跑的 AI 伙伴：有脸（Live2D）、有声（克隆声线）、有人格（character card → QLoRA）、能被打断、会自己找话说。**首要目标不是直播，而是本地把对话体验跑好。**

## 设计原则

- **三段式进程分离**：交互端（渲染+音频 IO）— 编排器（回合/打断/路由）— 模型服务（ASR/TTS/LLM），互不越界。
- **协议先行**：进程间走 `momo-ws v1`（JSON 控制帧 + 二进制音频帧单连接复用）；LLM 独立走 OpenAI-compatible HTTP。
- **文档先行**：实现与 [`momo-docs`](https://github.com/Momoyeyu/momo-docs) 不一致视为 bug，先改文档再改代码。
- **本地优先，可降级**：正常档跨三机（Mac Air + laptop + mac-studio），降级档 laptop 单机全栈——**只改 env，不改代码**。
- **实测说话**：性能数字一律标注「实测+日期」或「预估/待实测」。

## 技术栈速览

| 层 | 仓库 | 技术 |
|---|---|---|
| 交互端 | momo-app | TypeScript · Vite · Pixi.js + pixi-live2d-display |
| 编排器 | momo-app | Rust · tokio · axum 0.8 · `momod` |
| 模型服务 | momo-models | Python ≥3.12 · FastAPI · uv · SenseVoice / CosyVoice3 / Kokoro |
| 运维 | momo-infra | Rust · clap · `momoctl` CLI + Web 控制台 |
| 大脑 | 外部 | mlx_lm.server / vLLM / DeepSeek API（OpenAI-compat） |

## 读图顺序

1. [总体架构](/guide/architecture) — 主图，先建立全局
2. [momo-ws 协议](/guide/protocol) — 进程间怎么说话
3. [回合状态机](/guide/runtime) + [打断](/guide/barge-in) — 运行时核心机制
4. [部署拓扑](/guide/deployment) — 跑在哪些机器上
5. [声线克隆](/guide/voice-clone) / [人格微调](/guide/persona) — 两条训练流水线
6. [训练 job 与控制台](/guide/infra-console) — 怎么管理这一切

::: info 权威来源
本站是 [`momo-docs`](https://github.com/Momoyeyu/momo-docs) 的可视化呈现。冲突时以仓库文档为准。
:::
