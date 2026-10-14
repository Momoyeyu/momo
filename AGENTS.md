# AGENTS.md

## Git

- conventional commit：`<type>: <what changed>`，`type` in `feat|fix|refactor|docs|chore|test|build`，一行说完一件事。
- 提交信息只写改动本身，不含任何 AI 署名或 trailer。
- 栈式 PR（base 非 master）在整条链全部 merge 前，禁止删除链上任何分支的远端引用——删 head 会 close 该 PR，删 base 会 close 其全部下游 PR，且重建分支后不可 reopen。本地 worktree/分支清理不受此限。

## Review 纪律

- PR comment 只放结果与证据；禁止把验证动作甩给 reviewer（"请手动验证连通性"类评论）。
- 被资源卡住（API key、算力、账号权限、可达网络）→ 直接在会话中向用户索要，不在 PR/issue comment 里索要或绕开。
- PR 正文的「手动验证」小节是给 reviewer 的复现路径，前提是"已自测通过"；不能用它替代自己应跑的验证。

## 输出与日志

- 面向用户的输出一律英文：CLI 日志、错误提示、hint、报告文本（测试断言同样适用）。源码注释与 `docs/` 文档保持中文。

## 文档先行

- 设计以 `docs/` 为准；实现与文档不一致视为 bug，先改文档再改代码。
- 新增/修改 `MOMO_` env 键必须同步 `app/.env.example` 与 `docs/01-architecture/config.md`；机器可读全集 schema（`config.schema.toml`）与 `momoctl config check` 校验器在 momo-infra 仓。

## 跨平台

- 开发环境 macOS；部署目标 Linux + Windows。
- 禁止无条件使用 unix-only API/路径（kill -0、/tmp、shell 路径分隔）；需要时用 `cfg(unix)`/`cfg(windows)` 或 `sys.platform` 分支。
- TLS 一律 rustls（禁 openssl 依赖）；路径一律 PathBuf/path.join 拼接。

## 仓库边界

- 仓库已拆分（ADR-027）：本仓含 `app/`（交互+编排）、`site/`（页面）、`docs/`（设计）、`evals/persona`（人格回归集）；`models/` 与 `infra/` 已迁往 momo-infra 仓。模块边界口径见 `docs/01-architecture/boundaries.md`。
- 各子目录 `AGENTS.md` 有模块级约定，冲突时以更具体的为准。

## Site verification

- `cd site && pnpm test`
- `cd site && pnpm build`
- 文档正文以 `docs/` 为准，`site/docs/guide/` 通过 include 消费 canonical docs。
- browser smoke 覆盖 Live2D + Mermaid 的 light/dark/mobile。
