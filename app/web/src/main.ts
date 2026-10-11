
import "./style.css";
import { WsClient } from "./net/ws-client";
import { AudioPlayer } from "./audio/player";
import { Live2DStage } from "./live2d/stage";
import { initChat } from "./ui/chat";
import { StatusIndicator } from "./ui/status";

const stageEl = document.querySelector<HTMLCanvasElement>("#stage");
const chatEl = document.querySelector<HTMLElement>("#chat");
const statusEl = document.querySelector<HTMLElement>("#status");
}

const stage = new Live2DStage();
stage.init(stageEl);

const player = new AudioPlayer();
const status = new StatusIndicator(statusEl);
const chat = initChat(chatEl);

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
      break;
        player.stop();
      }
      break;
    default:
      break;
  }
};

chat.onSubmit = (text) => {
};

