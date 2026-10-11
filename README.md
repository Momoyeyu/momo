# momo

本地优先的 AI 虚拟伙伴 —— Live2D 形象、语音对话、能被打断、有自己的人格与记忆。Monorepo，文档先于代码。

主页与设计文档站：<https://momoyeyu.github.io/momo/>

## 目录

| 目录 | 语言 | 内容 |
|---|---|---|
| `docs/` | MD | 全部设计文档（权威来源） |
| `app/` | Rust + TS | `momod` 编排器 + Web/Tauri 交互端 |
| `models/` | Python | ASR/TTS/embed 模型服务 + 训练/评测 |
| `infra/` | Rust | `momoctl` CLI + Web 运维控制台 |
| `site/` | VitePress | 产品主页 + 设计文档站（GitHub Pages） |

## 约定速览

- **文档先行**：实现与 `docs/` 不一致视为 bug，先改文档再改代码
- **Commit**：单行 `<type>: <what>`，type ∈ `feat|fix|refactor|docs|chore|test|build`，无 AI trailer
- **跨平台**：macOS 开发，Linux + Windows 部署（models 的 Windows 走 WSL2）；rustls；`PathBuf`
- **env**：`MOMO_*` 前缀；新增键同步 `*/.env.example` + `docs/01-architecture/config.md`

任务排期：[GitHub Project · 任务总览](https://github.com/users/Momoyeyu/projects/2)
