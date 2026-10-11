# 路线图与里程碑

<SvgDiagram src="roadmap.svg" title="开发路线图" />

两条并行轨道不与里程碑绑定：[声线克隆](/guide/voice-clone)（素材收集本周可开始）、[人格微调](/guide/persona)（P3 起认真做）。

## 阶段验收口径

| 里程碑 | 目标 | 验收 |
|---|---|---|
| P0 | 管道骨架 | 打字 → LLM → edge-tts 出声，协议握手全通 |
| P1 | 有脸 | Live2D 渲染、口型驱动、字幕气泡、Tauri 壳预留 |
| P2 | 对话 | 语音输入、barge-in 可用、本地 TTS（Kokoro）上线 |
| P3 | 灵魂 | 记忆检索、自发开口、克隆声线（零样本或 SFT）为默认 |
| P4 | 能力 | action 总线跑通 1 个游戏/工具、视觉输入 |
| P5 | 直播 | OBS/弹幕 —— **暂缓**，本地体验优先 |

## 需求管理

[GitHub Project · 任务总览](https://github.com/users/Momoyeyu/projects/2)：单视图，列 = Title / Assignees / Status / Repository / Priority / Start / Due / 内容类型。

- **优先级三级**：P0 当前阶段必做 / P1 近阶段应做 / P2 远期可选（与 Phase 独立）
- **粒度**：每条 issue ≈0.5-2 天工作量，做完即关；粗目标留在 milestone 描述
- **字段纪律**：每张卡必须有 Priority + Start + Due + Assignee + 内容类型

## 风险备忘（摘要）

- mac-studio 随时收回 → laptop 单机全栈兜底（只改 env）
- 克隆声线授权链 → P5 直播前必须确认
- HF 下载 → laptop 必须 `HF_ENDPOINT=https://hf-mirror.com`
- torch/torchaudio 版本严格对齐（已踩坑）

::: info 权威来源
[docs/00-context/roadmap.md](https://github.com/Momoyeyu/momo/blob/master/docs/00-context/roadmap.md) · [03-phases/](https://github.com/Momoyeyu/momo/tree/master/docs/03-phases) 各阶段完整任务分解。
:::
