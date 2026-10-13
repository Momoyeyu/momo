# 决策汇总（ADR）

> 所有已确认设计决策的单点定论。每条：标题 + 决策 + 理由 + 被否选项。其他文档引用结论，不重述理由。
> 状态均为「已锁定」，除非特别注明。推翻任何一条都需要先改本文档。

```mermaid
flowchart TD
    ADR001["ADR-001 monorepo<br/>协作期形态 → 拆分"] --> ADR027["ADR-027 终态拆分<br/>app=momo / models+infra=momo-infra"]
    ADR001 --> ADR002["ADR-002 语言分工<br/>Rust/Python/TS"]
    ADR001 --> ADR003["ADR-003 三段式进程<br/>+ momo-ws 协议"]
    ADR002 --> ADR004["ADR-004 编排器在 app<br/>推理在 models"]
    ADR005["ADR-005 MOMO_ env 配置"] --> ADR008["ADR-008 档位=改 env<br/>不改代码"]
    ADR006["ADR-006 LLM 统一<br/>OpenAI 接口"] --> ADR007["ADR-007 主脑 MLX<br/>+ DeepSeek 兜底"]
    ADR007 --> ADR024["ADR-024 主脑定案<br/>30B-A3B-4bit"]
    ADR028["ADR-028 app 单槽位 provider<br/>容灾在服务侧"] --> ADR029["ADR-029 momo-infra<br/>训练 + serving gateway"]
    ADR009["ADR-009 ASR+VAD"] --> ADR003
    ADR010["ADR-010 TTS 选型梯队"] --> ADR003
    ADR011["ADR-011 Web 优先迭代<br/>Tauri 后置复刻"] --> ADR012["ADR-012 Live2D+口型"]
    ADR013["ADR-013 回合语义/<br/>barge-in 三动作"] --> ADR003
    ADR019["ADR-019 扩展过滤边界"] --> FUTURE["未来 streaming 前置条件"]
```

## 仓库与工程

### ADR-001 协作期 monorepo，按目录划分模块（非终态）

- **决策**：当前 `momo` 以单一仓库组织；`app/`、`models/`、`infra/`、`site/`、`docs/` 按目录划分所有权，边界见 `01-architecture/boundaries.md`。**monorepo 只是协作期形态**，终态拆分见 ADR-027。
- **理由**：协议、模型与文档变更需要原子同步；目录级 CI 仍可保持工具链隔离。
- **被否**：现在就拆成多个仓库（跨仓协议迁移和文档同步无法原子完成，拆分在模块契约稳定后进行）。

### ADR-002 语言分工：Rust=运行时与工具，Python=模型，TS=薄前端

- **决策**：Python 只做模型相关业务；Rust 做应用运行时与 infra 小工具；TS 仅限薄前端。Rust 生态不足处允许单独引入 C++ 组件，**须文档注明理由**。
- **理由**：模型生态（torch/onnx/MLX/训练脚本）全在 Python；回合状态机要稳定并发、低延迟、可分发单二进制；前端只需渲染与音频 IO。
- **被否**：全 Python（GIL 下并发管线与分发都吃力）；全 Rust（模型绑定生态不足，推理侧会反复造轮）。

### ADR-005 一切环境差异走 `MOMO_` 环境变量

- **决策**：环境差异与密钥全部走 `MOMO_` 前缀环境变量；仓库只提交 `.env.example`（占位无值），本地 `.env` 不入库；兼容后续配置发布平台。规范见 [../01-architecture/config.md](../01-architecture/config.md)。
- **理由**：密钥不进 git；env 是部署平台最通用的注入面；降级切换=改 env。
- **被否**：配置文件入库（密钥泄露风险、多机差异难管）；CLI-only（对配置发布平台不友好，且长参数难维护）。

### ADR-020 文档先于代码 + 单一 GitHub Project

- **决策**：协作期内单一 monorepo + 1 个 GitHub Project（Projects V2）排需求；协议/状态机/配置先写文档再实现。仓库拆分后 project 归属随之迁移。
- **理由**：单人项目最大的风险是决策漂移，文档是唯一的对齐物。
- **被否**：先代码后补文档（单人也会忘）；每仓各自 issue 分散排期（全局优先级不可见）。

### ADR-023 infra CLI 唯一命名 `momoctl`

- **决策**：momo-infra 的命令行工具唯一名字是 `momoctl`；子命令集：`doctor` / `pull [--check]` / `serve up|down|status|logs|restart` / `switch --to` / `bench` / `gc` / `web`（ADR-025）/ `config check`（ADR-026）。
- **理由**：与产品名 `momo` 及进程名 `momod` 区分开，避免把产品名直接当命令用的歧义；`kubectl`/`systemctl` 同类惯例。
- **被否**：以 `momo` 作 CLI 名（与应用主体/产品同名易混）；`momoctl models verify|gc` 独立子命令树（作废，verify=`pull --check`、gc=`gc`）。

### ADR-025 infra 扩展为「CLI + Web 控制台」，训练 job 由 infra 编排、models 执行

- **决策**：`momoctl web` 子命令起 axum 控制台（REST + SSE + 内嵌静态 SPA），覆盖组件起停/模型管理/训练 pipeline/env 配置四块运维面；训练 job 状态机（queued→preparing→running→succeeded/failed/canceled→promoted）由 infra `jobs/` 编排，执行器分本地子进程与 SSH 远程（4090D/mac-studio），训练实体脚本仍在 momo-models `training/`。CLI 与 Web 共用同一核心与 `--json` schema（详见 [`../02-repos/momo-infra.md`](../02-repos/momo-infra.md) Web 专节）。
- **理由**：用户明确需求（2026-10-11）——web 页面操作 infra、管理推理与训练、模型选择与微调 pipeline、常见配置；单 binary 内嵌 SPA 保住单文件分发与 bootstrap 独立性；训练实体与编排分离守住「infra 无推理」硬约束。
- **被否**：独立 momo-web 仓/进程（多一套分发与版本对齐，收益不足）；Gradio 式 Python UI（引入 Python 依赖，违背 infra bootstrap 独立约束）；训练编排放 momo-models（模型仓不该管进程/ssh/状态目录等机器运维）。

### ADR-026 `momoctl config check`：env schema 漂移校验 + 敏感项默认纪律

- **决策**：`momoctl` 新增 `config check` 子命令，只读比对 `config.schema.toml` ↔ 三侧 `.env.example` ↔ `config.md`（CI `ci-config.yml` 门禁）。schema 校验实装敏感项纪律：密钥型敏感项（`string`/`url`/`path`）`default` 必须为空；开关型敏感项（`bool`/`enum`/`int`，如 `MOMO_DATA_KEEP_AUDIO`）保留安全常量默认；两类 `examples` 均不得覆写为真实值（只允许等于 `default`）。
- **理由**：密钥/凭据不能入库，`default` 留空从源头杜绝默认值泄钥；`examples` 覆写会经 `expected_entries` 反向要求真实值写进 `.env.example`。开关型敏感项的"敏感"指产物隐私（保留音频），安全常量 `off` 正是要固化的默认，置空反而丢掉安全默认并破坏 enum 取值校验。
- **被否**：敏感项一律空默认（`MOMO_DATA_KEEP_AUDIO` 等开关型被迫无默认，enum 校验与"安全默认"语义双输）；敏感项 `examples` 放开覆写（`sk-...` 类真实值可入库）。

### ADR-027 终态仓库拆分：`app` → `momo`，`models`+`infra` → `momo-infra`

- **决策**：模块契约稳定后仓库按产品边界拆分——`app/`（含随行的 `site/` 与产品 `docs/`）独立为 **`momo`**（终端用户产品仓）；`models/` + `infra/` 合并为 **`momo-infra`**（自有 AI 基础设施仓）。拆分前 monorepo 内的目录边界即按终态仓库边界设计。
- **理由**：app 是给终端用户的产品（轻、可分发、provider 由用户配置）；momo-infra 是自有训练/serving 底座（重、面向自己）——两者的用户、发布节奏与依赖体量完全不同，终态必然分仓。
- **被否**：把 monorepo 定为终态（用户产品与重型 AI infra 混在一仓，分发与依赖都无法收敛）；现在立即拆仓（协议与边界未稳定，跨仓同步成本高——ADR-001 已述）。

### ADR-028 app 边界：单槽位 provider 配置，容灾在服务侧

- **决策**：app 面向终端用户，每类能力只持有**一个**可配置 endpoint——OpenAI-compatible LLM、ASR 服务、TTS 服务各一；Live2D 模型可替换（默认 Hiyori）。app **不内置**多 provider 容灾、内部降级编排或负载均衡——provider 不可用只做 UI 层呈现降级（字幕兜底、禁用语音输入等），多路容灾由服务侧或 momo-infra gateway 承担。
- **理由**：把降级编排放进 app 会让轻产品膨胀并与 provider 实现耦合；单一 endpoint 语义使「换服务 = 改配置」对用户和 app 都最简单。
- **被否**：app 内建多 provider 列表+故障切换（臃肿且与 momo-infra gateway 职责重叠）；app 内嵌推理（同 ADR-004 被否理由）。

### ADR-029 momo-infra：自有训练 + serving，复用开源底座，统一对外 gateway

- **决策**：`momo-infra` 是自有的 AI 基础设施——LLM 后训练、ASR/TTS 全训练 pipeline，以及模型 serving 能力。推理底座**复用开源实现**（vLLM 等），不手搓推理引擎/KV cache；自研部分收敛到**统一对外 gateway**：对内可配置多模型容灾与负载均衡，对外暴露单一接口；ASR/TTS 对外接口形态对齐主流云服务（REST/WS 标准协议），使 app 的 provider 配置同时兼容 momo-infra 与第三方云服务。
- **理由**：推理引擎是已被开源解决的深水区，自研无差异化价值；差异化价值在「自有模型的训练→评测→部署」闭环与一个对 app/用户透明的稳定入口。
- **被否**：自研推理引擎/KV cache（重复造轮）；把 gateway 编排放进 app（同 ADR-028）；ASR/TTS 用私有协议（抬高 app 与第三方服务的对接成本）。

## 架构与协议

### ADR-003 三段式进程分离 + 进程间 WebSocket

- **决策**：三层进程分工 + momo-ws 事件与音频协议；LLM 使用 OpenAI-compatible SSE。规范见 [../01-architecture/protocol.md](../01-architecture/protocol.md)。
- **理由**：三段可部署到不同机器（正常档/降级档拓扑不同）；WS 同时承载双向控制与流式音频，打断语义天然契合。
- **被否**：gRPC/REST（服务端流式 + 双向打断语义不顺手，且浏览器端音频二进制支持繁琐）；进程内单体式（无法跨机部署，mac-studio 收回时无降级路径）。

### ADR-004 编排器放 momo-app（Rust），模型推理放 momo-models（Python）

- **决策**：编排器（tokio+axum）与前端同属 `momo-app`；ASR/TTS/LLM 接入与训练在 `momo-models`；边界细则见 [../01-architecture/boundaries.md](../01-architecture/boundaries.md)。
- **理由**：推理代码迭代快、依赖重（torch/onnx 数 GB），独立成 Python 服务按需起停；编排器轻依赖、要随前端/桌面壳一起分发。
- **被否**：推理内嵌 Rust（ort/candle 对目标模型生态覆盖不足）；编排放 Python（回到 ADR-002 被否理由）。

### ADR-006 LLM 统一 OpenAI-compatible chat 接口抽象

- **决策**：大脑无论后端是 MLX（`mlx_lm.server`）、借用的 vLLM 还是 DeepSeek API，编排器一律走 OpenAI-compatible `/chat/completions` 流式接口；换脑=改 endpoint env。
- **理由**：三家后端原生同接口，热切换零代码；不重复造 LLM 私有协议。
- **被否**：把 LLM 也纳入 momo-ws 自定义消息（要为每个后端写适配层，违背"调用现有库"原则）。

### ADR-008 档位切换 = 改 env，不改代码

- **决策**：正常档（交互端 Mac Air/laptop + 编排器 laptop + 大脑 mac-studio MLX）与降级档（laptop 单机全栈 + 小模型/DeepSeek）用**同一套协议与代码**，只切 endpoint 配置。档位切换是 **momo-infra 运维面**职责（`momoctl switch`），不属于 app 的 provider 容灾（ADR-028）。
- **理由**：mac-studio 是借来的随时可能收回；bigstrong vLLM 实测 502 不通（2026-10-10）证明依赖外部必须有降级路径。
- **被否**：单独维护降级分支/构建（代码分叉即腐烂）；人工改代码切模型；在 app 内做多 endpoint 自动 failover（ADR-028：那是服务侧/gateway 职责）。

### ADR-013 回合状态机与 barge-in 三动作语义

- **决策**：`TurnCoordinator` 独占回合状态；`Speaking` 从 `playback.started` 开始。barge-in 先同步停本地播放/嘴型并原子失效旧回合，同时并行取消 LLM/TTS/句切/音频，绝不等待 ACK；随后按 playback ledger 结算确认听见的完整句与部分审计。capture 生命周期独立保留。细则见 [../01-architecture/runtime.md](../01-architecture/runtime.md)。
- **理由**：「能插话」是桌面陪伴的体验核心；ACK 账本让模型上下文不虚构用户听见的内容。
- **被否**：打断即整回合丢弃（用户听到的和模型记得的不一致）；排队制不打断（破坏"像人"第一指标）。

### ADR-019 为外部 ingress/egress 预留过滤边界

- **决策**：本地单用户 turn core 不绑定平台过滤业务；未来 streaming adapter 若接外部输入，必须在 adapter 边界实装输入/输出过滤与一键熔断。
- **理由**：外部内容的风险模型与本地 companion 不同，过滤必须在公开接入前完成，同时不能污染 turn core。
- **被否**：把平台过滤写进 turn core runtime；公开接入后再补。

### ADR-021 app 直连单一 LLM 端点；gateway 属 momo-infra

- **决策**：**app 侧不设 gateway**——编排器直连配置的 OpenAI-compatible endpoint（SSE），上游可为 vLLM / DeepSeek / llama.cpp / momo-infra gateway，对 app 只是 `MOMO_LLM_ENDPOINT` 指向不同。多模型容灾、负载均衡等 gateway 能力归属 **momo-infra**（ADR-029），对内可做，对外（对 app）仍是单一接口。
- **理由**：app 保持薄——一个 endpoint 就是全部认知；gateway 价值在多后端编排，那是 infra 的事，对 app 透明。
- **被否**：app 内置多 endpoint failover（ADR-028）；把 LLM 纳入 momo-ws 自定义消息（同 ADR-006 被否理由）。

### ADR-022 编排器与语音服务共享通信契约

- **决策**：编排器与语音服务使用同一份 momo-ws 契约，统一流标识、取消与播放回执；调试接口独立于实时对话通路。
- **理由**：打断、取消、流式音频、VAD 事件（`input.vad`）语义已在 momo-ws 定义；再立一套 REST 主协议要维护两份语义且打断语义难表达。
- **被否**：为模型服务另定义 REST `/v1/*` 作为运行时主协议。

## 模型选型

### ADR-007 主脑 = mac-studio MLX，Qwen3-8B-4bit 起步

- **决策**：主脑用 mac-studio 上 MLX 跑 Qwen3-8B-4bit（**实测** 2026-10-10：121 tok/s、峰值内存 4.7GB，已达标）；30B-A3B-4bit 实测达标后已升为主脑（见 ADR-024），8B-4bit 保留为备档。降级=laptop 本地小模型或 `deepseek-v4-flash`。机会型副脑=bigstrong 常驻 vLLM qwen3.8-27B-FP8（理论每人 30–45 tok/s，**实测 2026-10-10 服务 502 不通**，待主人修）。
- **理由**：512GB 统一内存跑 MoE 无压力；本地优先保护隐私。
- **被否**：laptop 常驻 8B（8GB 显存要与 TTS 抢）；云 API 做主脑（依赖网络+隐私，只当兜底）。

### ADR-009 ASR = SenseVoice-Small（sherpa-onnx）+ VAD = silero-vad

- **决策**：ASR 用 SenseVoice-Small 走 sherpa-onnx 纯 CPU（**实测** 2026-10-10：laptop CPU×4 RTF 0.040，8.4s→337ms；zh/en/ja/ko/yue + 自动语言检测 + 情感/事件标记）；VAD 用 silero-vad；**不训练**。
- **理由**：CPU 就够不占显卡，语言覆盖满足中/英/日要求。
- **被否**：Whisper turbo（~6GB VRAM 或 CPU 慢档；语言广度不够时再评估）；云端 ASR（违背本地优先）。MeloTTS 的否决见 ADR-010。

### ADR-010 TTS 梯队：CosyVoice3 首选 / GPT-SoVITS 备选 / Kokoro+F5 兜底 / edge-tts 仅过渡

- **决策**：目标是声线克隆型 TTS。首选 **Fun-CosyVoice3-0.5B**（9 语言含中日英、双向流式官方标称 150ms、instruct 情感控制、有训练脚本；实际部署需求**待实测**）；备选 **GPT-SoVITS**（MIT，zh/en/ja/ko/yue，官方标 4060Ti RTF 0.028，零样本 5s/少样本 1min，生态最成熟）；已实测兜底 **Kokoro-82M**（实测 2026-10-10：首包 209ms、RTF 0.023、VRAM 0.76GB）与 **F5-TTS v1**（实测：零样本 RTF 0.535、VRAM 0.8GB）；edge-tts（实测：首包 1937ms）仅作管线过渡。
- **理由**：克隆声线是产品目标；梯队保证每一档都有已验证退路。
- **被否**：**MeloTTS**（laptop 无 sudo，fugashi 依赖系统 MeCab 装不了——已否决）；edge-tts 长期用（1937ms 首包远超 1.5s 指标，且依赖外网）。

### ADR-011 Web 优先迭代，Tauri 后置复刻

- **决策**：**Web（momod + Web UI）是交互主迭代面**——所有对话、呈现与相处体验先在浏览器内定稿；Tauri 2 桌面宿主在交互验证完成后落地，**复刻同一套 Web 交互**（透明窗口、system menu、拖拽/缩放、置顶与可选 click-through），不与 Web 双轨并行。平台能力不支持时使用普通可见窗口，menu 不可用时禁用点击穿透。
- **理由**：Web 迭代成本远低于 Tauri；交互形态（字幕、打断、动效节奏）需要快速试错，先锁宿主等于给未定稿的交互上双份约束。桌面在场感仍是终态闭环的一部分，只是顺序后置。
- **被否**：Tauri 先行/并行（交互未定稿时双端维护，拖慢迭代；且当前实装的 momod+Web 已验证全链路可行）；放弃桌面宿主（在场感是产品闭环，终态仍需要）。

### ADR-012 Live2D = Pixi 7 adapter + 官方许可 sample；口型来自实际播放

- **决策**：复用 Live2D/Pixi 上游，通过自有 `AvatarAdapter`/`MotionDirector` 隔离；网站使用桃濑日和 Hiyori 官方 sample 作 demo，并保持模型与角色设计原样。口型由实际 AudioWorklet 播放 RMS 驱动。
- **理由**：先复用官方资源验证真实 Live2D，同时保持角色与 renderer 可替换。Runtime Core、sample 与应用代码许可证分别核验。
- **被否**：自研 Runtime；用假 CSS 角色替代实际 Live2D；让 emotion motion 覆盖 lipsync。

### ADR-014 长期记忆 = momo 自主管理 + 用户可审计可撤销

- **决策**：长期记忆由 **momo 自主管理**——自主归纳、整理、引用与遗忘，这是「像人」与自进化的核心，不做逐条人工确认制。用户侧保留完整监督面：查看记忆正文与来源、编辑、检索、删除，删除须同时清理正文和 vector/index。embedding 可复用 sqlite-vec + BGE-M3 adapter。
- **理由**：像人的核心是"她记得"，不是"用户替她记"；自进化要求记忆系统是角色自身能力而非用户管理负担。可解释与可撤销仍是底线，落在审计与撤销能力上而非确认门槛上。
- **被否**：逐条 proposal+确认（把记忆管理变成用户负担，阻断自进化）；只删 UI 记录但保留向量索引；外部常驻向量库。

### ADR-015 工具与截图逐次授权

- **决策**：工具/action 使用独立扩展消息组；每次请求向用户展示动作、范围和数据去向，只有 allow 后执行，deny/cancel/late-result 都有显式路径。
- **理由**：LLM 不直接执行；用户授权与结构化 schema 共同形成安全边界。
- **被否**：LLM 直接执行代码；本地可信即默认永久授权；把工具业务写进 turn core。

### ADR-016 截图 = 用户逐次授权的 provider adapter

- **决策**：截图先展示采集范围与数据去向；allow once 后交给可配置 vision provider，不自建 VLM。
- **理由**：屏幕内容敏感，授权边界比具体 provider 更稳定。
- **被否**：后台自动截图；把单一远端 provider 写死为产品能力。

### ADR-024 主脑定案 Qwen3-30B-A3B-4bit

- **决策**：主脑由 Qwen3-8B-4bit 升级为 **Qwen3-30B-A3B-4bit**，跑在 mac-studio MLX（`mlx_lm.server :7820`）。**实测 2026-10-10（M3 Ultra）：121 tok/s、峰值 17.2GB**。8B-4bit 同为 121 t/s，保留为备档/二级兜底。
- **理由**：MoE 激活 3B 使生成速度与 8B dense 持平而回答质量更高；512GB 统一内存无压力。
- **被否**：继续常驻 8B-4bit（同速但更弱，降为备档）；32B dense（同显存档下 MoE 更快）。

## 环境约束（硬事实，写文档/代码时视为前提）

### ADR-017 laptop 必须走 hf-mirror，只能 user-space 安装

- **决策**：laptop 访问 huggingface.co 不通，必须 `HF_ENDPOINT=https://hf-mirror.com`（mac-studio 直连慢，镜像同样更快）；PyPI 同理走镜像。laptop **无 sudo**，一切安装必须 user-space（uv/venv/cargo）。
- **理由**：网络实测结论（2026-10-10），不是偏好。
- **被否**：依赖系统包管理器的方案（无 sudo 不可行，MeloTTS 即因此否决）。

### ADR-018 torch 与 torchaudio 版本严格对齐

- **决策**：momo-models 中 torch/torchaudio 版本成对锁定（已踩坑）。
- **理由**：版本错位导致过实际故障。
- **被否**：交给解析器自由解析。

## 检查表

- [ ] 每条 ADR 含：决策、理由、被否选项
- [ ] 实测数据均标注「实测 + 日期」，未达标量标「预估/待实测」
- [ ] GPU 写法统一为 **RTX 3070 Laptop（8GB）**，无 Ti 后缀
- [ ] CosyVoice3 部署需求写「待实测」，不引用旧稿 ~5GB 未验证估计
- [ ] 其他文档只引用本文档结论，不重述理由
