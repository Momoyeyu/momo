# AGENTS.md

## Git

- conventional commit：`<type>: <what changed>`，`type` in `feat|fix|refactor|docs|chore|test|build`，一行说完一件事。
- 提交信息只写改动本身，不含任何 AI 署名或 trailer。

## 文档先行

- 设计以 momo-docs 为准；实现与文档不一致视为 bug，先改文档再改代码。
- 新增/修改 `MOMO_` env 键必须同步更新本仓 `.env.example` 与 momo-docs 的 `01-architecture/config.md`。
