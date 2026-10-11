# 仓库与协作

单一 monorepo：[`Momoyeyu/momo`](https://github.com/Momoyeyu/momo)，默认分支 `master`，按目录划分模块。

| 目录 | 语言 | 内容 | CI |
|---|---|---|---|
| `docs/` | MD | 全部设计文档（权威来源） | — |
| `app/` | Rust + TS | `momod` 编排器 + Web 前端 | fmt/clippy/test + web ✓ |
| `models/` | Python | ASR/TTS/embed 服务 + 训练 | ruff/pytest ✓ |
| `infra/` | Rust | `momoctl` CLI + Web 控制台 | fmt/clippy/test ✓ |
| `site/` | VitePress | 本站（产品主页 + 设计文档） | Pages 自动部署 ✓ |

## 开发约定

- **文档先行**：实现与 `docs/` 不一致视为 bug，先改文档再改代码
- **Commit**：单行 `<type>: <what>`，`type` ∈ `feat|fix|refactor|docs|chore|test|build`，无 AI trailer
- **跨平台**：macOS 开发 / Linux+Windows 部署（`models/` 的 Windows 走 WSL2）；禁无条件 unix-only API，路径用 `PathBuf`/`path.join`，TLS 一律 rustls
- **env 纪律**：新增 `MOMO_*` 键同步 `*/.env.example` + `docs/01-architecture/config.md` + 实现；敏感值永不入库
- **模块边界**：见 `docs/01-architecture/boundaries.md`——infra 不含推理、不在数据通路；app 不含推理实现

## 二次开发入口

1. 先读 [总体架构](/guide/architecture) 与 [协议](/guide/protocol)
2. clone `Momoyeyu/momo`，按各目录 README 起本地环境；`momoctl doctor` 检查依赖与端点
3. 在 [Project 看板](https://github.com/users/Momoyeyu/projects/2) 认领 issue（P0 优先）
4. 改架构/协议/配置 → 先改 `docs/` 再改代码（同一仓库内一次 PR 即可）
