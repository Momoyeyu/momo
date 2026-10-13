# 总体架构

> 桌面呈现负责在场，回合编排负责接话，模型服务提供理解与表达能力。三层通过清晰的事件与音频边界协作。

## 运行时组件

```mermaid
flowchart TB
    DESK["桌面呈现 · app/TS + Tauri<br/>Live2D / 输入 / 字幕 / 音频播放"]
    TURN["回合编排 · app/Rust<br/>Session actor + TurnCoordinator"]
    ASR["听懂 · models/Python<br/>ASR + VAD"]
    LLM["回答 · 本地或远端 LLM<br/>OpenAI-compatible SSE"]
    TTS["说出 · models/Python<br/>TTS adapter"]
    DATA[("本地记录<br/>transcript / persona<br/>memory + index")]
    DESK <-->|"momo-ws：输入 / 状态 / 音频"| TURN
    TURN <-->|"语音 / 转写"| ASR
    TURN <-->|"流式文本"| LLM
    TURN <-->|"句子 / 音频"| TTS
    TURN -->|"确认播出的内容"| DATA
```

`app/src-tauri` 承载桌面产品，浏览器用于网站与开发预览。infra 是独立控制面，只负责启停、探测和产物管理，不承载 conversation 数据；详见 [模块边界](./boundaries.md) 与 [infra 控制面](../02-repos/momo-infra.md)。

## 呈现的内部协作

```mermaid
flowchart TB
    EVENTS["回合事件"] --> DIRECTOR["TS PresentationDirector"]
    PLAYER["AudioPlayer<br/>实际 playback / RMS"] --> DIRECTOR
    POINTER["鼠标 / 点击"] --> DIRECTOR
    DIRECTOR --> MOTION["MotionDirector<br/>参数优先级"] --> AVATAR["AvatarAdapter<br/>Live2D/Pixi"]
```

## 自有边界与上游复用

应用拥有 `AvatarAdapter`、`MotionDirector`、`AudioPlayer`、`CompanionShell`、`TurnCoordinator` 与本地 conversation repository。Live2D/Pixi/ASR/TTS/LLM 推理优先复用上游库和许可清晰的模型；Runtime Core、sample model 与应用代码分别适用各自许可。

| 层 | 拥有 | 不拥有 |
|---|---|---|
| TS presentation | avatar 适配、`PresentationDirector`、动作仲裁、音频 IO、用户操作 | 回合真相、模型推理 |
| Rust runtime | session actor、turn/取消/历史、provider 激活、persona 与 memory 正文及 vector index | Live2D 参数细节、推理实现 |
| Python models | ASR/TTS/embedding inference adapter、训练/评估入口 | memory 决策、正文/index、turn 状态、桌面业务 |
| infra | 启停、诊断、模型/训练/配置运维 | 对话文本/音频数据通路 |

## readiness 与可用能力

shell/settings、avatar 与 backend 并行准备，各自决定对应能力。

```mermaid
flowchart TB
    SHELL["创建 shell + settings"] --> AVATAR{"avatar ready？"}
    AVATAR -->|"是"| CHARACTER["显示互动形象"]
    AVATAR -->|"否"| AVATARRETRY["形象错误与重试<br/>聊天 / 设置仍可用"]
```

```mermaid
flowchart TB
    BACKEND["连接 backend"] --> LLM{"LLM ready？"}
    LLM -->|"是"| TEXT["文字 reply 可用"]
    LLM -->|"否"| DRAFT["保留 draft / 设置 / idle avatar<br/>不可生成 AI reply"]
```

```mermaid
flowchart TB
    VOICE["探测语音 provider"] --> ASR{"ASR ready？"}
    ASR -->|"否"| NOTRANSCRIBE["禁用语音输入"]
    ASR -->|"是"| INPUT["PTT / VAD 可用"]
    VOICE --> TTS{"TTS ready？"}
    TTS -->|"否"| TEXTONLY["回复以文字完成"]
    TTS -->|"是"| SPOKEN["有声回复可用"]
```

ASR/TTS 不就绪不阻止 LLM 文字回复；LLM 不就绪时只保留 draft、设置与 idle avatar。

## normal 与 degraded

normal/degraded 使用同一代码与协议；差异是本机拥有的 child process 与连接的 provider。应用退出只终止自己启动并记录为 owned 的 child process，绝不关闭 attached remote provider。provider 切换先 warmup/probe，成功后原子激活；活动回合存在时先显式取消。

## 核心不变量

1. `TurnCoordinator` 是 turn 的唯一所有者；persona/连接、turn、animation 三类状态彼此独立。
2. `Speaking` 只由 `playback.started` 进入，不由文本生成推导。
3. Stop 同步停本地播放与归零嘴型；actor 同时取消 LLM/TTS/句切/音频，不等待播放 ACK。
4. 旧 turn 的帧按 `stream_id` + turn/sentence 绑定丢弃，不能进入新 turn。
5. infra 永远在 turn 数据路径外；静态网站永远不连接本地 API。
6. 产品默认中文，并在 provider 声明能力时支持中、英、日输入与输出。

## 操作入口

- [桌面宿主与窗口生命周期](./desktop.md)
- [运行时与状态机](./runtime.md)
- [通信协议](./protocol.md)
- [用户操作路径图谱](./workflows.md)
- [模块边界](./boundaries.md)
