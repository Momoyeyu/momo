// 回合状态指示、连接状态（momo-app.md §前端）。
// 状态指示跟随 state.turn；不得用它推断音频时序（protocol §6）。

import type { TurnState } from "../net/protocol";
import type { WsConnState } from "../net/ws-client";

const TURN_LABEL: Record<TurnState, string> = {
  idle: "空闲",
  listening: "聆听中",
  thinking: "思考中",
  speaking: "说话中",
  interrupted: "被打断",
};

const CONN_LABEL: Record<WsConnState, string> = {
  connecting: "连接中…",
  open: "已连接",
  closed: "已断开",
};

export class StatusIndicator {
  private conn: WsConnState = "connecting";
  private turn: TurnState = "idle";

  private readonly el: HTMLElement;

  constructor(el: HTMLElement) {
    this.el = el;
    this.render();
  }

  setConn(state: WsConnState): void {
    this.conn = state;
    this.render();
  }

  setTurn(state: TurnState): void {
    this.turn = state;
    this.render();
  }

  private render(): void {
    this.el.textContent = `${CONN_LABEL[this.conn]} · ${TURN_LABEL[this.turn]}`;
    this.el.dataset.conn = this.conn;
    this.el.dataset.turn = this.turn;
  }
}
