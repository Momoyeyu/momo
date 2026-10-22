# app/AGENTS.md

- frontend/ 是 React 游戏界面；backend/ 是唯一业务编排；src-tauri/ 负责窗口、单实例、受控 sidecar 与发布打包。
- 以迁入的已验收行为和测试为基线，不恢复 app/web 或旧 Rust API。
- 新增 MOMO_ 配置同步 app/.env.example、docs/development/configuration.md。
- runtime API 必须覆盖 REST、SSE、WS、上传、探测和资源访问；桌面令牌不进入 URL、普通日志或用户配置。
- 开发数据、冻结构建输入与安装资源分别存放，不复用用户真实数据库做测试。
