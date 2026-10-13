# momo

让桌面多一个，懂得回应的存在。

momo 是本地优先的 AI 桌宠：有形象，能听你说话，也能安静陪着你。以桌面陪伴为起点，让文字、声音、表情与记忆构成连贯的相处体验。

主页与设计文档站：<https://momoyeyu.github.io/momo/>

## 现状

- **桌面闭环设计已定稿**：Tauri 桌面宿主、Live2D 形象、文字/按住说话/连续语音、可打断回合、字幕口型、本地记录。
- **实现早期**：架构与协议文档先行，代码按 `docs/` 逐步落地。开发进度见 [工程状态](./docs/06-management/github-project.md#工程状态) 与 [任务看板](https://github.com/users/Momoyeyu/projects/2)。

## 仓库结构

| 目录 | 语言 | 内容 |
|---|---|---|
| `docs/` | MD | 产品、桌面、协议与操作路径的系统设计（设计权威） |
| `app/` | Rust + TS | 桌面宿主、形象与交互、回合编排与本地记录 |
| `models/` | Python | ASR/TTS/Embedding inference adapters + 训练/评估 |
| `infra/` | Rust | `momoctl` CLI + Web 运维控制台 |
| `site/` | VitePress + Vue | 产品主页、Live2D 互动预览与设计文档站 |

## 产品方向

- 桌面形象、文字与语音、可打断的回合组成基础交流闭环。
- 记忆、模式与个性化持续打磨，工具和更多场景按需接入；直播仅保留扩展接口。
- 产品默认中文；中、英、日输入输出按 provider capability 启用。
- 主页 Live2D 预览使用官方桃濑日和 Hiyori sample（© Live2D Inc.），模型与 Cubism Core 各自许可，见 [素材与许可](./docs/00-context/resources.md)。

## 约定速览

- **文档先行**：实现与 `docs/` 不一致时，先修正文档与决策。
- **Commit**：单行 `<type>: <what>`，type ∈ `feat|fix|refactor|docs|chore|test|build`，无 AI trailer。
- **跨平台**：macOS 开发，Linux + Windows 部署（models 的 Windows 走 WSL2）；rustls；`PathBuf`。
- **env**：`MOMO_*` 前缀；新增键同步 `*/.env.example` + `docs/01-architecture/config.md`。

## License

代码以 **PolyForm Noncommercial 1.0.0** 发布（[LICENSE](./LICENSE.md)）：

- 个人与非商业用途可自由使用、修改、分发。
- 商用或专利主张需另行取得授权（在 Issue 中联系）。
- `site/docs/public/live2d/` 下的 Live2D sample 素材与 Cubism Core 等第三方资源按各自许可，不受本许可覆盖。
