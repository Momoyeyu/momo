# momo 路线图



```mermaid
flowchart LR
```



- 会话历史查看、改标题、correction、删除、导出与新对话；原 audit entry 不被改写。
- 事件、字幕与音频通过 momo-ws 统一协作。



- quiet / focus / companion 三种模式，与麦克风 mute 分离。
- 用户明确启用的主动建议；专注、通话、收听、安静时段不触发，dismiss 后保持安静。
- 仅使用获授权素材的声音个性化，复用 frozen evaluation gate 与回滚门槛。



- 工具与截图逐次请求权限，allow/deny/cancel/result 全路径可见。

## 语言与长期轨道

产品默认中文；中、英、日输入与输出按 ASR/TTS/LLM provider 的 capability 声明启用。

- [人格](../04-tracks/persona.md)：稳定人设与可纠正上下文优先；微调是可选后续工具。
