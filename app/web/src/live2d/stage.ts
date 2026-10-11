// pixi 舞台 + pixi-live2d-display 模型加载（momo-app.md §前端）。
//
// 模型：Cubism 免费 sample（开发期）；直播 license P5 前确认。
// pixi 主版本锁定 v7 = pixi-live2d-display@0.5.0-beta 的 peer 范围。

import * as PIXI from "pixi.js";

// pixi-live2d-display 约定从全局读 PIXI。
(window as unknown as { PIXI: typeof PIXI }).PIXI = PIXI;

export class Live2DStage {
  private app: PIXI.Application | null = null;
  private model: unknown = null;

  /** 初始化舞台（透明背景叠在页面上）。 */
  init(canvas: HTMLCanvasElement): void {
    this.app = new PIXI.Application({
      view: canvas,
      resizeTo: window,
      backgroundAlpha: 0,
      antialias: true,
    });
  }

  /** 加载 Live2D 模型（Cubism 4 runtime）。 */
  async loadModel(url: string): Promise<void> {
    // pixi-live2d-display 类型与其 pixi 版本耦合，stub 阶段按 any 处理。
    const mod: any = await import("pixi-live2d-display/cubism4");
    this.model = await mod.Live2DModel.from(url);
    this.app?.stage.addChild(this.model as PIXI.DisplayObject);
  }

  /** 播放状态入口：mouth open / 表情动作由 lipsync.ts 驱动。 */
  setMouthOpen(value: number): void {
    // TODO(P0): model.internalModel.coreModel.setParamFloat("ParamMouthOpenY", v)
    void value;
    void this.model;
  }
}
