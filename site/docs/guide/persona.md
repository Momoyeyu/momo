# 人格微调轨道

目标：momo 说话像「她」而不是通用助手。两步走——**P0-P2 character card + system prompt 逼近；P3 起攒数据 QLoRA 把人格写进权重**（prompt 聊久会滑回助手腔，LoRA 是解药）。

<SvgDiagram src="persona-qlora.svg" title="人格微调流水线" />

## character card

单一事实来源 `persona/momo.yaml`（`{b[:-1]}/` 仓），schema 覆盖：

```yaml
name: momo
identity: { species, birthday, likes, dislikes }
personality: { traits, tone, taboos }
speech_style: { language: [zh,en,ja], code_switch, sentence_habit, honorifics, examples }
emotion: { tags: [8 值与协议对齐], tag_guide }
behavior: { on_interrupt, on_idle, on_unknown }
dialogue_examples: []   # 人工修写过的 few-shot，金标准
```

纪律：`traits` 与 `dialogue_examples` **必须人工修写**，不堆自动生成；card 变更走 PR + 回归评测通过才合并——**改人设视同改代码**。

## 数据 → 权重

1. 日常会话采集（脱敏入库）
2. 人工修写不符人设的回复 → SFT JSONL（数百条起步）
3. 20 题固定评测集回归
4. 达标 → 4090D QLoRA（8-14B，≤24h）→ LoRA 挂载 → 灰度回归 → 上线；不达标 → 回改 card

::: info 权威来源
[docs/04-tracks/persona.md](https://github.com/Momoyeyu/momo/blob/master/docs/04-tracks/persona.md) — prompt 注入方式、评测集设计、LoRA 灰度策略。
:::
