# 人格与记忆：主流开源项目调研

> 调研目的：为 soul.md（人格）与 stage.md 记忆流的设计提供参照基线。
> 调研对象：SillyTavern chara card spec、AIRI（moeru-ai）、Letta/MemGPT、mem0、LangMem、Zep。
> 日期：2026-10-13。

## 1. 人格（soul）怎么定义

### 1.1 SillyTavern chara card v2/v3——事实上的行业标准

角色卡生态（SillyTavern、AIRI、各类前端）事实标准。v2 核心字段：

```ts
data: {
  name, description, personality, scenario,
  first_mes, mes_example,
  system_prompt, post_history_instructions,
  alternate_greetings, character_book,   // 关键词触发的 lorebook
  tags, creator, character_version, extensions
}
```

关键设计：

- **分层恒驻**：`name/description/personality/scenario` 是 permanent tokens，每次请求都在上下文里；`mes_example`（示例对话）是有余量才注入的软层。即「核心设定恒驻、示例可挤占」。
- **lorebook（character_book）**：关键词触发的小条目，命中关键词才进上下文——按需在恒驻与召回之间的第三层。
- **行为定义走示例而非规则**：`mes_example` 里用 `{{char}}`/`{{user}}` 写示范对话，让模型学语气和反应模式，比列行为条款有效得多。

### 1.2 AIRI（moeru-ai）——最接近的对标项目

- 角色即「卡」（`AiriCard`，基于 CCv3）：`systemPrompt + description + personality + scenario` 拼成 system prompt；卡可以换、可以云同步，**灵魂是数据不是代码**。
- 卡扩展字段挂了模块选择（consciousness=LLM provider/model、speech=TTS provider/voice、stageModel=Live2D/VRM 模型）——**一张卡同时决定「她是谁」和「她用什么身体/声音/大脑」**，换卡即换完整人格实例。
- 记忆是可选模块：IndexedDB/嵌入式存储 + 向量召回，settings 里有记忆浏览/管理页，默认关。

### 1.3 momo 现状对照

`app/persona/momo.yaml` 已有 `identity/personality/speech_style/emotion/behavior/dialogue_examples`，结构上与 chara card 同构，且做得更细的点：emotion tags 映射 Live2D、behavior 定义 on_interrupt/on_idle/on_unknown——这些是 chara card 没有的「交互行为面」。

差距与借鉴：

| 现状 | 借鉴方向 |
|---|---|
| yaml 单文件 | 可对齐 CCv3 字段命名，未来兼容社区卡生态（导入别人的卡） |
| 人格与载体分离 | AIRI 的卡捆绑模型/声线/大脑选择——momo 设置面可把「卡+provider+模型」当一个可切换的组合 |
| 无 lorebook 层 | persona.yaml 可加 `lore` 条目（关键词触发），放设定细节不占恒驻上下文 |
| examples 被复读 | chara card 把 mes_example 设为非恒驻；momo 可降权或压缩示例量 |

## 2. 记忆（memory）怎么管

### 2.1 Letta/MemGPT——记忆分层 + 自我编辑

最系统的分层模型：

```
CORE MEMORY（恒驻上下文，块结构）
├─ persona block   自我认知、人格准则（可自我改写→自进化的人格）
├─ human block     用户画像、偏好
└─ custom blocks   各 2000-5000 字符上限

EXTERNAL MEMORY（上下文外，按需检索）
├─ archival memory  语义检索（memory_insert/memory_replace 工具自写）
└─ conversation search  历史会话搜索
```

核心思想：**记忆是 agent 自己用工具维护的**——`memory_insert/replace/rethink` 是普通工具调用，agent 自主决定记什么、怎么改。block 有字符上限，满了要自己做取舍（改写、压缩、下沉到 archival）。

### 2.2 mem0——写路径管线化

写入管线（每次对话后异步跑，不占响应延迟）：

```
消息对 → 上下文查找（找相似旧记忆） → LLM 抽取事实（ADD-only）
       → 去重 + embedding → 实体抽取建图 → 时间元数据标注
```

关键决策：**ADD-only**——新事实追加而非覆盖旧事实，矛盾靠 retrieval 时排序解决，写路径永远幂等。三层存储：向量库（事实+embedding）、实体图、SQL（审计日志+滚动消息窗）。

### 2.3 LangMem——记忆分类学

三类记忆按用途分，回答「要学什么 → 存哪 → 怎么召回」：

| 类型 | 存什么 | momo 对应 |
|---|---|---|
| Semantic | 事实/偏好（profile 或 collection） | 用户的偏好、习惯、约定 |
| Episodic | 过去经历/会话摘要 | 「上次聊到你面试」这类可引用的共同经历 |
| Procedural | 行为模式/规则 | momo 学到的互动习惯（如「他喜欢睡前聊」→ 影响 on_idle 行为） |

写入两条路：热路径工具调用（agent 主动记）+ 后台 manager（异步抽取/合并）。

### 2.4 Zep/Graphiti——时序知识图谱

每条事实（edge）带双时间轴：valid time（事实上何时为真）+ ingestion time（系统何时学到）。新事实矛盾旧事实时**旧的标记失效而非删除**——保留「曾经是什么」的历史，可回答「当时你是怎么说的」。对「用户改口/纠正」场景最鲁棒，但图引擎对单机 app 偏重。

## 3. 对 momo 的设计建议

综合对标，momo 的记忆系统建议落在：

```
L0 恒驻      persona.yaml（身份/人格/行为） + human block（用户画像摘要，自维护，字符上限）
L1 触发      lore 条目（关键词命中注入，chara card lorebook 式）
L2 召回      记忆条目：异步抽取管线（mem0 式 ADD-only + 去重），向量检索按回合注入
L3 审计      原始会话转写（不可改写，供溯源/训练语料）
```

写路径：

- **异步抽取**（学 mem0）：回合结束后后台跑「上下文查找→LLM 抽取→去重→embedding」，不阻塞回复延迟。模型自己决定记什么（Letta 式 self-editing 的温和版：自主归纳而非逐条问用户）。
- **纠错即失效**（学 Zep）：用户纠正（correction）→ 相关记忆标记 `invalid` 不删除，审计可见完整历史。
- **procedural 记忆回流人格**（学 LangMem）：长期稳定的互动模式可沉淀为 persona 的补丁层（如 `learned_traits`），这就是「自进化」的具体机制——人格不只静态卡，而是可被相处塑造。

读路径：

- 每回合：L0 恒驻注入 + 用户输入 embedding 召回 L2 top-k（有 embed 服务时）+ lore 关键词命中。
- 召回失败/无 embed 服务：纯 L0 运行，记忆退化但不崩。

审计面（用户侧）：

- 设置页的记忆管理：列表浏览、编辑、删除（AIRI 的记忆管理页模式）。
- 删除即物理删（向量+条目+派生）；correction 是语义失效（保留历史）。

## 4. 参考链接

- chara card spec v2: https://github.com/malfoyslastname/character-card-spec-v2/blob/main/spec_v2.md
- AIRI: https://github.com/moeru-ai/airi（`stores/modules/airi-card.ts`、LTM PR #1973）
- Letta memory blocks: https://docs.letta.com/v1-sdk/memory/memory-blocks/ + MemGPT 论文
- mem0: https://docs.mem0.ai/core-concepts/how-it-works + arXiv 2504.19413
- LangMem: https://langchain-ai.github.io/langmem/concepts/conceptual_guide/
- Zep/Graphiti: https://arxiv.org/abs/2501.13956
