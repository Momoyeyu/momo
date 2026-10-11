# momo-app

应用主体：Rust 编排器 momod + TypeScript 薄前端（Live2D/音频 IO）。短期 Web，长期 Tauri 2 壳。

目标平台：Linux + Windows（开发环境 macOS）。

设计文档：<https://github.com/Momoyeyu/momo-docs>

## 开发

```bash
# 配置（占位模板，复制后按需改 endpoint）
cp .env.example .env

# 编排器 momod（默认 127.0.0.1:7800）
cargo run --bin momod
curl http://127.0.0.1:7800/healthz        # → 200

# 前端（vite dev，/ws 代理到 momod）
cd web
pnpm install
pnpm dev                                # → http://localhost:5173
```

检查：

```bash
cargo fmt --all && cargo clippy --all-targets -- -D warnings && cargo test
cd web && pnpm typecheck && pnpm lint && pnpm test && pnpm build
```
