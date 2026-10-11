# 编排器运行时

> 每次交流都围绕一轮完整的回应展开：接收输入、生成表达、同步播放、接受插话，并保存真实发生的对话。

## 三个独立时钟

- persona / connection：角色配置、连接与 provider readiness；
- turn：`TurnCoordinator` 唯一拥有的对话状态；
- presentation：TS `PresentationDirector` 的动作与嘴型仲裁。

收到文本不等于正在说话，表情动作也不能改变 turn。

## turn 状态机

```mermaid
stateDiagram-v2
    [*] --> idle
    idle --> listening: "input.audio.start / VAD speech_start"
    idle --> thinking: "input.text accepted / voice final"
    listening --> thinking: "input.audio.segment final"
    listening --> idle: "cancel / empty / false positive"
    thinking --> speaking: "playback.started"
    thinking --> thinking: "new text invalidates old turn then accepts new"
    thinking --> interrupted: "Stop / PTT / VAD speech_start"
    thinking --> idle: "LLM failed + turn.finished(failed)"
    thinking --> idle: "text-only output.complete"
    speaking --> interrupted: "Stop / PTT / VAD speech_start"
    speaking --> interrupted: "new input.text"
    speaking --> thinking: "device / mute / TTS failure; failed streams removed"
    speaking --> idle: "output.complete + all required completed ACK"
    speaking --> idle: "LLM failed + invalidate audio + turn.finished(failed)"
    interrupted --> listening: "new capture remains active"
    interrupted --> thinking: "new text accepted"
    interrupted --> idle: "no new input after settlement"
```

`Speaking` 只由实际 AudioWorklet 的 `playback.started` 进入。静音/纯文字 turn 不进入 speaking。正常完成只有在 `output.complete` 与 required completed ACK 齐备后回 idle；图中的 LLM failure 是显式异常终态，必须先 invalidate audio、清除 pending set 并发送 `turn.finished(failed)`。

## capture 状态机

```mermaid
stateDiagram-v2
    state "failed" as captureFailure
    [*] --> closed
    closed --> opening: "PTT press / VAD opt-in"
    opening --> capturing: "permission + device ready"
    opening --> denied: "permission denied / device missing"
    capturing --> finalizing: "PTT release / speech_end"
    capturing --> canceled: "Stop listening / focus loss / device loss"
    finalizing --> closed: "final segment emitted"
    finalizing --> captureFailure: "ASR failed"
    denied --> closed: "release mic, use text / retry"
    canceled --> closed: "release mic, show re-enable"
    captureFailure --> closed: "release mic, re-record / use text"
```

capture 不属于 reply turn；打断旧回复时不得取消正在形成的新 capture。

## 增量句切

```mermaid
flowchart TD
    TOKEN["LLM delta"] --> BUFFER["追加到 incremental buffer"]
    BUFFER --> TAG{"开头是 8 个已知 emotion tag？"}
    TAG -->|"是"| STRIP["移除 tag；更新当前 emotion"]
    TAG -->|"否 / 未知 []"| KEEP["原样保留文本<br/>不吞 markdown 内容"]
    STRIP --> BOUNDARY{"检查切分边界"}
    KEEP --> BOUNDARY
    BOUNDARY -->|"CJK 强标点 / 换行"| EMIT["按 Unicode 边界发 output.sentence"]
    BOUNDARY -->|"ASCII . ! ?"| LOOK{"小数 / URL / 缩写？<br/>或后接空白 / stream end？"}
    LOOK -->|"小数 / URL / 缩写"| WAIT["等待 lookahead"]
    LOOK -->|"空白 / stream end"| EMIT
    BOUNDARY -->|"逗号且 buffer ≥ 25"| EMIT
    BOUNDARY -->|"buffer ≥ 60"| HARD["最近安全 Unicode 边界强切"] --> EMIT
    BOUNDARY -->|"尚未成句"| WAIT
    EMIT --> NEXT["清已发部分；继续累计"]
    END["LLM stream end"] --> LEFT{"有 leftover？"}
    LEFT -->|"是"| FLUSH["flush leftover sentence"] --> COMPLETE["output.complete"]
    LEFT -->|"否"| COMPLETE
    CANCEL["turn cancel"] --> DROP["丢弃未 emitted buffer"]
```

软切阈值使用既有 `MOMO_SENT_SOFT_CHARS=25`，硬上限使用 `MOMO_SENT_MAX_CHARS=60`。只有固定 8 个 emotion tag 被剥离；其他方括号内容保留。

## generation、TTS 与播放

```mermaid
flowchart TD
    SENT["output.sentence(seq)"] --> TTS{"TTS 成功？"}
    TTS -->|"是"| OPEN["audio.open(stream → turn/sentence)"]
    OPEN --> BUFFER{"AudioWorklet 有可播 buffer？"}
    BUFFER -->|"首播前不足"| WAIT["保持 thinking / 等 chunk"] --> BUFFER
    BUFFER -->|"后续句不足"| HOLDSTATE["当前 speaking 状态不变"] --> BUFFER
    BUFFER -->|"足够"| START["playback.started → speaking"]
    START --> PROGRESS["playback.progress ledger"]
    PROGRESS --> EOF{"EOS / audio.end 且 buffer EOF？"}
    EOF -->|"否"| PROGRESS
    EOF -->|"是"| ACK["校验 total samples 后 completed ACK"]
    TTS -->|"失败"| CLEAR["清 stream / pending set / 嘴型"] --> TEXT["剩余回复 text-only"]
    COMPLETE["output.complete"] --> JOIN{"generation 完成且<br/>required final ACK 齐？"}
    ACK --> JOIN
    TEXT --> JOIN
    JOIN -->|"是"| IDLE["turn.finished → idle"]
    JOIN -->|"否且 deadline 未到"| HOLD["等待 ledger；不重播"]
    JOIN -->|"deadline 到"| UNCONFIRMED["停止 stream；unconfirmed audit<br/>移除 pending 后关闭 turn"]
```

TTS 失败后不重播已经听过的文字。LLM 部分失败保留可见输出并标记 incomplete。

## bounded queue 与 backpressure

```mermaid
flowchart TD
    MIC["mic uplink buffer<br/>最多 2s"] --> MICFULL{"overflow / seq gap？"}
    MICFULL -->|"是"| ABORTCAP["中止 capture；释放 mic<br/>提示重新录制"]
    MICFULL -->|"否"| ASR["ASR"]
    LLM["LLM SSE"] --> SQ["sentence_q<br/>容量 8"]
    SQ --> SQFULL{"队列满？"}
    SQFULL -->|"是"| BLOCK["暂停读取 LLM SSE"] --> SQ
    SQFULL -->|"否"| SERIAL["TTS 串行合成<br/>保持 sentence 顺序"]
    SERIAL --> OUT["output audio buffer<br/>96 × 50ms"]
    OUT --> OUTFULL{"队列满 / consumer stuck？"}
    OUTFULL -->|"未满"| PLAY["AudioPlayer"]
    OUTFULL -->|"满"| PAUSE["暂停拉取 TTS"]
    PAUSE --> STUCK{"deadline 内恢复？"}
    STUCK -->|"是"| OUT
    STUCK -->|"否"| FAIL["invalidate audio；turn 转 text-only<br/>显示失败通知"]
    CTRL["priority control channel"] -.->|"Stop / cancel 永不排在 data 后"| ASR & SERIAL & PLAY
```

上行不静默丢旧帧，否则 chunk sequence 与 transcript 都不可信。控制通道具有独立优先级；data backlog 不能阻塞 Stop/cancel。

## interruption：立即取消，限时结算

```mermaid
flowchart TD
    TRIGGER["Stop / 新输入 / VAD speech_start"] --> LOCAL["UI 同步 stop + flush<br/>嘴型=0"]
    TRIGGER --> INVALID["actor 原子 invalidate old turn"]
    INVALID --> FORK["并行取消 LLM / splitter / TTS / audio"]
    INVALID --> NOTICE["立即发 stop notice"]
    NOTICE --> LEDGER["收 playback ACK<br/>settlement deadline 500ms"]
    LEDGER --> HEARD{"确认听见？"}
    HEARD -->|"完整句"| CONTEXT["写 spoken context"]
    HEARD -->|"部分句"| AUDIT["audit: played_samples + partial<br/>无 timestamps 不造文本前缀"]
    HEARD -->|"缺 ACK / deadline"| OMIT["保持 unconfirmed"]
    CONTEXT --> FINISH["幂等 finalize<br/>turn.finished 恰好一次"]
    AUDIT --> FINISH
    OMIT --> FINISH
    LATE["迟到 ACK"] --> ANNOTATE["只补 audit annotation<br/>不改 model context"]
```

取消不等待 ACK；新 capture 单独保留。completed ACK 必须单调、在 total samples 范围内，并与 EOS/audio.end 对齐。

## audio device、mute 与 AudioContext

```mermaid
flowchart TD
    EVENT["speaking 中设备变化 / mute"] --> STOP["同步停播 + settlement"]
    STOP --> REMOVE["清 audio stream 并从 pending set 移除"]
    REMOVE --> TEXT["剩余 output 以 text-only 完成"]
    TEXT --> FUTURE["unmute 只影响未来 sentence / turn<br/>不重播漏听音频"]
    SUSPEND["AudioContext suspended"] --> CTA["显示点击恢复播放"]
    CTA --> RESUME{"用户触发 resume 成功？"}
    RESUME -->|"是"| PLAY["仅播放尚未错过的有效 stream"]
    RESUME -->|"否"| TEXT
    DEADLINE["playback.started 后<br/>AudioContext running 且 2000ms 无 progress"] --> STOPSTREAM["停止 stream；unconfirmed audit"] --> REMOVE
```

普通播放的 stall deadline 是 `playback.started` 之后、AudioContext 处于 running 时连续 2000ms 无进度；尚未开始的 queued audio 不计 stall。500ms 只用于 interruption settlement。LLM/synthesis stall 使用既有 `MOMO_LLM_TIMEOUT_MS` 或 turn provider timeout，不复用 500ms。

## stale stream 防线

```mermaid
flowchart LR
    FRAME["到达 binary frame"] --> KNOWN{"stream_id 已由 audio.open 绑定？"}
    KNOWN -->|"否"| DROP1["拒绝 unknown stream"]
    KNOWN -->|"是"| VALID{"turn 有效 + direction / format 正确？"}
    VALID -->|"否"| DROP2["丢弃并记协议错误"]
    VALID -->|"是"| SEQ{"chunk_seq / offset 连续且未重复？"}
    SEQ -->|"重复"| DROP3["幂等丢弃"]
    SEQ -->|"gap"| ABORT["中止 stream + retry affordance"]
    SEQ -->|"连续"| PLAY["交给绑定的 capture / player"]
```

不能用“最近一次 state.turn”绑定音频。已取消 turn 的迟到帧、TTS 结果或 LLM delta 都不可见。

## presentation 仲裁

```mermaid
flowchart TD
    EVENTS["turn + pointer + audio events"] --> DIRECTOR["TS PresentationDirector"]
    DIRECTOR --> STOP{"Stop / invalid stream？"}
    STOP -->|"是"| RESET["停止动作与播放<br/>嘴型立即归零"]
    STOP -->|"否"| SPEAK{"实际 playback 正在进行？"}
    SPEAK -->|"是"| MOUTH["AudioWorklet RMS 驱动嘴型<br/>表情不得覆盖嘴型"]
    SPEAK -->|"否"| LT{"listening / thinking？"}
    LT -->|"是"| STATUS["收听 / 思考 expression"]
    LT -->|"否"| TAP{"tap reaction 待执行？"}
    TAP -->|"是"| REACT["短 reaction"]
    TAP -->|"否"| IDLE["gaze / available idle"]
```

固定优先级：`stop > speaking mouth from actual AudioWorklet playback > listening/thinking expressions > tap reaction > gaze/idle`。

## 任务与错误所有权

Session actor 串行处理命令；每个 reply turn 建立独立 cancellation token tree，扇出到 LLM、splitter、TTS scheduler、audio mux。任务只能回传事件，不能直接修改 actor 状态。
