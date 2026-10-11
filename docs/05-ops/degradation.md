# 当前降级与恢复手册

> provider 或平台能力不可用时保留普通可见窗口、avatar、draft、设置和本地 transcript；LLM ready 时才提供 AI 文字 reply。

## 触发

- provider readiness/probe 失败或活动连接断开；
- 透明窗口、Wayland、全局快捷键、system menu 或麦克风能力不可用；
- normal profile 所需机器不可达；
- 用户主动选择更轻的本地模式。

## 决策路径

```mermaid
flowchart TD
    FAIL["能力 / provider 不可用"] --> TURN{"活动 turn？"}
    TURN -->|"是"| STOP["同步 Stop + cancel + settlement"]
    TURN -->|"否"| CLASSIFY{"故障类型"}
    STOP --> CLASSIFY
    CLASSIFY -->|"透明窗口"| WINDOW["普通可见窗口"]
    CLASSIFY -->|"menu / tray"| MENU["禁用 click-through<br/>保留窗口内菜单"]
    CLASSIFY -->|"麦克风 / ASR"| TEXTIN["释放 mic；禁用语音输入<br/>文字输入仍可编辑"]
    CLASSIFY -->|"TTS"| TEXTOUT["剩余 reply text-only"]
    CLASSIFY -->|"LLM"| NOLLM["保留 avatar / draft / 设置<br/>不可生成 AI reply"]
    CLASSIFY -->|"provider 可替换"| OLD{"旧 provider 仍健康？"}
    OLD -->|"是"| ROLLBACK["原子 rollback"]
    OLD -->|"否"| PROFILE["选择 degraded provider"]
    PROFILE --> WARM["warmup + probe"]
    WARM --> READY{"通过？"}
    READY -->|"否"| NOLLM
    READY -->|"是"| ACTIVATE["下个 turn 原子 activate"]
```

## 恢复

候选能力恢复后仍走 warmup/probe；成功前不替换当前健康路径。窗口恢复透明模式前转换坐标并 clamp。session 重连只取 snapshot，不播放断线前音频。退出/切换只停止 owned child，attached provider 不受影响。

## 检查

- [ ] 失败切换不会破坏 active 配置。
- [ ] menu 不可用时 click-through 已禁用。
- [ ] LLM 不可用时没有伪装成可工作的 text fallback。
- [ ] 切换前活动 turn 得到显式 cancel。
- [ ] 恢复后第一轮没有 stale stream 或旧音频。
