# 回合状态机

momod（Rust 编排器）的运行时核心：五态状态机 + 取消令牌 + 句切 + 队列背压 + 延迟预算。

<DiagramFrame src="turn-states.html" :height="560" />

## 各状态进入动作

| 状态 | 进入动作 |
|---|---|
| `Idle` | 重置 idle 计时器；广播 `state.turn{idle}` |
| `Listening` | 绑定新 `turn_id`；ASR 接收音频；广播 |
| `Thinking` | 建取消令牌；组装历史+人设 prompt → 发起 LLM 流 |
| `Speaking` | 句切器消费 token 流 → `output.sentence` 双发（UI+TTS） |
| `Interrupted` | 执行 [barge-in 三动作](/guide/barge-in) |

**自发开口**：`Idle` 持续 `MOMO_IDLE_TIMEOUT_S`（默认 20s，0=关闭）→ 注入情境直接进 `Thinking`。连续自发最多 1 次防刷屏。

## 句切规则（按序应用）

1. **强切**：`。！？!?；;…` 或换行立即成句；ASCII `.`/`!`/`?` 仅后随空白/流结束才切（避免 `Mr.`、`3.14` 误切）
2. **弱切**：`,，、` + 缓冲 ≥25 字（`MOMO_SENT_SOFT_CHARS`）——防长句饿死首包
3. **硬切**：60 字（`MOMO_SENT_MAX_CHARS`）无标点强制成句
4. **tag 剥离**：`[xxx]` 先移除再计长；表外值归一 `neutral`
5. **收尾**：流结束 flush `final:true`；空回复直接回 `Idle`
6. **语言**：每句主导语言检测写 `lang` 提示 TTS；v1 不按语言拆句

## 队列与背压

| 队列 | 容量 | 溢出策略 |
|---|---|---|
| mic 上行（orch→ASR） | 2s 音频 | 丢旧帧 + 日志 |
| `sentence_q` | 8 句 | 暂停读 LLM 流（反压） |
| TTS 在途 | 1 句并发 | 逐句串行保序 |
| 下行音频 | 96 帧 ≈4.8s | 溢出→`BACKPRESSURE_OVERFLOW`+取消回合 |
| VAD 段上限 | 30s | ASR 侧强制 final |

## 延迟预算（目标 <1.5s 出声）

| 环节 | 语音回合 | 依据 |
|---|---|---|
| VAD 判停 | 250 ms | silero `silence_ms` |
| ASR 转写 | ~150 ms（3s 语音） | **实测** RTF 0.040 |
| 编排开销 | 20 ms | 目标值 |
| LLM 首 token | 300 ms | **预估**；30B 实测 121 t/s |
| 首句成形 | 200 ms | ~25 tok 推算 |
| TTS 首 chunk | 250 ms | Kokoro **实测** 209 ms |
| 网络+jitter | 100 ms | LAN |
| **合计** | **~1.3 s** | 余量 ~200ms |

::: info 权威来源
[momo-docs/01-architecture/runtime.md](https://github.com/Momoyeyu/momo-docs/blob/master/01-architecture/runtime.md) — 取消令牌树、模块布局、完整预算表。
:::
