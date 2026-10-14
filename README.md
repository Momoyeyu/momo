<p align="center">
  <strong>English</strong> · <a href="./README_ZH.md">简体中文</a>
</p>

# momo

**One more presence on your desktop — one that knows how to respond.**

[![ci-app](https://github.com/Momoyeyu/momo/actions/workflows/ci-app.yml/badge.svg)](https://github.com/Momoyeyu/momo/actions/workflows/ci-app.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](./LICENSE.md)
[![Docs](https://img.shields.io/badge/docs-momoyeyu.github.io%2Fmomo-blue.svg)](https://momoyeyu.github.io/momo/)

Most AI software is measured by what it does for you — questions answered, tasks closed, tokens spent. momo starts from a different observation: what people actually miss is rarely another tool. It's presence.

momo is a local-first AI companion: a Live2D presence with a steady personality and a long-term memory she keeps herself. She is not an assistant graded on completed tasks — "like a person" is the bar. She has her own look, her own moods, and a past; conversations have pauses as well as answers; the longer you know her, the more she becomes herself. She lives in the browser today, with a desktop home in the works.

## Living with her

- **Talk however feels natural.** Type, hold to talk, or open continuous voice — subtitles, lip-sync driven by real playback, and she stops when you cut in.
- **A character, not a config.** Her persona gives her a stable personality; her mood shows on her face and in how she answers.
- **She keeps her own memory.** After a conversation she reflects, keeps what mattered, and lets the rest fade. Everything she remembers lives on your machine — you can read it, correct it, or delete it.
- **Everything about her can change.** Her avatar is any `.model3.json` you drop in through settings; her voice, her hearing, and the mind behind her are each a model service you choose — swap any of them and keep talking.
- **Yours, locally.** Transcripts, persona, memory, and settings stay on your machine — inspectable, editable, exportable. The microphone only runs when you turn it on.

## Quickstart

Requires Rust stable (pinned by `rust-toolchain.toml`), Node.js 22+ with pnpm, and one OpenAI-compatible LLM endpoint — a local server (Ollama, vLLM, mlx_lm) or a hosted API both work.

```bash
git clone https://github.com/Momoyeyu/momo.git
cd momo

./scripts/fetch-hiyori.sh   # pull the default Live2D avatar (assets are fetched, not vendored)

cd app
cp .env.example .env      # set MOMO_LLM_ENDPOINT / MOMO_LLM_MODEL / MOMO_LLM_API_KEY
cargo run --bin momod     # orchestrator on http://127.0.0.1:7800
```

Then the frontend, in another terminal:

```bash
cd app/web
pnpm install
pnpm dev                  # → http://localhost:5173
```

Open <http://localhost:5173> and start talking. To bring your own services, open the settings view and point LLM / ASR / TTS at your endpoints — changes apply on save.

Homepage & design docs: <https://momoyeyu.github.io/momo/>

## Acknowledgements

- [Live2D Hiyori sample data](https://www.live2d.com/en/learn/sample/momose-hiyori/) — the default avatar, fetched on demand under the Live2D Free Material License
- [Live2D Cubism Core](https://www.live2d.com/) — rendering core, loaded at runtime under its own license
- [pixi-live2d-display](https://github.com/guansss/pixi-live2d-display) — renderer adapter
- [momo-infra](https://github.com/Momoyeyu/momo-infra) — companion repo for model training and serving

## License

Code is released under the [MIT License](./LICENSE.md).
