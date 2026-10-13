// 启动装配：canvas + ws-client + ui + PTT mic capture（momo-app.md §前端）。

import "./style.css";
import { uiToken, wsUrl } from "./config";
import { WsClient } from "./net/ws-client";
import { newId } from "./net/protocol";
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
if (!stageEl || !chatEl || !statusEl) {
  throw new Error("index.html 缺少 #stage/#chat/#status 挂载点");
}

// 装配（骨架：模型路径等 P0 实装时接入）。
const stage = new Live2DStage();
stage.init(stageEl);
// TODO(P0): stage.loadModel(assetBase() + "models/<cubism-sample>/model.json")

const player = new AudioPlayer();
const status = new StatusIndicator(statusEl);
const chat = initChat(chatEl);

const client = new WsClient(wsUrl(), uiToken());
client.onState = (state) => status.setConn(state);
client.onAudioChunk = (header, payload) => {
  player.ensureContext();
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
      // 字幕气泡按句流式追加（P0：聚合到 assistant 气泡）。
      chat.addBubble("assistant", msg.text);
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
