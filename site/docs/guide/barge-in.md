# 打断 (barge-in)

真打断是「像人」的关键——momo 说话中途你开口，她立刻停。**三动作，顺序固定、幂等执行。**

<SvgDiagram src="barge-in.svg" title="barge-in 打断" />

## 触发源

- `vad_speech`：`Speaking`/`Thinking` 中 ASR 上报 `input.vad{speech_start}`，防抖 ≥`MOMO_BARGEIN_MIN_MS`（250ms）
- `user`：手动插话键 / PTT
- `system`：系统原因

## 三动作

1. **停播**：广播 `control.barge_in{applied:true}` → UI 清播放缓冲并回报 `played_seq`；orch 丢弃下行未发送部分
2. **取消上游**：回合取消令牌触发——abort LLM HTTP 流（OpenAI 无取消语义，直接断）、清空 `sentence_q`、`control.cancel` 至 TTS(turn)/ASR(segment)
3. **已播入账**：`played_seq` 前完整句以 assistant 消息入历史（`interrupted:true`）；在播句按已送音频估算前缀截断；未下发句丢弃

## 硬约束

- 同 `turn_id` 重复 barge_in 幂等，只执行一次
- `played_seq` 回报超时（500ms）按「已送出即已播」保守入账
- **记账完成前不回 `Listening`**——保证模型「记得的」= 用户「听到的」
- 取消令牌不复用：新回合 = 新令牌

## 时序目标

取消令牌 cancel 后各管线任务 <50ms 退出（`select!` 监听子 token）；模型服务须 200ms 内停产出，迟到帧按 `seq`/`tag` 丢弃。

::: info 权威来源
[docs/01-architecture/runtime.md](https://github.com/Momoyeyu/momo/blob/master/docs/01-architecture/runtime.md) §2-3 · [protocol.md](https://github.com/Momoyeyu/momo/blob/master/docs/01-architecture/protocol.md) `control.barge_in`/`control.cancel`。
:::
