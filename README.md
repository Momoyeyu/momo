# momo

让桌面多一个，懂得回应的存在。

momo 是本地优先的 AI 桌宠：有形象，能听你说话，也能安静陪着你。以桌面陪伴为起点，让文字、声音、表情与记忆构成连贯的相处体验。

主页与设计文档站：<https://momoyeyu.github.io/momo/>

## 模块设计

| 目录 | 语言 | 内容 |
|---|---|---|
| `docs/` | MD | 产品、桌面、协议与操作路径的系统设计 |
| `app/` | Rust + TS | 桌面宿主、形象与交互、回合编排与本地记录 |
| `models/` | Python | ASR/TTS/Embedding inference adapters + 训练/评估 |
| `infra/` | Rust | `momoctl` CLI + Web 运维控制台 |
| `site/` | VitePress + Vue | 产品主页、Live2D 互动预览与设计文档站 |

## 产品方向

- 桌面形象、文字与语音、可打断的回合组成 P0 交流闭环。
- P1 继续打磨记忆、模式与个性化，P2 连接工具和更多场景。
- Haru sample 与 Cubism Core 使用 Live2D 各自许可，不属于仓库 MIT 代码许可。
- 产品默认中文；中、英、日输入输出按 provider capability 启用。

[开发进度](./docs/06-management/github-project.md#工程状态)

## 约定速览

- **文档先行**：实现与 `docs/` 不一致时，先修正文档与决策。
- **Commit**：单行 `<type>: <what>`，type ∈ `feat|fix|refactor|docs|chore|test|build`，无 AI trailer。
- **跨平台**：macOS 开发，Linux + Windows 部署（models 的 Windows 走 WSL2）；rustls；`PathBuf`。
- **env**：`MOMO_*` 前缀；新增键同步 `*/.env.example` + `docs/01-architecture/config.md`。

任务排期：[GitHub Project · 任务总览](https://github.com/users/Momoyeyu/projects/2)
