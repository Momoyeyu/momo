//
// pixi 主版本锁定 v7 = pixi-live2d-display@0.5.0-beta 的 peer 范围。

import * as PIXI from "pixi.js";

// pixi-live2d-display 约定从全局读 PIXI。
(window as unknown as { PIXI: typeof PIXI }).PIXI = PIXI;

export class Live2DStage {
  private app: PIXI.Application | null = null;

  /** 初始化舞台（透明背景叠在页面上）。 */
  init(canvas: HTMLCanvasElement): void {
    this.app = new PIXI.Application({
      view: canvas,
      resizeTo: window,
      backgroundAlpha: 0,
      antialias: true,
    });
  }

  async loadModel(url: string): Promise<void> {
    const mod: any = await import("pixi-live2d-display/cubism4");
  }

  setMouthOpen(value: number): void {
  }
}
