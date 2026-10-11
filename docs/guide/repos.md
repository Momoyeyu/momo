# 仓库与协作

四个独立 public 仓库，`master` 为默认分支；`momo/` 本地目录只是整合文件夹，不是 monorepo。

| 仓库 | 语言 | 内容 | CI |
|---|---|---|---|
| [momo-docs](https://github.com/Momoyeyu/momo-docs) | MD | 全部设计文档（权威来源） | — |
| [momo-app](https://github.com/Momoyeyu/momo-app) | Rust+TS | `momod` 编排器 + Web 前端 | fmt/clippy/test + web ✓ |
| [momo-models](https://github.com/Momoyeyu/momo-models) | Python | ASR/TTS/embed 服务 + 训练 | ruff/pytest ✓ |
| [momo-infra](https://github.com/Momoyeyu/momo-infra) | Rust | `momoctl` CLI + Web 控制台 | fmt/clippy/test ✓ |

## 开发约定

- **文档先行**：实现与 momo-docs 不一致视为 bug，先改文档再改代码
- **Commit**：单行 `<type>: <what>`，`type` ∈ `feat|fix|refactor|docs|chore|test|build`，无 AI trailer
- **跨平台**：macOS 开发 / Linux+Windows 部署（momo-models 的 Windows 走 WSL2）；禁无条件 unix-only API，路径用 `PathBuf`/`path.join`，TLS 一律 rustls
- **env 纪律**：新增 `MOMO_*` 键同步 `.env.example` + `config.md` + 实现；敏感值永不入库

## 二次开发入口

1. 先读 [总体架构](/guide/architecture) 与 [协议](/guide/protocol)
2. 按仓库 README 起本地环境；`momoctl doctor` 检查依赖与端点
3. 在 [Project 看板](https://github.com/users/Momoyeyu/projects/2) 认领 issue（P0 优先）
4. 改架构/协议/配置 → 先提 momo-docs PR
