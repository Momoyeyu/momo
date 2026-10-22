<p align="center">
  <img src="docs/assets/momo-readme-banner.svg" width="100%" alt="momo：墨紫夜色里的纸影与星轨">
</p>

<h1 align="center">momo</h1>
<p align="center"><strong>在这里，慢慢熟悉彼此。</strong><br>一款以相处、记忆与共同成长为方向的 AI 陪伴游戏。</p>

<p align="center">
  <strong>简体中文</strong> · <a href="README_EN.md">English</a> · <a href="README_JA.md">日本語</a>
</p>
<p align="center">
  <a href="https://momoyeyu.github.io/momo/">认识 momo</a> ·
  <a href="docs/usage/getting-started.md">开始指南</a> ·
  <a href="https://github.com/Momoyeyu/momo/releases">版本状态</a> ·
  <a href="CONTRIBUTING.md">一起参与</a>
</p>
<p align="center">
  <a href="https://github.com/Momoyeyu/momo/actions/workflows/ci-app.yml"><img src="https://github.com/Momoyeyu/momo/actions/workflows/ci-app.yml/badge.svg?branch=master" alt="应用检查状态"></a>
  <a href="https://github.com/Momoyeyu/momo/actions/workflows/ci-site.yml"><img src="https://github.com/Momoyeyu/momo/actions/workflows/ci-site.yml/badge.svg?branch=master" alt="官网检查状态"></a>
  <a href="LICENSE.md"><img src="https://img.shields.io/badge/code-MIT-E7BE77?labelColor=28232E" alt="项目代码：MIT 许可"></a>
</p>

## 从一句「今天过得怎么样」开始

不必准备一个好问题。和 momo 讲讲今天，听她接一句话，看她用表情和动作回应。下次回来时，名字、喜好和一起聊过的小事，可以留在记忆里。

momo 有一份稳定的核心人格，也为共同经历留出空间。我们希望做的是可以慢慢相处的游戏：你能回顾、存档、改变设置，也能决定哪些事值得记住。

<p align="center">
  <a href="https://momoyeyu.github.io/momo/"><img src="site/docs/public/media/first-meeting.png" width="100%" alt="momo 的真实初次见面画面：暖色房间里，日和站在中央，询问该怎么称呼你。"></a>
</p>
<p align="center"><a href="https://momoyeyu.github.io/momo/">在官网观看真实无声演示，翻一翻相处片刻</a></p>

<sub>画面使用隔离演示数据与 Hiyori 示例模型，不含私人对话或配置。[素材来源与说明](site/docs/public/media/asset-notice.txt)。</sub>

## 现在，可以怎样相处

| 体验 | 已有的内容 |
| --- | --- |
| **初次见面** | 主动打招呼、询问称呼，之后继续自己的对话。 |
| **文字或麦克风** | 打字，也可以直接说话；语音识别随应用提供。 |
| **有表情的回应** | Live2D 形象、表情与动作；可选接入 GPT-SoVITS，听见语音回复并打断、继续。 |
| **记忆由你决定** | 查看、编辑或删除记忆，修正记错的小事。 |
| **留下与带走** | 对话回顾、存读档和数据导出 / 导入，保留可恢复的相处记录。 |
| **你的相处方式** | 更换形象、配置动作，在中文 / English / 日本語间切换，连接可信的拓展工具。 |

让长期相处更自然、让共同经历带来更细腻的回应，是接下来继续打磨的方向。当前可体验的能力以版本说明为准。

## 开始相遇

**macOS 候选已完成安装产物与真实语音联调验证，目前尚无公开发布的安装包。** 查看 [版本页面](https://github.com/Momoyeyu/momo/releases) 了解后续交付；从源码运行请读 [开发指南](docs/development/migration.md)。

取得体验版后，从三件小事开始：

1. 在设置中填写兼容的对话服务地址、API key 和模型，然后测试连接。
2. 先打个招呼，或开启麦克风；文字聊天不要求部署语音合成服务。
3. 想听见声音时，再连接可选 TTS，提供自己有权使用的参考音频与对应文本。

对话会发送到你选择的模型服务；启用语音合成时，参考录音也会发给所选语音服务。项目不附带默认音色样本，第三方服务的费用与条款由对应服务决定。

| 你想了解 | 入口 |
| --- | --- |
| 第一次打开、模型与形象设置 | [初次见面](docs/usage/getting-started.md) |
| 参考音、语音连接与可选 GPU 部署 | [让 momo 开口说话](docs/usage/tts.md) |
| 记录在哪里，怎样备份与迁移 | [备份与迁移](docs/usage/data.md) |
| 从源码开发、运行与构建 | [开发指南](docs/development/migration.md) · [运行配置](docs/development/configuration.md) |

目前优先完成 **macOS** 体验；**Windows** 后续单独推进，**Linux** 为次要可选方向。momo 面向电脑，不提供手机 / 平板应用。

## 一起把这里变得更好

不只有代码能帮上忙。一次具体的体验反馈、一句更自然的翻译、形象与交互设计建议，都会让相处更好一些。

- **体验与反馈**：[报告问题或提出想法](https://github.com/Momoyeyu/momo/issues)，告诉我们发生了什么，以及你希望怎样相处。
- **文案、翻译与设计**：欢迎改进中文、英文、日文表达，或分享可以讨论的界面方案。
- **代码与工具**：界面、记忆体验、语音、可信拓展都可以参与，从 [贡献指南](CONTRIBUTING.md) 找到开发与验证入口。

## 致谢与许可

感谢 [AIRI](https://github.com/moeru-ai/airi) 等开源项目为 AI 角色与陪伴体验提供启发。默认 Hiyori 模型来自 Live2D Inc.；字体、语音识别、检索模型与语音服务都建立在各自的上游工作之上。

项目代码按 [MIT 许可](LICENSE.md) 提供。第三方模型、Cubism Core、字体、依赖与展示素材遵守各自条款，仓库的 MIT 许可不替代它们。来源、许可与再分发注意事项见 [第三方声明](THIRD_PARTY_NOTICES.md)。
