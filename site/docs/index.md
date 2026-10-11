---
layout: home

hero:
  name: momo
  text: 住在你电脑里的 AI 伙伴
  tagline: 三段式架构 · 全本地可跑 · 自己的声线与人格
  actions:
    - theme: brand
      text: 查看架构设计
      link: /guide/architecture
    - theme: alt
      text: GitHub
      link: https://github.com/Momoyeyu/momo/tree/master/docs

features:
  - icon: 🧠
    title: 本地大脑
    details: Qwen3-30B-A3B 4bit 在 Apple Silicon 上实测 121 tok/s；DeepSeek API 兜底。OpenAI-compatible 接口，换脑不改代码。
  - icon: 🗣️
    title: 自己的声线
    details: 零样本克隆已跑通（F5-TTS RTF 0.535）；不达标走 4090D SFT ≤24h，产物灰度切换可回滚。
  - icon: ⚡
    title: 真·打断
    details: barge-in 三动作 —— 停播、取消上游、已播句入账。取消令牌全管线传播，<50ms 退出。
  - icon: 🔌
    title: 协议解耦
    details: momo-ws v1：JSON 控制帧 + 二进制音频帧复用一条连接。交互端、编排器、模型服务进程分离。
---

<div class="landing-section">

## 一条语音回合 · 1.5 秒内出声

麦采集 → VAD 分段 → SenseVoice 转写 → LLM 流式 → 句切 → TTS 逐句合成 → 边下边播。
你说到一半，她会停下来听你说。

<DiagramFrame src="turn-voice.html" :height="620" />

</div>

<div class="landing-section">

## 实测数据（2026-10-10，真机跑出）

<div class="metric-row">
  <div class="metric-chip"><div class="num">121 t/s</div><div class="lbl">Qwen3-30B-A3B-4bit · M3 Ultra MLX</div></div>
  <div class="metric-chip"><div class="num">209 ms</div><div class="lbl">Kokoro TTS 首包 · 3070 Laptop</div></div>
  <div class="metric-chip"><div class="num">0.535</div><div class="lbl">F5-TTS 克隆 RTF · 3070 Laptop</div></div>
  <div class="metric-chip"><div class="num">0.040</div><div class="lbl">SenseVoice ASR RTF · 纯 CPU</div></div>
</div>

所有「实测」标注可复现；「预估/待实测」在文档中显式标注，不混用。

</div>

<div class="landing-section">

## 开发路线 · P0 → P5

<div class="stage-grid">
  <div class="stage-card"><span class="stage-tag">P0</span><h3>管道骨架</h3><p>文字 → LLM → edge-tts → 出声。打通协议与进程链路。</p></div>
  <div class="stage-card"><span class="stage-tag">P1</span><h3>有脸</h3><p>Live2D 渲染、口型驱动、WebUI 字幕与气泡。</p></div>
  <div class="stage-card"><span class="stage-tag">P2</span><h3>对话</h3><p>语音输入、VAD 打断、本地 TTS 上线。</p></div>
  <div class="stage-card"><span class="stage-tag">P3</span><h3>灵魂</h3><p>记忆检索、自发开口、声线 SFT 上线。</p></div>
  <div class="stage-card"><span class="stage-tag">P4</span><h3>能力</h3><p>action 总线（游戏/工具）、视觉输入。</p></div>
  <div class="stage-card"><span class="stage-tag">P5</span><h3>直播</h3><p>OBS / 弹幕接入 —— 暂缓，本地先跑稳。</p></div>
</div>

两条并行轨道不与里程碑绑定：<a href="/guide/voice-clone">声线克隆</a>（本周可开始攒素材）与
<a href="/guide/persona">人格微调</a>（P3 起认真做）。

</div>

<div class="landing-section">

## 一个仓库 · 五个模块

全部代码与设计都在 [Momoyeyu/momo](https://github.com/Momoyeyu/momo) 单仓内，按目录划分：

<div class="stage-grid">
  <div class="stage-card"><span class="stage-tag">Rust + TS</span><h3><a href="https://github.com/Momoyeyu/momo/tree/master/app">app/</a></h3><p>交互端 + 编排器。tokio/axum，回合状态机、打断、句切、音频队列。</p></div>
  <div class="stage-card"><span class="stage-tag">Python</span><h3><a href="https://github.com/Momoyeyu/momo/tree/master/models">models/</a></h3><p>模型服务。FastAPI，ASR/TTS/embed 服务 + 训练/评测脚本。</p></div>
  <div class="stage-card"><span class="stage-tag">Rust</span><h3><a href="https://github.com/Momoyeyu/momo/tree/master/infra">infra/</a></h3><p>momoctl 运维工具 + Web 控制台。doctor、serve、模型管理、训练 job。</p></div>
  <div class="stage-card"><span class="stage-tag">Docs</span><h3><a href="https://github.com/Momoyeyu/momo/tree/master/docs">docs/</a></h3><p>全部设计文档（本站内容的权威来源），文档先行。</p></div>
  <div class="stage-card"><span class="stage-tag">VitePress</span><h3><a href="https://github.com/Momoyeyu/momo/tree/master/site">site/</a></h3><p>本站源码——产品主页 + 设计文档站。</p></div>
</div>

任务排期见 <a href="https://github.com/users/Momoyeyu/projects/2">GitHub Project · 任务总览</a>。

</div>
