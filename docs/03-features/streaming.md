# 直播扩展


```mermaid
flowchart LR
    IN["平台 ingress"] --> ADAPTER["streaming adapter"]
    ADAPTER --> CORE["复用同一 TurnCoordinator"]
    CORE --> OUT["现有字幕 / 音频 / avatar 输出"]
    OUT --> OVERLAY["平台 overlay"]
```

## 扩展方向

adapter 负责平台接入、授权、过滤与 overlay，不承载 turn、persona、memory 或推理业务。公开使用需要分别确认 Live2D 模型、Runtime Core、声音与平台许可。

相关协作方式见 [受控扩展](./dialogue.md)。
