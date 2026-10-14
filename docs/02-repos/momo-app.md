# `app/`：用户产品运行时（未来的 `momo` 仓）

> app 是终端用户使用的产品本体：TypeScript 呈现角色，Rust 编排完整回合，provider 全部由用户配置（单槽位，ADR-028）。当前交付形态是 `momod` + Web UI；Tauri 桌面壳后置复刻（ADR-011）。

## 模块结构

```text
app/
├── web/src/                  # 当前唯一交付形态：Web UI
│   ├── avatar/               # AvatarAdapter + Live2D manifest/load/dispose（模型可替换）
│   ├── presentation/         # PresentationDirector + MotionDirector
│   ├── audio/                # AudioPlayer/AudioWorklet capture/playback/ACK
│   ├── shell/                # CompanionShell web-facing adapter
│   └── chat/                 # overlay、文字/PTT/VAD、历史/设置
├── src-tauri/                # Tauri 桌面壳（窗口/tray/momod 生命周期；desktop.md「当前实现状态」）
└── src/
    ├── session/              # session actor
    ├── turn/                 # TurnCoordinator、取消、playback ledger
    ├── protocol/             # momo-ws 消息与音频 codec
    ├── repository/           # transcript/correction/persona/memory/index
    └── providers/            # LLM SSE、ASR/TTS/Embedding clients（用户单槽位配置）
```

## 所有权

```mermaid
flowchart LR
    SHELL["CompanionShell<br/>Web（Tauri 后置）"] --> UI["TS / Pixi presentation"]
    UI <--> AUDIO["AudioPlayer / Worklet"]
    AUDIO -->|"实际 playback"| PRESENT["PresentationDirector"]
    PRESENT -->|"avatar params"| UI
    UI <-->|"momo-ws"| ACTOR["Rust session actor"]
    ACTOR --> TURN["TurnCoordinator"]
    TURN -->|"domain events"| UI
    TURN --> REPO["local conversation repository"]
    REPO --> EMBED["Python EmbeddingProvider"]
    TURN --> LLM["OpenAI SSE"]
    TURN <--> MODELS["Python ASR / TTS adapters"]
```

- `TurnCoordinator` 唯一拥有 reply turn；capture 独立。
- `PresentationDirector` 位于 TS presentation，只消费 domain/audio events。
- Rust repository 拥有 session、correction、persona、memory 正文与 vector index；Python 只计算 embedding。
- `AudioPlayer` 以实际播放生成 started/progress ACK，Stop 同步清 buffer 与嘴型。
- provider 配置是**用户面**：每类能力一个 endpoint（LLM OpenAI-compatible / ASR / TTS），可在设置里更换；app 不做多 provider 容灾编排（ADR-028）。
- Tauri 壳（`src-tauri/`）attach-or-spawn momod：momod 已在跑则 attach，否则按 `MOMO_MOMOD_BIN` → exe 同级 → `PATH` 解析 spawn 为 owned child（desktop.md「当前实现状态」）。
- Tauri 能力失败时 `CompanionShell` 使用普通可见窗口；menu 不可用时禁用 click-through。
- 退出只终止 app 自己启动的 owned child，attached remote provider 只断开连接。

## ws 子系统

每条 `/ws/ui` 连接是一个独立 actor，tokio 任务拓扑：

```mermaid
flowchart LR
    SOCK["WebSocket"] --> RX["conn_rx 循环<br/>（conn task 内）"]
    RX -->|"decode → Msg"| SESS["Session actor<br/>有界 mpsc（64）"]
    SESS -->|"OutboundFrame"| Q["conn_tx 有界 mpsc（256）"]
    RX -->|"decode 失败 → error 帧"| Q
    Q --> TX["conn_tx task<br/>encode → 写帧"]
    TX --> SOCK
```

- 出站统一走 `OutboundFrame`（Json / Audio / Close）经 `conn_tx` 有界 mpsc：单队列天然保序，背压 = 生产者 `send().await` 阻塞，**不丢帧、不换序**。
- JSON 控制帧永不丢弃；对端消费不动时 `conn_tx` 每帧发送有 stall deadline，超时直接关连（protocol「控制消息不得静默丢弃」）。
- 解码异常（JSON 非法、未知 `type`、非法音频帧头）一律回 `error` 帧（non-fatal `BAD_MESSAGE`），连接保持；握手期/不可恢复异常回 `error` + 按 close code 表断连。
- `conn_rx` 只 decode 与转发，业务判断全在 Session actor；上行音频帧解码出头后经 stream 归属检查交给 ASR uplink task（扩展能力）。
- 握手（protocol §认证）：upgrade 前 Origin ∈ {`tauri://localhost`、`http://tauri.localhost`、`https://tauri.localhost`、`MOMO_WEB_ORIGIN`}，missing/未批准 → 403；upgrade 后 5s 内首帧必须是 `client='ui'` 且版本匹配的 hello，`auth_token` 与 `MOMO_UI_TOKEN` 恒时比对（未配置=standalone dev 不比对）；违规按 4400/4401/4001 断连。
- 单活跃 UI 连接：`SessionManager::attach` 让新连接顶掉旧 session，旧连接收 close 1000 关连；session restore 走 `resume_session_id` + snapshot（protocol §认证）。WS 断开后 session actor 进入 `MOMO_SESSION_RESUME_TTL_MS`（默认 60s）保活窗口：回合/uplink 即时清理、history 与 receipt cache 保留；TTL 内 resume 命中则换绑复用并下发 `session.snapshot`，超时 actor 退出回收。

## 验收指针

见 [Web 交互闭环](../03-features/pipeline.md)、[desktop](../01-architecture/desktop.md)（后置 Tauri 壳）、[runtime](../01-architecture/runtime.md) 与 [protocol](../01-architecture/protocol.md)。
