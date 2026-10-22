# momo backend

Python / FastAPI 负责对话、记忆、存档、内置 ASR / embedding 和可选 TTS / MCP。Tauri 将它冻结为受控 sidecar；本轮不将业务重写为 Rust。

安装构建、运行边界和依赖准备见 [桌面迁移](../../docs/development/migration.md)，环境变量见 [运行配置](../../docs/development/configuration.md)。浏览器开发时显式指定隔离数据目录与本地端口；`desktop.py` 仅接受受控壳提供的随机凭据、资源与数据目录。

```sh
uv sync --frozen
MOMO_DATA_DIR=/tmp/momo-browser-test uv run uvicorn app.main:app --host 127.0.0.1 --port 7822
uv run --frozen pytest -q
```

接口以代码和 `openapi.json` 为准；修改 API 后执行 `uv run python scripts/export_openapi.py`，再在 `../frontend` 执行 `pnpm gen:api`。`test_openapi.py` 校验快照一致性。桌面 API 包括文档路由均须鉴权。

内置模型权重由构建脚本按固定清单准备，不在运行时联网下载。默认测试使用替身，真实离线模型用例需要显式 opt-in；不要把普通测试通过视作原生安装包或 GPU 语音验证通过。配置、密钥与本地数据不提交。
