// 启动装配：canvas + ws-client + ui（momo-app.md §仓库结构）。

import "./style.css";
import { wsUrl } from "./config";
import { WsClient } from "./net/ws-client";
import { newId } from "./net/protocol";
import { AudioPlayer } from "./audio/player";
import { Live2DStage } from "./live2d/stage";
import { initChat } from "./ui/chat";
import { StatusIndicator } from "./ui/status";

const stageEl = document.querySelector<HTMLCanvasElement>("#stage");
const chatEl = document.querySelector<HTMLElement>("#chat");
const statusEl = document.querySelector<HTMLElement>("#status");
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

const client = new WsClient(wsUrl());
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
    case "control.barge_in":
      // 打断通知：清空播放缓冲并回报 played_seq（protocol §8）。
      if (msg.applied) {
        player.stop();
        client.send({ type: "control.barge_in", turn_id: msg.turn_id, applied: true, played_seq: 0 });
      }
      break;
    default:
      break;
  }
};

chat.onSubmit = (text) => {
  client.send({ type: "input.text", id: newId("r"), text, at_ms: Date.now() });
};

client.connect();
