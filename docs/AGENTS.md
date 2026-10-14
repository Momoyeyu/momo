# AGENTS.md

## Git

- conventional commit：`<type>: <what changed>`，`type` in `feat|fix|refactor|docs|chore|test|build`，一行说完一件事。
- 提交信息只写改动本身，不含任何 AI 署名或 trailer。
- 新增/修改 env 键必须同 PR 更新 `01-architecture/config.md` 表格与 `.env.example`；`config.schema.toml` 全集 schema 在 momo-infra 仓，跨仓键集变更须另行同步。
- 改协议/状态机先改 `01-architecture/`，再改引用它的文档，同一 PR 内完成同步。
