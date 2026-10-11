# 配置（MOMO_*）

所有运行参数走 `MOMO_` 前缀环境变量；`.env` 仅本地，仓内只有占位 `.env.example`。新增 env 键必须同步三处：`.env.example`、`config.md`、实现代码。

## 端点

| 键 | 默认 | 说明 |
|---|---|---|
| `MOMO_ORCH_LISTEN` | `127.0.0.1:7800` | momod WS + 静态托管 |
| `MOMO_UI_DIR` | `./web/dist` | 前端静态目录 |
| `MOMO_LLM_ENDPOINT` | `http://mac-studio.local:7820/v1` | 大脑 OpenAI-compat base URL |
| `MOMO_LLM_MODEL` | `qwen3-30b-a3b-4bit` | 传给大脑的 model 字段 |
| `MOMO_LLM_API_KEY` | 空 | **敏感**；DeepSeek 档必填 |
| `MOMO_ASR_ENDPOINT` | `ws://127.0.0.1:7810/ws` | ASR 服务 |
| `MOMO_TTS_ENDPOINT` | `ws://127.0.0.1:7811/ws` | TTS 服务 |
| `MOMO_EMBED_ENDPOINT` | `ws://127.0.0.1:7812/ws` | 记忆检索（P3） |

## 模型与声线

| 键 | 默认 | 说明 |
|---|---|---|
| `MOMO_MODELS_DIR` | `~/.cache/momo/models` | 权重根目录 |
| `MOMO_TTS_PROVIDER` | `kokoro` | edge / kokoro / cosyvoice3 / sovits / f5 |
| `MOMO_TTS_MODEL_DIR` | — | SFT 产物目录（声线切换=改它） |
| `MOMO_TTS_VOICE_REF` | — | 零样本参考音 |
| `MOMO_PROFILE` | `normal` | normal / degraded |

## 运行时

| 键 | 默认 | 说明 |
|---|---|---|
| `MOMO_IDLE_TIMEOUT_S` | `20` | 自发开口计时，0=关闭 |
| `MOMO_BARGEIN_MIN_MS` | `250` | 打断防抖 |
| `MOMO_SENT_SOFT_CHARS` / `MOMO_SENT_MAX_CHARS` | `25` / `60` | 句切软/硬上限 |

## 运维与其他

| 键 | 默认 | 说明 |
|---|---|---|
| `MOMO_DATA_COLLECTION` | `off` | 会话采集开关（人格轨道数据源） |
| `MOMO_DATA_KEEP_AUDIO` | `off` | 原始麦音频落盘（敏感产物） |
| `MOMO_WEB_TOKEN` | 空 | **敏感**；`momoctl web --listen` 出 localhost 时必填 |
| `MOMO_DEEPSEEK_API_KEY` | 空 | **敏感** |

::: info 权威来源
[momo-docs/01-architecture/config.md](https://github.com/Momoyeyu/momo-docs/blob/master/01-architecture/config.md) — 全量键表、degraded 覆盖样例、敏感清单。
:::
