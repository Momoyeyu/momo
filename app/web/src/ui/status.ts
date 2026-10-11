
import type { TurnState } from "../net/protocol";
import type { WsConnState } from "../net/ws-client";

const TURN_LABEL: Record<TurnState, string> = {
};

const CONN_LABEL: Record<WsConnState, string> = {
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
