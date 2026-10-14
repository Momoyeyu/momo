<p align="center">
  <a href="./README.md">English</a> · <strong>简体中文</strong>
</p>

# momo

**让桌面多一个，懂得回应的存在。**

[![ci-app](https://github.com/Momoyeyu/momo/actions/workflows/ci-app.yml/badge.svg)](https://github.com/Momoyeyu/momo/actions/workflows/ci-app.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](./LICENSE.md)
[![Docs](https://img.shields.io/badge/docs-momoyeyu.github.io%2Fmomo-blue.svg)](https://momoyeyu.github.io/momo/)

大多数 AI 软件的价值按它替你做了什么来算——回了多少问题、清了多少任务。momo 从另一个观察出发：人真正缺的往往不是多一个工具，而是在场。

momo 是一个本地优先的 AI 陪伴应用：一个有形象、有性格、有自己记忆的伙伴。她不是以完成任务为本职的助手，「像人」才是最高标准——她有自己的样子、自己的情绪、自己的过去；对话里有留白也有脾气；相处越久，她越是她该有的样子。她现在住在浏览器里，桌面上的家正在搭。

## 和她相处

- **怎么说都行。** 打字、按住说话、或者主动打开的连续语音——有字幕、有跟真实播放同步的口型，你插话她就停下来。
- **是性格，不是配置项。** persona 给了她稳定的人格；情绪会写在表情里，也会带进回答里。
- **她自己管自己的记忆。** 每次聊完她自己归纳，留下重要的，让其余的慢慢淡掉。她记住的每一条都在你机器上——可以看、可以纠正、可以删。
- **她的一切都可以换。** 形象是任意一个 `.model3.json`，在设置页丢进去就换好；她的声音、她的听力、她背后的心智，各是一个你选的模型服务——随时换，接着聊。
- **你的就是你的。** transcript、persona、记忆和设置都留在本机——可查看、可编辑、可导出；麦克风只在你主动打开时才工作。

## 快速开始

需要 Rust stable（`rust-toolchain.toml` 锁定）、Node.js 22+ 与 pnpm，以及一个 OpenAI-compatible LLM endpoint——本地服务（Ollama、vLLM、mlx_lm）或云端 API 均可。

```bash
git clone https://github.com/Momoyeyu/momo.git
cd momo

./scripts/fetch-hiyori.sh   # 拉取默认 Live2D 形象（素材按需拉取，不随仓库分发）

cd app
cp .env.example .env      # 填 MOMO_LLM_ENDPOINT / MOMO_LLM_MODEL / MOMO_LLM_API_KEY
cargo run --bin momod     # 编排器起在 http://127.0.0.1:7800
```

另开终端起前端：

```bash
cd app/web
pnpm install
pnpm dev                  # → http://localhost:5173
```

打开 <http://localhost:5173> 就能开始聊。要接自己的服务，进设置视图把 LLM / ASR / TTS 指到你的 endpoint——保存即生效。

主页与设计文档：<https://momoyeyu.github.io/momo/>

## Acknowledgements

- [Live2D Hiyori sample data](https://www.live2d.com/en/learn/sample/momose-hiyori/)：默认形象，按 Live2D Free Material License 按需拉取
- [Live2D Cubism Core](https://www.live2d.com/)：渲染核心，运行时按其自身许可加载
- [pixi-live2d-display](https://github.com/guansss/pixi-live2d-display)：渲染适配层
- [momo-infra](https://github.com/Momoyeyu/momo-infra)：模型训练与 serving 的配套仓库

## License

代码以 [MIT License](./LICENSE.md) 发布。
