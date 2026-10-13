// 启动装配：canvas + ws-client + ui + PTT mic capture（momo-app.md §模块结构）。

import "./style.css";
import { avatarModelUrl, uiToken, wsUrl } from "./config";
import { WsClient } from "./net/ws-client";
import { newId, normalizeEmotion } from "./net/protocol";
import { AudioPlayer } from "./audio/player";
import { MicCapture } from "./audio/capture";
import type { MicCaptureSink } from "./audio/capture";
import { Live2DStage } from "./live2d/stage";
import { initChat } from "./ui/chat";
import { StatusIndicator } from "./ui/status";

const stageEl = document.querySelector<HTMLCanvasElement>("#stage");
const chatEl = document.querySelector<HTMLElement>("#chat");
const statusEl = document.querySelector<HTMLElement>("#status");
const micBtn = document.querySelector<HTMLButtonElement>("#mic");
const fallbackEl = document.querySelector<HTMLElement>("#avatar-fallback");
if (!stageEl || !chatEl || !statusEl) {
  throw new Error("index.html is missing the #stage/#chat/#status mount points");
}

const stage = new Live2DStage();
stage.init(stageEl);

// 模型加载失败不阻断聊天：可见降级提示（#95）。
stage.loadModel(avatarModelUrl()).catch((err: unknown) => {
  console.error("live2d model load failed:", err);
  if (fallbackEl) {
    fallbackEl.hidden = false;
    fallbackEl.textContent = "Avatar failed to load — chat still works. Check the model assets and reload.";
  }
});

const player = new AudioPlayer();
// 口型链：worklet RMS → mouthOpenY → ParamMouthOpenY（#96）。
player.onMouthOpen = (v) => stage.setMouthOpen(v);

const status = new StatusIndicator(statusEl);
const chat = initChat(chatEl);

const client = new WsClient(wsUrl(), uiToken());
client.onState = (state) => status.setConn(state);
client.onAudioChunk = (header, payload) => {
  player.ensureContext();
  void player.init().catch((err: unknown) => {
    console.error("audio worklet init failed:", err);
  });
  player.enqueue(header, payload);
};
client.onServerMsg = (msg) => {
  switch (msg.type) {
    case "hello.ok":
      break;
    case "state.turn":
      status.setTurn(msg.state);
      break;
    case "output.sentence":
      // 字幕气泡按句流式追加，emotion 渲染到气泡情感位（#94）。
      chat.addBubble("assistant", msg.text, normalizeEmotion(msg.emotion));
      break;
    case "control.cancel":
      // orchestrator 取消 turn/stream：同步停播、清空播放缓冲（protocol §8）。
      if (msg.target === "turn" || msg.target === "stream") {
        player.stop();
      }
      break;
    case "error":
      console.warn("server error", msg.code, msg.message);
      break;
    default:
      break;
  }
};

chat.onSubmit = (text) => {
  // 用户手势窗口内 resume AudioContext（autoplay policy），TTS 下行才不哑。
  void player.ensureContext().resume();
  client.send({ type: "input.text", id: newId("r"), text, lang: "auto" });
};

// ---------- PTT mic capture（protocol §2 / workflows PTT） ----------
// 按住说话：pointerdown 开 capture，pointerup 结束（EOS + input.audio.end）；
// pointercancel / 窗口失焦 → input.audio.cancel 并释放 mic。
// 权限拒绝/设备缺失时 start() 抛错：mic 已释放，文字路径不受影响。

const mic = new MicCapture();
const micSink: MicCaptureSink = {
  send: (msg) => client.send(msg),
  sendAudio: (header, payload) => client.sendAudio(header, payload),
};

async function startPtt(): Promise<void> {
  try {
    await mic.start(micSink, { mode: "ptt" });
    // start 期间可能已被 end/cancel（失焦）→ 只在仍在采集时亮灯。
    if (mic.capturing) micBtn?.classList.add("recording");
  } catch (err) {
    console.warn("mic capture unavailable:", err);
    chat.addBubble("assistant", "Microphone unavailable; text input still works.");
  }
}

function endPtt(): void {
  if (!mic.capturing && !mic.currentCaptureId) return;
  mic.end();
  micBtn?.classList.remove("recording");
}

function cancelPtt(): void {
  if (!mic.capturing && !mic.currentCaptureId) return;
  mic.cancel();
  micBtn?.classList.remove("recording");
}

if (micBtn) {
  micBtn.addEventListener("pointerdown", (e) => {
    e.preventDefault();
    // 捕获指针：按住在按钮外松开也能收到 pointerup；
    // 合成事件/无 active pointer 时 setPointerCapture 抛错，不得影响采集启动。
    try {
      micBtn.setPointerCapture(e.pointerId);
    } catch {
      /* inactive pointerId（合成事件/旧 webview）→ 忽略 */
    }
    void startPtt();
  });
  micBtn.addEventListener("pointerup", () => endPtt());
  micBtn.addEventListener("pointercancel", () => cancelPtt());
}
window.addEventListener("blur", () => cancelPtt());

client.connect();
