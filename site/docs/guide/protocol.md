# momo-ws 协议 v1

进程间通信协议：**JSON 文本帧（控制面）+ 二进制音频帧（数据面），单连接复用**。LLM 推理不在本协议内——编排器对大脑走 OpenAI-compatible HTTP SSE。

<DiagramFrame src="turn-voice.html" :height="640" />

## 连接拓扑

| 链路 | 方向 | 用途 |
|---|---|---|
| UI 链路 | 前端 → 编排器 `/ws/ui` | 用户输入、字幕/表情、播放、状态广播 |
| action 链路 | action 方 → 编排器 `/ws/action` | 注册/执行 action（P4） |
| 模型链路 | 编排器 → ASR/TTS `/ws` | 音频上行、合成下发、取消 |

## 握手

建连后第一条消息必须是 `hello`（含 `protocol:"momo-ws"`、`version:"v1"`、`client` 角色、音频参数协商）；服务端回 `hello.ok`（`session_id`、`role`、`capabilities`）或 `error{VERSION_MISMATCH}` + close `4001`。

## 消息目录

| type | 方向 | 何时发 |
|---|---|---|
| `hello` / `hello.ok` | 双向 | 建连握手 |
| `input.text` | UI→orch | 用户提交文字 |
| `input.audio.segment` | ASR→orch → UI 镜像 | VAD 段转写完成 |
| `input.vad` | ASR→orch → UI 镜像 | speech_start/end 边界；Speaking 中即打断触发源 |
| `output.sentence` | orch→UI + TTS | LLM 每成形一句（含 emotion） |
| `audio.chunk` | 二进制双向 | 音频流 |
| `control.barge_in` | UI↔orch→TTS/ASR | 打断请求/通知/回报 |
| `control.cancel` | orch→TTS/ASR/action | 取消指定对象 |
| `state.turn` | orch→UI | 状态迁移广播 |
| `action.*` | action 方↔orch | 注册/调用/结果（P4） |
| `error` | 双向 | 协议/运行错误 |

## 二进制音频帧

固定 12B 头 + 载荷：`ver(1B) | stream(1B: 0x01上行/0x02下行) | flags(2B, bit0=EOS) | seq(4B) | tag(4B: 下行=句seq / 上行=VAD段号) | payload`。
默认 `pcm_s16le` 单声道 50ms/帧；上行 16kHz、下行 24kHz（hello 协商）。

## emotion tag

LLM 内联 `[happy]` 标注 → 编排器剥离写入 `output.sentence.emotion`。v1 固定 8 值：`neutral happy sad angry surprised thinking shy sleepy`。tag→Live2D 表情/TTS instruct 的映射是各端实现细节，不进协议。

## 背压与断线

- 下行音频**永不静默丢帧**：orch 缓冲 96 帧 ≈4.8s；溢出即 `BACKPRESSURE_OVERFLOW` + 取消回合。
- 上行丢旧帧（实时性优先），缓冲 2s；ASR 对 >30s 段强制 final。
- JSON 控制帧永不丢弃，消费不动即关连（close `4400`）。
- UI 断连→取消活动回合保留历史；重连走 hello 重同步，不重放音频。
- 重连退避：指数 0.5s→8s，±20% 抖动。

::: info 权威来源
[docs/01-architecture/protocol.md](https://github.com/Momoyeyu/momo/blob/master/docs/01-architecture/protocol.md) — 全部 JSON schema、close code、版本协商细则。
:::
