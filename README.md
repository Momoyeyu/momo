# momo

让桌面多一个，懂得回应的存在。

momo 是一个有形象、有性格、有自己记忆的伙伴：她能听你说话，也懂得怎么陪你。本地优先；先在 Web 上把相处体验做透，再长出自己的桌面形态；直播是保留的远方。

主页与设计文档站：<https://momoyeyu.github.io/momo/>

## 现状

- **Web 交互闭环已跑通**：Live2D 形象（默认 Hiyori，可替换）、人格化对话、文字/按住说话、字幕口型、可打断回合、本地记录。
- **桌面壳后置**：交互在 Web 定稿后由 Tauri 复刻。
- 开发进度见 [工程状态](./docs/06-management/github-project.md#工程状态) 与 [任务看板](https://github.com/users/Momoyeyu/projects/2)。

## 仓库结构



## 产品方向

- 内核是「像人」：人格、情绪、像人的打断与留白；相处质感是目标，不是助手。
- momo 自管理的长期记忆 → 自进化；用户可查看/纠正/删除。
- provider 全部用户可配：OpenAI-compatible LLM、ASR、TTS 各一个 endpoint；Live2D 模型可替换（默认 Hiyori）。
- 容灾/负载均衡不属于 app，由服务侧或 momo-infra gateway 承担。
- 直播（streaming adapter）保留扩展接口，当前不交付。

## 约定速览

- **文档先行**：实现与 `docs/` 不一致时，先修正文档与决策。
- **Commit**：单行 `<type>: <what>`，type ∈ `feat|fix|refactor|docs|chore|test|build`，无 AI trailer。

## License

代码以 **MIT License** 发布（[LICENSE](./LICENSE.md)）。`site/docs/public/live2d/` 下的 Live2D sample 素材、Cubism Core 与模型权重等第三方资源按各自许可，不受本许可覆盖。
