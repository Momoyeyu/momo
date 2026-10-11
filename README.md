# momo

让桌面多一个，懂得回应的存在。

momo 是本地优先的 AI 桌宠：有形象，能听你说话，也能安静陪着你。目标产品是 Tauri 2 桌面 companion；当前 app/models 代码仍是 `momo-ws v1` handshake/scaffold，完整 P0 与协议 v2 见设计文档。

主页与设计文档站：<https://momoyeyu.github.io/momo/>

## 目录

| 目录 | 语言 | 内容 |
|---|---|---|
| `docs/` | MD | 产品、桌面、协议与操作路径的权威设计 |
| `app/` | Rust + TS | 当前 runtime scaffold；目标包含 Tauri host、presentation 与本地 repository |
| `models/` | Python | ASR/TTS/Embedding inference adapters + 训练/评估 |
| `infra/` | Rust | `momoctl` CLI + P2 Web 运维控制台 |
| `site/` | VitePress + Vue | 产品主页、Live2D 固定 demo 与 canonical 文档站 |

## 当前边界

- P0 / P1 / P2 是产品优先级，不是已完成状态。
- 浏览器是网站与开发 preview；P0 产品宿主规划为 `app/src-tauri`。
- Haru sample 与 Cubism Core 使用 Live2D 各自许可，不属于仓库 MIT 代码许可。
- 产品默认中文；中、英、日输入输出按 provider capability 启用。

## 约定速览

- **文档先行**：实现与 `docs/` 不一致时，先修正文档与决策。
- **Commit**：单行 `<type>: <what>`，type ∈ `feat|fix|refactor|docs|chore|test|build`，无 AI trailer。
- **跨平台**：macOS 开发，Linux + Windows 部署（models 的 Windows 走 WSL2）；rustls；`PathBuf`。
- **env**：`MOMO_*` 前缀；新增键同步 `*/.env.example` + `docs/01-architecture/config.md`。

任务排期：[GitHub Project · 任务总览](https://github.com/users/Momoyeyu/projects/2)
