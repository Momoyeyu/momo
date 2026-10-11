# momo-page

momo 产品主页 + 设计文档站（GitHub Pages）：产品介绍、架构主图与各功能流程图、开发指南。

- 框架：VitePress（`pnpm dev` 预览 / `pnpm build` 构建到 `docs/.vitepress/dist`）
- 图表：`.archify/` 下由 archify 生成的独立交互 HTML，产出物拷贝到 `docs/public/diagrams/` 后以 iframe 内嵌
- 权威设计来源：https://github.com/Momoyeyu/momo-docs
- 部署：push 到 `master` 触发 GitHub Actions → Pages，base `/momo/`
