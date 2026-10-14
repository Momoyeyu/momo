# 配置规范：`MOMO_` 环境变量

> 一切环境差异与密钥走 `MOMO_` 前缀环境变量（ADR-005）。仓库只提交 `.env.example`（占位无值），本地 `.env` 不入库。
> 本文档收窄到 **app 产品面**：`models/`、`infra/` 消费的键不在此展开——机器可读全集已随拆分迁往 momo-infra 仓，以其 `config.schema.toml` 与各模块 `.env.example` 为准。
> 机器可读单源 schema：`config.schema.toml` 在 momo-infra 仓根（名称/类型/默认值/敏感/读取组件/各 `.env.example` 归属，见 §6）；本仓只保留 `app/.env.example` 与本文档。

## 1. 解析优先级

```mermaid
flowchart LR
    DEF["内置默认值<br/>(代码内常量)"] --> ENV["环境变量 / .env<br/>MOMO_*"] --> CLI["CLI flag<br/>--llm-endpoint 等"]
    CLI -.->|"最终生效值"| RUN["运行时"]
```

内置默认 < 环境变量 < CLI flag，高优先级覆盖低优先级。CLI flag 命名 = env 名去 `MOMO_` 前缀转 kebab-case（`MOMO_LLM_ENDPOINT` → `--llm-endpoint`）。启动时各组件把**生效值**打进日志（敏感项打码，见 §4）。

## 2. 变量全集（app 面）

### 2.1 通用（多组件共用，读取组件见表）

| 名称 | 含义 | 默认值 | 取值 | 读取组件 | 敏感 |
|---|---|---|---|---|---|
| `MOMO_ENV` | 运行环境标识 | `dev` | `dev`/`prod` | 全部 | 否 |
| `MOMO_LOG_LEVEL` | 日志级别 | `info` | `trace/debug/info/warn/error` | 全部 | 否 |
| `MOMO_LOG_FORMAT` | 日志输出格式 | `pretty` | `pretty`/`json` | 全部 | 否 |
| `MOMO_PROFILE` | 部署档位 | `normal` | `normal`/`degraded` | 服务侧运维面（决定启停拓扑与健康检查对象）；app 不直接读 | 否 |
| `MOMO_MODELS_DIR` | 模型文件根目录 | `~/.cache/momo/models` | path | 服务侧 | 否 |

### 2.2 编排器（app 读）

| 名称 | 含义 | 默认值 | 取值 | 读取组件 | 敏感 |
|---|---|---|---|---|---|
| `MOMO_ORCH_LISTEN` | WS + 静态 UI 监听地址 | `127.0.0.1:7800` | host:port | app | 否 |
| `MOMO_UI_DIR` | 前端静态目录（打包内嵌时忽略） | `./web/dist` | path | app | 否 |
| `MOMO_PERSONA_FILE` | persona card 路径；读不到回退内嵌 card，每回合重读（热加载） | `persona/momo.yaml` | path | app | 否 |
| `MOMO_WEB_ORIGIN` | 浏览器来源（CORS/WS Origin 白名单） | `http://localhost:5173` | URL | app | 否 |
| `MOMO_UI_TOKEN` | `/ws/ui` hello 的期望 `auth_token`；desktop 启动时注入，空=standalone dev 不比对（字段仍必填） | 空 | str | app | **是** |
| `MOMO_LLM_ENDPOINT` | 大脑 OpenAI-compatible base URL | `http://mac-studio.local:7820/v1` | URL | app | 否 |
| `MOMO_LLM_MODEL` | 传给 LLM endpoint 的 model 字段 | `qwen3-30b-a3b-4bit` | str | app | 否 |
| `MOMO_LLM_API_KEY` | `MOMO_LLM_ENDPOINT` 所指上游的鉴权 key；本地可空，远端如 DeepSeek 档必填 | 空 | str | app | **是** |
| `MOMO_LLM_TIMEOUT_MS` | LLM 请求总超时 | `30000` | ms | app | 否 |
| `MOMO_ASR_ENDPOINT` | ASR 服务 WS 地址 | `ws://127.0.0.1:7810/ws` | ws:// | app | 否 |
| `MOMO_TTS_ENDPOINT` | TTS 服务 WS 地址 | `ws://127.0.0.1:7811/ws` | ws:// | app | 否 |
| `MOMO_EMBED_ENDPOINT`（记忆） | embed 服务 WS 连接地址 | `ws://127.0.0.1:7812/ws` | ws:// | app | 否 |
| `MOMO_BARGEIN_ENABLED` | 允许打断 | `true` | bool | app | 否 |
| `MOMO_BARGEIN_MIN_MS` | 打断防抖：`input.vad{speech_start}` 须持续该时长才触发 barge-in | `250` | ms | app | 否 |
| `MOMO_SESSION_RESUME_TTL_MS` | WS 断开后 session 保活窗口；TTL 内 `hello.resume_session_id` 命中则复用并下发 `session.snapshot`，超时回收 | `60000` | ms | app | 否 |
| `MOMO_IDLE_TIMEOUT_S` | 空闲多少秒触发自发行为 | `20` | s，0=关闭 | app | 否 |
| `MOMO_SENT_SOFT_CHARS` | 句切弱切阈值 | `25` | 字符数 | app | 否 |
| `MOMO_SENT_MAX_CHARS` | 句切硬上限 | `60` | 字符数 | app | 否 |
| `MOMO_HISTORY_TURNS` | 注入 LLM 的历史回合数 | `20` | int | app | 否 |
| `MOMO_FILTER_ENABLED` | 未来外部 ingress/egress 过滤开关（streaming adapter 前置） | `false` | bool | app | 否 |
| `MOMO_DATA_COLLECTION` | 会话采集开关（语料落盘供训练/微调） | `on` | `on`/`off` | app | 否 |
| `MOMO_DATA_KEEP_AUDIO` | 原始麦音频短期保留（仅显式开启且授权时，供 ASR 排错） | `off` | `on`/`off` | app | **是** |
| `MOMO_EMOTION_MAP` | emotion tag → Live2D 表情映射文件路径 | `config/emotion.map.yaml` | path | app | 否 |

上游凭据只有一套，按**端点**而非厂商划分：`MOMO_LLM_API_KEY` 就是 `MOMO_LLM_ENDPOINT` 所指上游的 key——把 endpoint 改成 `https://api.deepseek.com/v1` 即切到 DeepSeek，不需要新键（ADR-006 统一 OpenAI 兼容接口；ADR-016 已否掉「把单一远端 provider 写死为产品能力」）。

### 2.3 服务侧变量（迁出说明）

`models/`（ASR/TTS/embedding/vision 服务监听、后端、模型与设备选择）与 `infra/`（`momoctl` 大脑启停、控制台鉴权）消费的 `MOMO_` 键，以及 `HF_ENDPOINT` 等第三方库直通变量，不在本仓文档面展开；已随拆分迁往 momo-infra 仓，全集见其 `config.schema.toml`、对应模块 `.env.example` 与 `docs/01-architecture/config.md`。

## 3. `.env.example`（提交入库的完整模板）

```dotenv
# ===== momo .env.example —— 复制为 .env 后填值；.env 不入库 =====
# 通用
MOMO_ENV=dev
MOMO_LOG_LEVEL=info
MOMO_LOG_FORMAT=pretty         # pretty | json
MOMO_PROFILE=normal            # normal | degraded（档位=改 env 不改代码）
MOMO_MODELS_DIR=~/.cache/momo/models

# 编排器（momo-app / momod）
MOMO_ORCH_LISTEN=127.0.0.1:7800
MOMO_UI_DIR=./web/dist
MOMO_PERSONA_FILE=persona/momo.yaml
MOMO_WEB_ORIGIN=http://localhost:5173   # 浏览器来源白名单（vite dev）
MOMO_UI_TOKEN=               # 敏感：/ws/ui hello auth_token；desktop 启动注入，空=dev 不校验
MOMO_LLM_ENDPOINT=http://mac-studio.local:7820/v1
MOMO_LLM_MODEL=qwen3-30b-a3b-4bit
MOMO_LLM_API_KEY=              # 敏感：本地可空；远端上游（如 DeepSeek）时填 sk-...
MOMO_LLM_TIMEOUT_MS=30000
MOMO_ASR_ENDPOINT=ws://127.0.0.1:7810/ws
MOMO_TTS_ENDPOINT=ws://127.0.0.1:7811/ws
MOMO_EMBED_ENDPOINT=ws://127.0.0.1:7812/ws   # 记忆：编排器→embed 服务
MOMO_BARGEIN_ENABLED=true
MOMO_BARGEIN_MIN_MS=250        # speech_start 持续该时长才触发打断
MOMO_SESSION_RESUME_TTL_MS=60000   # 断线 session 保活窗口；TTL 内 resume_session_id 可恢复
MOMO_IDLE_TIMEOUT_S=20
MOMO_SENT_SOFT_CHARS=25
MOMO_SENT_MAX_CHARS=60
MOMO_HISTORY_TURNS=20
MOMO_FILTER_ENABLED=false
MOMO_DATA_COLLECTION=on        # 会话采集开关（语料落盘）
MOMO_DATA_KEEP_AUDIO=off       # 敏感产物：授权排错时短期保留原始麦音频
MOMO_EMOTION_MAP=config/emotion.map.yaml   # tag → Live2D 表情映射
```

> 模板中 `models/`、`infra/` 消费的键段与上游直通变量已随拆分迁往 momo-infra 仓，以 `models/.env.example`、`infra/.env.example` 现状为准；本仓只剩 `app/.env.example`。

配置覆盖记忆、语音和视觉服务；具体能力按产品优先级启用。

## 4. 敏感项与安全

- **敏感清单（app 面）**：`MOMO_LLM_API_KEY`、`MOMO_UI_TOKEN`、`MOMO_DATA_KEEP_AUDIO`（后续新增一律按"值即密钥/隐私开关"判断；服务侧敏感项随拆分由 momo-infra 仓登记）。
- **敏感数据（非 key）**：`MOMO_DATA_KEEP_AUDIO=on` 时落盘的原始麦音频属隐私数据——key 本身只是开关，但其产物按敏感数据同等纪律处理（短期保留、仅 ASR 排错用途、到期清理、不外传）。
- `.env` 已在各仓 `.gitignore`；`.env.example` 中敏感项**只留空值占位**，禁止提交真实 key。
- 日志/诊断输出对敏感值打码（显示前 4 位 + `…`）；诊断工具只做"是否设置"检查，不回显。
- 组件间不转发敏感变量：`MOMO_LLM_API_KEY` 由 app 持有并注入请求；服务侧组件的凭据持有与打码纪律见 momo-infra 仓。

## 5. 与配置发布平台的兼容

UI 偏好（窗口位置、scale、置顶、穿透、输入模式、设备、companion mode）保存在本地 settings，不新增或覆盖本页 env 默认值。

- 配置面是**扁平 key=value 环境变量**，无嵌套配置文件——任何支持 env 注入的平台（systemd/Docker/k8s/自研发布平台）都可直接消费。
- 平台对接约定：以 momo-infra 仓 `config.schema.toml` 的 key 集合为 schema；敏感项由平台密钥管理注入，不写文件。
- 配置草稿的 validate → test → atomic apply / rollback 属服务侧控制台职责（momo-infra），不在 app 面。
- 新增变量流程：同一 PR 内更新 `config.md` 表格 + `app/.env.example` + 代码默认值；schema 键集（`config.schema.toml`）在 momo-infra 仓，跨仓变更须另行同步。
- 配置变更通过改 env + 重启生效。

## 6. 机器可读 schema：`config.schema.toml`

`config.schema.toml` 是 `MOMO_*` 键的权威来源（single source of truth），已随拆分迁往 **momo-infra 仓根**——校验器 `momoctl config check` 同在该仓（`infra/`），本仓不再持有 schema 文件与校验器，§2/§3 与 `app/.env.example` 的手工同步纪律不变。schema **不是生成器**：没有从 schema 产出本文档或 `.env.example` 的代码；校验器只做**比对**，不一致即失败，但不会替你改。字段口径如下，写全才比对得了：

```toml
schema = 1            # 文件格式版本
key = [ { name, type, values?, default, sensitive?, readers, files, examples?, component, description } ]
passthrough = [ { name, type?, default, files, description } ]   # 非 MOMO_ 前缀，第三方库自读
```

| 字段 | 口径 |
|---|---|
| `name` | env 键名，`MOMO_` 前缀；非前缀变量进 `passthrough` |
| `type` | `string`/`bool`/`int`/`path`/`url`/`enum`；供配置页渲染与校验用 |
| `values` | `type=enum` 时的合法取值集合 |
| `default` | 内置默认值；空字符串 = 无默认/留空占位。**密钥型敏感项（`string`/`url`/`path`）一律为空**；开关型敏感项（`bool`/`enum`/`int`，如 `MOMO_DATA_KEEP_AUDIO`）保留安全常量默认——"敏感"指产物隐私而非值即密钥 |
| `sensitive` | 仅 `true` 时书写；日志/诊断打码、配置页掩码的判据 |
| `readers` | 代码中消费该键的模块：`app`/`models`/`infra` |
| `files` | 该键必须以非注释行出现在哪些 `<mod>/.env.example` |
| `examples` | 可选：某文件示例值偏离 `default` 时的覆盖；**敏感项不得覆写真实值**（只允许等于 `default`），真实 key/开关取值一律走本地 `.env` |
| `component` | 配置页分 section：`common`/`app`/`models`/`infra`；本文档只展开 app 面小节，models/infra 键已迁出 |
| `description` | 含义，与 §2「含义」列保持一致 |

`passthrough.files = []` 表示该变量只允许以注释形式出现在 `.env.example`。

## 检查表

- [ ] 每个 app 面变量有：含义/默认值/取值范围/读取组件/敏感标记
- [ ] §2 表格/§3 模板与 `app/.env.example` 一致；与 momo-infra 仓 `config.schema.toml` 的 app 面键集对齐（`momoctl config check` 在 momo-infra 仓运行）
- [ ] 默认值与 protocol.md、runtime.md 中的数字一致（端口、队列、句切阈值、VAD）
- [ ] 敏感项在日志与诊断输出中打码
- [ ] models/infra 侧键不在本文档展开，拆出后以 momo-infra 仓文档为准
