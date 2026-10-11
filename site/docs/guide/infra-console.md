# 训练 job 与运维控制台

`momoctl web`（`{b[:-1]}/`，P2）：axum 内嵌 SPA 的运维控制台——组件起停、模型激活、训练 pipeline、env 配置，浏览器可操作。CLI 与 Web 共用同一核心与同一份 job store。

## 训练 job 生命周期

<DiagramFrame src="train-job.html" :height="600" />

- **job store**：`jobs/<job_id>/{job.json,logs/,artifacts/}`；重启后 runner 重扫恢复 running 态监控（参照 LlamaBoard resume 语义）
- **执行器**：`exec_local`（spawn + pidfile + 日志落盘）或 `exec_ssh`（4090D/mac-studio，rsync 日志与 checkpoint 回传；复用 `~/.ssh/config`）
- **借用窗口**：`borrowed_night_only` 标记的 job 只在配置窗口调度

## 两条内置 pipeline

| pipeline | 步骤 | 终点 |
|---|---|---|
| `voice-sft` | 参考音频 → prepare_dataset → sft_cosyvoice3 → 盲听 A/B | `MOMO_TTS_MODEL_DIR` 激活 |
| `persona-qlora` | 对话素材 → build_dataset → lora_qwen → regression-20 | 评估报告 + LoRA 挂载 |

训练实体脚本在 `models/` `training/`；infra 只做编排与生命周期——**infra 无推理**。

## 控制台页面

| 页 | 内容 | API |
|---|---|---|
| 概览 | doctor 矩阵可视化 + 组件健康 | `GET /api/doctor` |
| 组件 | up/down/restart + SSE 日志 + normal/degraded 切换 | `POST /api/serve/*` |
| 模型 | MANIFEST 列表、pull 进度、激活选择 | `POST /api/models/activate` |
| 训练 | pipeline 向导、job 列表、实时日志/loss、promote | `GET/POST /api/jobs` |
| 配置 | `MOMO_*` env 编辑（敏感掩码、`.bak` 回滚） | `GET/PUT /api/config` |

## 安全边界

默认只绑 `127.0.0.1`；`--listen` 放宽才出 localhost，届时强制 `MOMO_WEB_TOKEN`（未设拒绝启动）；POST 走 CSRF token；敏感值永不回显。**控制台只做运维面，不碰对话/人设业务。**

::: info 权威来源
[docs/02-repos/momo-infra.md](https://github.com/Momoyeyu/momo/blob/master/docs/02-repos/momo-infra.md) Web 控制台专节 · ADR-025。
:::
