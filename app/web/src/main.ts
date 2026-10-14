// 启动装配：canvas + ws-client + ui + PTT mic capture（momo-app.md §模块结构）。

import "./style.css";
import { avatarModelUrl, uiToken, wsUrl } from "./config";
import { initDesktopShell } from "./desktop";
import { WsClient } from "./net/ws-client";
import { newId, normalizeEmotion } from "./net/protocol";
import { AudioPlayer } from "./audio/player";
import { MicCapture } from "./audio/capture";
import type { MicCaptureSink } from "./audio/capture";
import { Live2DStage } from "./live2d/stage";
import { initChat } from "./ui/chat";
import { initSettings } from "./ui/settings";
import { initSubtitle } from "./ui/subtitle";
import { StatusIndicator } from "./ui/status";
import { currentView, showView } from "./ui/views";

const stageEl = document.querySelector<HTMLCanvasElement>("#stage");
const chatEl = document.querySelector<HTMLElement>("#chat");
const subtitleEl = document.querySelector<HTMLElement>("#subtitle");
const statusEl = document.querySelector<HTMLElement>("#status");
const micBtn = document.querySelector<HTMLButtonElement>("#mic");
const fallbackEl = document.querySelector<HTMLElement>("#avatar-fallback");
if (!stageEl || !chatEl || !subtitleEl || !statusEl) {
  throw new Error("index.html is missing the #stage/#chat/#subtitle/#status mount points");
}

// Tauri 壳内：无边框窗口空白区拖拽（browser 态 no-op）。
initDesktopShell();

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
// 字幕条：audio.open/end 驱动「正在说的这句」，text-only 句短显（#147）。
const subtitle = initSubtitle(subtitleEl);

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
    case "input.audio.segment":
      // ASR 转写：partial → 草稿气泡原地更新；final → 定稿为 user 气泡（#147）。
      if (msg.final) {
        chat.commitDraft(msg.capture_id, msg.text);
      } else {
        chat.setDraft(msg.capture_id, msg.text);
      }
      break;
    case "input.rejected":
      // 语音输入被拒（ref_id=capture_id）：丢弃对应 ASR 草稿。
      chat.clearDraft(msg.ref_id);
      break;
    case "output.sentence":
      // 聊天气泡按句流式追加，emotion 渲染到气泡情感位（#94）；
      // 同时登记 seq→text 映射供字幕条按 audio.open 取用（#147）。
      chat.addBubble("assistant", msg.text, normalizeEmotion(msg.emotion));
      subtitle.onSentence(msg);
      break;
    case "audio.open":
      // 仅 downlink 驱动字幕；uplink open 是 UI 自己发出的回声（不会被服务端转发）。
      subtitle.onAudioOpen(msg);
      break;
    case "audio.end":
      subtitle.onAudioEnd(msg);
      break;
    case "turn.finished":
      // turn 终态：仍未绑定 downlink stream 的句即 text-only，字幕短显（#147）。
      subtitle.onTurnFinished(msg.turn_id);
      break;
    case "control.cancel":
      // orchestrator 取消 turn/stream：同步停播、清空播放缓冲（protocol §8）。
      if (msg.target === "turn" || msg.target === "stream") {
        player.stop();
        subtitle.onCancel(msg.target, msg.id);
      } else if (msg.target === "capture") {
        // 服务端取消 capture：丢弃 ASR 草稿、释放本地 mic。
        chat.clearDraft(msg.id);
        if (mic.capturing) {
          mic.cancel();
          micBtn?.classList.remove("recording");
        }
      }
      break;
    case "error":
      // 归因字段把异步失败定位到具体 stream/capture/turn（#132）。
      console.warn("server error", msg.code, msg.message, {
        ref_id: msg.ref_id,
        turn_id: msg.turn_id,
        capture_id: msg.capture_id,
        stream_id: msg.stream_id,
        sentence_seq: msg.sentence_seq,
      });
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
  // 取消后不会再有 partial/final：本地丢弃 ASR 草稿。
  chat.clearDraft();
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

// ---------- 视图路由：menu（主页）/ scene（场景）/ settings（设置） ----------
// 进 scene 才连 ws——「开始」即进入她的房间；回主页不断线（session 保活 TTL 内可续）。

const settings = initSettings({ stage });
const menuStartBtn = document.querySelector<HTMLButtonElement>("#menu-start");
const menuSettingsBtn = document.querySelector<HTMLButtonElement>("#menu-settings");
const menuBackBtn = document.querySelector<HTMLButtonElement>("#menu-btn");
const chatSettingsBtn = document.querySelector<HTMLButtonElement>("#chat-settings");

let entered = false;

function toView(view: "menu" | "scene" | "settings"): void {
  showView(view);
  stage.refit();
}

menuStartBtn?.addEventListener("click", () => {
  toView("scene");
  if (!entered) {
    entered = true;
    client.connect();
    if (menuStartBtn) menuStartBtn.textContent = "继续";
  }
});
menuSettingsBtn?.addEventListener("click", () => void settings.open("menu"));
chatSettingsBtn?.addEventListener("click", () => void settings.open("scene"));
menuBackBtn?.addEventListener("click", () => toView("menu"));
// Esc 在场景内回主页；menu 内不进 scene（主页按 Esc 无意义）。
window.addEventListener("keydown", (e) => {
  if (e.key === "Escape" && currentView() === "scene") toView("menu");
});
