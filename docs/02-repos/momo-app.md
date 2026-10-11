# `app/`：桌面 companion 与运行时

> P0 目标包含 Tauri 2 桌面宿主、TS/Pixi 7 presentation 与 Rust tokio+axum session actor；当前实现仍是 v1 scaffold。

## 目标结构

```text
app/
├── src-tauri/                 # P0 规划：窗口/menu/权限/生命周期/sidecar
├── web/src/
│   ├── avatar/               # AvatarAdapter + Live2D manifest/load/dispose
│   ├── presentation/         # PresentationDirector + MotionDirector
│   ├── audio/                # AudioPlayer/AudioWorklet capture/playback/ACK
│   ├── shell/                # CompanionShell web-facing adapter
│   └── chat/                 # overlay、文字/PTT/VAD、历史/设置
└── src/
    ├── session/              # session actor
    ├── turn/                 # TurnCoordinator、取消、playback ledger
    ├── protocol/             # v2 target codec；current v1 scaffold
    ├── repository/           # transcript/correction/persona/memory/index
    └── providers/            # LLM SSE、ASR/TTS/Embedding clients
```

## 所有权

```mermaid
flowchart LR
    SHELL["CompanionShell<br/>Tauri"] --> UI["TS / Pixi presentation"]
    UI <--> AUDIO["AudioPlayer / Worklet"]
    AUDIO -->|"实际 playback"| PRESENT["PresentationDirector"]
    PRESENT -->|"avatar params"| UI
    UI <-->|"momo-ws v2 target"| ACTOR["Rust session actor"]
    ACTOR --> TURN["TurnCoordinator"]
    TURN -->|"domain events"| UI
    TURN --> REPO["local conversation repository"]
    REPO --> EMBED["Python EmbeddingAdapter"]
    TURN --> LLM["OpenAI SSE"]
    TURN <--> MODELS["Python ASR / TTS adapters"]
```

- `TurnCoordinator` 唯一拥有 reply turn；capture 独立。
- `PresentationDirector` 位于 TS presentation，只消费 domain/audio events。
- Rust repository 拥有 session、correction、persona、memory 正文与 vector index；Python 只计算 embedding。
- `AudioPlayer` 以实际播放生成 started/progress ACK，Stop 同步清 buffer 与嘴型。
- Tauri 能力失败时 `CompanionShell` 使用普通可见窗口；menu 不可用时禁用 click-through。
- 退出只终止 app 自己启动的 owned child，attached remote provider 只断开连接。

## P0 验收指针

见 [P0 桌面 MVP](../03-phases/p0-pipeline.md)、[desktop](../01-architecture/desktop.md)、[runtime](../01-architecture/runtime.md) 与 [protocol](../01-architecture/protocol.md)。
