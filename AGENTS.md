# AGENTS.md

## Git

- conventional commit：`<type>: <what changed>`，`type` in `feat|fix|refactor|docs|chore|test|build`，一行说完一件事。
- 提交信息只写改动本身，不含任何 AI 署名或 trailer。

## 文档先行

- 设计以 `docs/` 为准；实现与文档不一致视为 bug，先改文档再改代码。
- 新增/修改 `MOMO_` env 键必须同步对应模块 `.env.example` 与 `docs/01-architecture/config.md`。

## 跨平台

- 开发环境 macOS；部署目标 Linux + Windows（`models/` 的 Windows 走 WSL2）。
- 禁止无条件使用 unix-only API/路径（kill -0、/tmp、shell 路径分隔）；需要时用 `cfg(unix)`/`cfg(windows)` 或 `sys.platform` 分支。
- TLS 一律 rustls（禁 openssl 依赖）；路径一律 PathBuf/path.join 拼接。

## Monorepo

- 模块边界见 `docs/01-architecture/boundaries.md`：`app/`（交互+编排）、`models/`（模型服务+训练）、`infra/`（运维）、`site/`（页面）、`docs/`（设计）。
- 各子目录 `AGENTS.md` 有模块级约定，冲突时以更具体的为准。
