# 用户操作路径图谱



### 文字回合

```mermaid
flowchart TD
    ENTER["输入框 Enter / 点击发送"] --> IME{"IME composing？"}
    IME -->|"是"| KEEP["忽略 Enter，保留 draft"]
    IME -->|"否"| EMPTY{"trim 后为空？"}
    EMPTY -->|"是"| KEEP
    EMPTY -->|"否"| LIMIT{"≤ 4000 字符？"}
    LIMIT -->|"否"| REJECT["input.rejected；保留 draft"]
    LIMIT -->|"是"| SEND["input.text(id)"]
    SEND --> DUP{"256-entry receipt cache 已有 id？"}
    DUP -->|"是"| ACKOLD["ACK 既有 turn；不新建"]
    DUP -->|"否"| BUSY{"活动 turn？"}
    BUSY -->|"是"| INVALID["立即 invalidate / cancel 旧 turn"]
    BUSY -->|"否"| ACCEPT["input.accepted + 新 turn"]
    INVALID --> ACCEPT
    ACCEPT --> STREAM["LLM → 句切 → 字幕 / TTS / 播放"]
    STREAM --> STOP{"Stop / 新输入？"}
    STOP -->|"是"| INTERRUPT["本地停播 + 并行取消 + settlement"]
    STOP -->|"否"| DONE["output.complete + required ACK → idle"]
```

文字输入回执固定使用 `input.accepted.ref_id = input.text.id`。

### PTT

```mermaid
flowchart TD
    PRESS["按下 PTT"] --> ACTIVE{"当前 thinking / speaking？"}
    ACTIVE -->|"是"| STOP["同步本地 Stop<br/>invalidate 旧 reply；不取消新 capture"]
    ACTIVE -->|"否"| PERM{"权限、设备与 ASR 可用？"}
    STOP --> PERM
    PERM -->|"否"| TEXT["释放 mic；文字 / 选设备 / retry"]
    PERM -->|"是"| START["input.audio.start(mode=ptt)<br/>新 capture 独立存活"]
    START --> EVENT{"下一事件"}
    EVENT -->|"松开"| SEGMENT{"检测到非静音 segment？"}
    SEGMENT -->|"否"| TOO["cancel；释放 mic；提示未听到语音"]
    SEGMENT -->|"是"| END["input.audio.end → ASR final"]
    EVENT -->|"主动取消"| CANCEL["input.audio.cancel；释放 mic"]
    EVENT -->|"窗口失焦"| CANCEL
    END --> REPLY["final text 开启 reply turn"]
```

focus loss 始终取消 PTT。旧 reply 的 cancellation token 不包含新 capture。

### 连续 VAD

```mermaid
flowchart TD
    ENABLE["用户开启连续麦"] --> READY{"权限、设备与 ASR ready？"}
    READY -->|"否"| EXIT["释放 mic；回到关闭态<br/>提供重新启用 / PTT / 文字"]
    READY -->|"是"| DUPLEX{"可靠 AEC 或用户确认耳机？"}
    DUPLEX -->|"是"| LISTEN["continuous capture"]
    DUPLEX -->|"否"| HALF["half-duplex<br/>播放期间 mute uplink"]
    LISTEN --> START{"speech_start？"}
    HALF --> PLAYING{"正在播放？"}
    PLAYING -->|"是"| MUTED["uplink muted<br/>Stop / PTT 仍可打断"] --> PLAYING
    PLAYING -->|"否"| START
    START -->|"否"| DUPLEX
    START -->|"thinking 或 speaking"| BARGE["立即 Stop 旧 reply<br/>保留活动新 capture"]
    START -->|"idle"| RECORD["收集语音"]
    BARGE --> RECORD
    RECORD --> END{"speech_end + final？"}
    END -->|"误触 / 空段"| LISTEN
    END -->|"有效 final"| TURN["启动新 reply turn<br/>continuous capture 保持"]
    TURN --> DUPLEX
```

```mermaid
flowchart LR
    OWNER["continuous capture owner<br/>LISTEN / HALF / MUTED / RECORD / TURN"] --> EVENT{"停止收听 / device loss / ASR error"}
    EVENT --> EXIT["从任一 capture 状态退出<br/>释放 mic + 显示重新启用"]
```

没有可靠 AEC 且没有确认使用耳机时只允许 half-duplex；不能用一次风险确认冒充 full-duplex 回声抑制。全局 capture owner 让用户主动停止、设备断开或 ASR 错误从任一 listening/half-duplex/muted/recording 状态退出。

### 正常语音序列

```mermaid
sequenceDiagram
    participant U as 用户
    participant UI as 桌面 UI / AudioWorklet
    participant O as TurnCoordinator
    participant A as ASR / VAD
    participant L as LLM
    participant T as TTS
    U->>UI: PTT 或 opt-in VAD 说话
    UI->>O: input.audio.start + audio.open/uplink + frames
    O->>A: 转发 capture stream
    A-->>O: input.audio.segment(final,text,capture_id)
    O-->>UI: input.accepted(ref_id=capture_id) + state.turn(thinking)
    O->>L: chat/completions SSE
    L-->>O: tokens
    O-->>UI: output.sentence
    O->>T: sentence synth
    T-->>O: audio.open/downlink + frames + EOS
    O-->>UI: audio stream
    UI-->>O: playback.started → state.turn(speaking)
    UI-->>O: playback.progress(completed)
    O-->>UI: output.complete + turn.finished(completed)
```


### 断线、resume 与 receipt

```mermaid
flowchart TD
    LOST["WS / 网络断开"] --> LOCAL["同步停播、归零嘴型<br/>保留本地 draft 与 accepted receipts"]
    LOCAL --> RETRY["退避重连"]
    RETRY --> CONNECT{"连接成功？"}
    CONNECT -->|"否"| OFFLINE["显示离线；不自动提交"] --> RETRY
    CONNECT -->|"是"| HELLO["发送 hello(resume_session_id)"]
    HELLO --> OK["hello.ok"] --> SNAP["session.snapshot<br/>turn / receipts / last_event_seq"]
    SNAP --> STATE{"event sequence 可收敛？"}
    STATE -->|"否"| CLOSE["关闭分叉连接；重新握手"] --> RETRY
    STATE -->|"是"| RECEIPT{"本地 input id 状态"}
    RECEIPT -->|"snapshot 已 accepted"| USE["采用已有 receipt / turn<br/>绝不再生成"]
    RECEIPT -->|"恢复同 session 但 unknown"| SAME["展示 draft；用户可用同一 id 重发"]
    RECEIPT -->|"新 session / receipt 已淘汰"| NEW["展示 draft；用户显式发送时生成新 id"]
    USE --> IDLE["不续播旧声音"]
    SAME --> IDLE
    NEW --> IDLE
```

每 session 持久化最近 256 个 accepted receipts。客户端忽略 `event_seq <= last_event_seq` 的 duplicate/stale event；重连从不自动发送 draft。

### provider 切换

```mermaid
flowchart TD
    PICK["用户选择 provider"] --> ACTIVE{"有活动 turn？"}
    ACTIVE -->|"是"| CONFIRM{"明确取消当前 turn？"}
    CONFIRM -->|"否"| KEEP["保留当前 provider"]
    CONFIRM -->|"是"| CANCEL["Stop + cancel turn"]
    ACTIVE -->|"否"| WARM["后台 warmup 候选"]
    CANCEL --> WARM
    WARM --> PROBE{"probe / readiness 通过？"}
    PROBE -->|"否"| ROLLBACK["保留旧 provider；显示错误 / retry"]
    PROBE -->|"是"| SWAP["原子 activate；下个 turn 生效"]
    SWAP --> RETAIN["短期保留旧实例供 rollback"]
```

### 会话历史、correction 与文件事务

```mermaid
flowchart TD
    OPEN["打开历史"] --> LIST["按 session 列表"]
    LIST --> ACTION{"操作"}
    ACTION -->|"查看"| VIEW["显示 audit transcript<br/>含 interrupted / incomplete"]
    ACTION -->|"改标题"| RENAME["准备 rename transaction"]
    ACTION -->|"纠正已听文本"| CORRECT["保留原 audit entry<br/>准备新增 correction"]
    ACTION -->|"删除 / 新对话"| CONFIRM{"先确认切换 / 删除？"}
    CONFIRM -->|"否"| LIST
    CONFIRM -->|"是"| ACTIVE{"目标是 active session？"}
    ACTIVE -->|"是"| CANCEL["Stop + cancel current turn"] --> COMMIT["原子更新 session 与 derived-memory references"]
    ACTIVE -->|"否"| COMMIT
    ACTION -->|"导出"| EXPORT["选择位置 → JSON / Markdown"]
    RENAME --> TXN{"文件事务成功？"}
    CORRECT --> TXN
    COMMIT --> TXN
    EXPORT --> TXN
    TXN -->|"否：空间满 / 权限拒绝 / 取消"| RETRY["保留本地数据与 draft<br/>报告错误 / retry；不显示成功"]
    TXN -->|"是"| SUCCESS["transaction 完成后显示成功"]
    RETRY --> LIST
    SUCCESS --> LIST
```



### quiet / focus / companion

```mermaid
flowchart TD
    MODE["选择 companion mode"] --> WHICH{"模式"}
    WHICH -->|"quiet"| QUIET["不主动出声 / 建议；用户输入仍回答"]
    WHICH -->|"focus"| FOCUS["仅必要状态提示；屏蔽主动建议"]
    WHICH -->|"companion"| COMP["允许用户已启用的主动建议"]
    QUIET --> MIC{"麦克风 mute？"}
    FOCUS --> MIC
    COMP --> MIC
    MIC -->|"是"| MUTED["停止 capture；文字保持可用"]
    MIC -->|"否"| INPUT["按另行选择的 PTT / VAD 模式收听"]
```

模式不是 mute；quiet 仍可使用 PTT，companion 也不能绕过麦克风关闭。

### 记忆提议、correction 与 CRUD

```mermaid
flowchart TD
    CHAT["对话或 correction 出现可记内容"] --> PROPOSE["memory proposal<br/>来源与用途可见"]
    PROPOSE --> CONFIRM{"用户确认？"}
    CONFIRM -->|"否"| DROP["不保存"]
    CONFIRM -->|"是"| EDIT{"先编辑？"}
    EDIT -->|"是"| REVISE["修改内容 / 类型"]
    EDIT -->|"否"| SAVE["本地保存正文 + vector index"]
    REVISE --> SAVE
    SAVE --> RETRIEVE["相关 turn 检索；标明记忆来源"]
    RETRIEVE --> MANAGE{"用户管理"}
    MANAGE -->|"编辑"| REINDEX["更新正文并重建 index"] --> RETRIEVE
    MANAGE -->|"删除"| DELETE["事务删除正文 + index"]
    MANAGE -->|"来源 correction"| INVALIDATE["失效 derived memory<br/>重新提议后才保存"]
```

### 主动建议

```mermaid
flowchart TD
    TICK["情境 / idle 触发候选"] --> ENABLED{"用户已启用 proactive？"}
    ENABLED -->|"否"| SILENT["保持安静"]
    ENABLED -->|"是"| BLOCK{"focus / 通话 / listening / quiet hours？"}
    BLOCK -->|"是"| SILENT
    BLOCK -->|"否"| SUGGEST["显示轻量建议；不抢输入"]
    SUGGEST --> RESPONSE{"用户反应"}
    RESPONSE -->|"接受"| TURN["显式开启 turn"]
    RESPONSE -->|"dismiss"| COOLDOWN["该建议保持安静 / 进入冷却"]
    RESPONSE -->|"关闭功能"| DISABLE["持久关闭 proactive"]
```


### 工具与截图

```mermaid
flowchart TD
    NEED["模型提出 tool / screenshot 请求"] --> SHOW["展示动作、范围、数据去向"]
    SHOW --> DECIDE{"用户决定"}
    DECIDE -->|"deny"| DENY["返回 denied；不执行"]
    DECIDE -->|"allow once"| EXEC["按最小权限执行"]
    EXEC --> CANCEL{"执行中取消？"}
    CANCEL -->|"是"| ABORT["control.cancel；丢弃迟到 result"]
    CANCEL -->|"否"| RESULT["结构化 result → 当前有效 turn"]
    RESULT --> PRESENT["向用户显示结果 / 错误"]
```

### streaming adapter（以后）

```mermaid
flowchart LR
    INGRESS["平台 ingress adapter"] --> NORMALIZE["归一化输入 + 平台授权 / 过滤"]
    NORMALIZE --> CORE["同一个 TurnCoordinator"]
    CORE --> OUTPUT["既有 subtitle / audio / avatar output"]
    OUTPUT --> OVERLAY["平台 overlay adapter"]
    CORE -.-> NO["adapter 不承载 turn / 记忆 / 推理业务"]
```

