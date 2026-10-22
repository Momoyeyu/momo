# 一起参与 momo

欢迎通过体验反馈、翻译、设计或代码参与。先看看 [项目介绍](README.md)，再选择一个你在意的小地方。

English and Japanese contributions are welcome. You can write issues and pull requests in Chinese, English, or Japanese. Start with the [English README](README_EN.md) or [日本語 README](README_JA.md); the detailed development guides currently use Chinese.

## 反馈与想法

在 [Issues](https://github.com/Momoyeyu/momo/issues) 中描述实际体验：使用的版本与系统、操作步骤、期待的结果和实际结果。界面建议可以附草图；涉及较大行为变化时，先讨论想解决的问题会更容易合作。

截图或日志请去掉密钥、私人对话与参考录音。普通问题报告不需要上传自己的完整数据备份。

## 找到合适的入口

| 方向 | 从这里开始 |
| --- | --- |
| 相处体验、交互与形象表现 | [应用前端](app/frontend/) · [初次见面](docs/usage/getting-started.md) |
| 记忆、存档与语音 | [应用后端](app/backend/) · [备份与迁移](docs/usage/data.md) · [语音设置](docs/usage/tts.md) |
| 界面翻译与文案 | 应用前端中的三语文案 · [官网三语内容](site/docs/.vitepress/theme/product-copy.mjs) · 三份 README |
| 官网与品牌 | [产品官网](site/) · [品牌 SVG](brand/) |
| 可选 GPU 语音服务 | [TTS 部署目录](deploy/tts/) |

环境准备、源码运行、打包与测试命令统一放在 [开发指南](docs/development/migration.md)，运行配置见 [配置说明](docs/development/configuration.md)。请同时阅读根目录 [AGENTS.md](AGENTS.md) 中的项目边界。

## 提交一个可以评审的改动

让一次 PR 解决一个清楚的问题，说明修改后的行为和实际验证结果。界面改动附截图；涉及持久数据时，用隔离测试目录验证并保留回退方式。只运行与改动相关的检查，不必为每个文案改动重建全部平台安装包。

当前先完成 macOS 体验，再推进 Windows；Linux 是次要可选方向，不增加手机 / 平板界面。用户数据、密钥、参考录音和测试产物不进入提交。新增模型或素材时，保留来源、许可与必要校验信息，第三方内容不自动适用仓库的 MIT 许可。

提交信息使用一行 Conventional Commit，例如 `fix: preserve the selected guide language`。不需要把整个开发过程写进 PR，说明问题、结果与验证即可。

## 许可

原创贡献遵循项目的 [MIT 许可](LICENSE.md)。引入或修改第三方内容时，遵循其原许可并更新 [第三方声明](THIRD_PARTY_NOTICES.md)；不要把模型、字体、Cubism Core 或其他依赖重新标为 MIT。
