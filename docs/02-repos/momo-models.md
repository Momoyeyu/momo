# `models/`：Python inference adapters 与训练实体

> 模型层提供语音识别、语音合成与文本向量能力，通过适配器复用成熟推理库，并与桌面业务和回合状态解耦。

## 边界

```mermaid
flowchart LR
    ORCH["app / TurnCoordinator"] -->|"capture stream"| ASR["ASR / VAD adapter"]
    ASR -->|"partial preview / final segment"| ORCH
    ORCH -->|"sentence"| TTS["TTS adapter"]
    TTS -->|"audio.open + PCM frames"| ORCH
    REPO["app / local repository"] -->|"text"| EMBED["EmbeddingProvider"]
    EMBED -->|"vector"| REPO
    TRAIN["training + existing eval"] -.->|"validated artifacts"| TTS & EMBED
```

- ASR partial 不能触发 reply，final 才能。
- TTS stream 绑定 turn/sentence/stream；取消后停止产出，迟到帧仍由 app 丢弃。
- `EmbeddingProvider` 只把文本转换为向量；不持有 proposal、memory 正文/index、persona 或检索决策。
- provider 先 warmup/readiness；activate 由 app/infra 原子完成。
- 模型、Runtime、sample 各自服从上游许可；应用许可证不覆盖它们。

## provider 与产物

provider 抽象：`ASRProvider`、`TTSProvider`、`EmbeddingProvider`。manifest 固定 revision/checksum 并逐条声明结构化 `license`（`spdx`/`source`，非 SPDX 许可另带 `name`）；推理服务只读已校验本地产物，不在活动 turn 中在线下载。

provider 同级可切换：每种能力经 registry 按 env 选实现——TTS `MOMO_TTS_PROVIDER`、ASR `MOMO_ASR_PROVIDER`、VAD `MOMO_VAD_PROVIDER`（本地 sherpa 与云端实现同级注册）。同一约定推广到 app 侧：LLM 经 `MOMO_LLM_ENDPOINT` 指向任意 OpenAI 兼容上游（本地 llama-server 或云端 API 等价），Live2D 模型/声线由 app 侧资源选择承担。用户可在设置项里于 momo 支持的 provider 列表中切换；自定义 provider 接入属长期方向，注册入口已预留。

provider 内的具体权重由 MANIFEST 指定：ASR 用 `MOMO_ASR_MODEL` 选条目名，条目 `dest` 即 `${MOMO_MODELS_DIR}` 下的模型目录，运行时只读该目录并按条目校验 sha256（条目名未知或清单缺失 → readyz 不就绪）。

TTS provider 实装状态：`kokoro`（默认）与 `edge` 已实装，`f5`/`cosyvoice3`/`sovits` 为 stub。kokoro 固定 MANIFEST 条目 `kokoro-82m`（dest 即模型目录，`config.json`/`kokoro-v1_0.pth`/`voices/*.pt` 经 sha256 门控），`KModel` 直接吃本地路径不经 HF 下载；音色优先级 `MOMO_TTS_VOICE` > `VoiceRef.lang`/`MOMO_TTS_LANG` 映射（zh→zf_xiaoxiao、en→af_heart、ja→jf_alpha），pipeline lang_code 取音色名首字母；产出统一为 PCM s16le 24kHz mono，cancel 在 chunk 边界停产出。

情感：`output.sentence.emotion`（protocol 固定 8 tag）经 `providers/tts/emotion.py` 的 `map_emotion(tag, provider, caps)` 落成各 provider 参数——kokoro→`speed`、edge→`rate`/`pitch`/`volume`、cosyvoice3→`instruct` 指令、sovits→参考音情感提示；`None`/未知 tag 兜底 `neutral`，`caps.emotion=False` 返回空表（产出与 neutral 一致）。消费 emotion 的 provider 声明 `caps.emotion=True`。

VAD 是独立 provider（`providers/vad/`），不从属于 ASR：`VADProvider.feed(pcm)` 以窗口喂入上行 PCM，产出 `VadEvent`——`speech_start`（供 app 打断判定）与 `speech_end`（携带整段裁剪 PCM 供 ASR 识别）；`flush`/`reset` 收尾。默认实现 `silero` 走 sherpa-onnx `VadModel`（窗口级 `is_speech` + min-speech/min-silence/max-speech 迟滞），权重经 MANIFEST sha256 校验后懒加载；选 sherpa 而非 `silero-vad` pip 包是因后者拖入完整 torch 依赖，对 CPU 断句过重。判停时长 `MOMO_VAD_SILENCE_MS`。

模型选型、官方标称与带日期的实测记录集中在 [research.md](../00-context/research.md) 与 [decisions.md](../00-context/decisions.md)，作为复现与比较基线。

## 训练边界

- 声线与 persona 训练实体、checkpoint、评估脚本在 `models/`。
- job 生命周期、本地/SSH 执行、恢复与 promotion 编排在 `infra/`。
- 声线复用 [voice-clone.md](../04-tracks/voice-clone.md) 的 frozen gate；persona 复用 [persona.md](../04-tracks/persona.md) 的历史实验规格。

## 验收指针

- handshake/frame/schema 与 [通信协议](../01-architecture/protocol.md) 一致。
- cancel、seq gap、duplicate、wrong direction、unknown stream 与 stale turn 有确定行为。
- ASR/TTS 失败遵循 [runtime.md](../01-architecture/runtime.md) 的用户可见降级。
- manifest checksum 失败不 ready、不 activate。
