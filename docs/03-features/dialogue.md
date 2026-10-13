# 受控扩展


## 工具权限路径

```mermaid
flowchart TD
    REQUEST["LLM 提议 tool/screenshot"] --> DISCLOSE["显示动作、范围、数据去向"]
    DISCLOSE --> USER{"allow once / deny / cancel"}
    USER -->|"deny"| DENIED["返回 denied，不执行"]
    USER -->|"allow once"| EXEC["最小权限执行"]
    EXEC --> ACTIVE{"turn 仍有效？"}
    ACTIVE -->|"否/取消"| DROP["cancel；丢弃迟到 result"]
    ACTIVE -->|"是"| RESULT["结构化 result 回到同一 turn"]
    RESULT --> SHOW["向用户呈现结果或错误"]
```

## future streaming adapter

```mermaid
flowchart LR
    PLATFORM["未来平台 ingress"] --> ADAPTER["adapter：归一化/平台权限/过滤"]
    ADAPTER --> CORE["同一个 TurnCoordinator"]
    CORE --> OUTPUT["既有字幕/音频/avatar"]
    OUTPUT --> OVERLAY["未来 overlay adapter"]
```

adapter 不拥有 turn、persona、memory、推理或 runtime 业务。

## 验收

- [ ] deny 不产生外部副作用；执行中 cancel 会丢弃迟到结果。
- [ ] screenshot 每次说明采集范围，不因曾允许一次而永久授权。
- [ ] streaming adapter 保持独立，只负责平台接入与输出 overlay。
