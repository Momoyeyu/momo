# 部署拓扑

**正常档**：交互端在 Mac Air（或 laptop），编排+语音在 laptop（WSL2, RTX 3070 Laptop 8GB），大脑在 mac-studio（M3 Ultra 512GB，借用）。
**降级档**：mac-studio 不可用时 laptop 单机全栈——同一套协议与代码，**只改 env**。

<DiagramFrame src="deploy.html" :height="640" />

## 各机器角色

| 机器 | 角色 | 可用性假设 |
|---|---|---|
| Mac Air | 浏览器交互端；可选 CPU ASR / 8B MLX 二级兜底 | 自有 |
| laptop (WSL2) | 编排器 + ASR + TTS + infra；降级档大脑 | 自有，必须可独立全栈 |
| mac-studio | 大脑：Qwen3-30B-A3B-4bit 实测 121 t/s | **借用，随时收回** |
| bigstrong | 副脑 vLLM qwen3.8-27B | **借用，他人生产服务**，仅夜间窗口 |
| 4090D ×2 | 训练 burst（TTS SFT / LLM QLoRA） | 学校云，24h 硬上限 |
| DeepSeek API | 兜底/对照大脑、P4 视觉 | 云端，无音频能力 |

## 降级路径

```
mac-studio 收回 → MOMO_LLM_ENDPOINT 切本地 7-8B Q4 或 deepseek-v4-flash
                   → momoctl switch --to degraded 一键完成
```

降级档延迟预算放宽至 <2s；8GB VRAM 争抢时本地大脑让位给克隆 TTS，大脑走 DeepSeek。

## 借用纪律（硬约束）

- **bigstrong**：他人一直在用的生产 vLLM——并发槽仅 3，只当副脑不当依赖；仅配置窗口内使用，用完恢复。
- **4090D**：24h 会话硬上限；checkpoint 每 2h 落盘，**结束前 ≥1h 完成产物回传**，否则等于白训。
- **mac-studio**：按「随时消失」设计，正常档所有关键路径都有降级映射。

::: info 权威来源
[docs/05-ops/deployment.md](https://github.com/Momoyeyu/momo/blob/master/docs/05-ops/deployment.md) · [degradation.md](https://github.com/Momoyeyu/momo/blob/master/docs/05-ops/degradation.md) · [borrowed-etiquette.md](https://github.com/Momoyeyu/momo/blob/master/docs/05-ops/borrowed-etiquette.md)
:::
