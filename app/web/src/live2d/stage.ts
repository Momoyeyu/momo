// pixi 舞台 + pixi-live2d-display 模型加载（momo-app.md §模块结构）。
//
// 模型：Cubism 免费 sample（开发期）；直播 license P5 前确认。
// pixi 主版本锁定 v7 = pixi-live2d-display@0.5.0-beta 的 peer 范围。
// Cubism Core 不入库：从官方版本化地址注入 <script>（docs/00-context/resources.md）。

import * as PIXI from "pixi.js";
import { cubismCoreUrl } from "../config";

// pixi-live2d-display 约定从全局读 PIXI。
(window as unknown as { PIXI: typeof PIXI }).PIXI = PIXI;

/** Cubism Core script 注入超时。 */
const CORE_TIMEOUT_MS = 15000;

let corePromise: Promise<void> | null = null;

/** 注入 live2dcubismcore.min.js（幂等；失败重置可重试）。 */
export function ensureCubismCore(): Promise<void> {
  const global = window as unknown as { Live2DCubismCore?: unknown };
  if (global.Live2DCubismCore) return Promise.resolve();
  corePromise ??= new Promise<void>((resolve, reject) => {
    const script = document.createElement("script");
    const timer = window.setTimeout(() => {
      script.remove();
      corePromise = null;
      reject(new Error("Cubism Core load timed out"));
    }, CORE_TIMEOUT_MS);
    const fail = (why: string) => {
      window.clearTimeout(timer);
      script.remove();
      corePromise = null;
      reject(new Error(why));
    };
    script.addEventListener(
      "load",
      () => {
        if (!global.Live2DCubismCore) {
          fail("Cubism Core script loaded but Live2DCubismCore is missing");
          return;
        }
        window.clearTimeout(timer);
        resolve();
      },
      { once: true },
    );
    script.addEventListener("error", () => fail("Cubism Core script failed to load"), {
      once: true,
    });
    script.src = cubismCoreUrl();
    script.crossOrigin = "anonymous";
    document.head.appendChild(script);
  });
  return corePromise;
}

export class Live2DStage {
  private app: PIXI.Application | null = null;
  // pixi-live2d-display 类型与其 pixi 版本耦合，按 any 处理。
  private model: any = null;
  private bounds: { x: number; y: number; width: number; height: number } | null = null;

  /** 初始化舞台（透明背景叠在页面上）。 */
  init(canvas: HTMLCanvasElement): void {
    this.app = new PIXI.Application({
      view: canvas,
      resizeTo: window,
      backgroundAlpha: 0,
      antialias: true,
    });
    this.app.renderer.on("resize", () => this.fitModel());
  }

  /** 模型是否已加载（口型/表情写参前置检查）。 */
  get ready(): boolean {
    return this.model != null;
  }

  /** 加载 Live2D 模型（Cubism 4 runtime）；失败 reject，由调用方给降级提示。 */
  async loadModel(url: string): Promise<void> {
    if (!this.app) throw new Error("stage not initialized");
    await ensureCubismCore();
    const mod: any = await import("pixi-live2d-display/cubism4");
    mod.config.sound = false;
    const model = await mod.Live2DModel.from(url);
    this.model = model;
    const b = model.getLocalBounds();
    this.bounds = { x: b.x, y: b.y, width: b.width, height: b.height };
    this.app.stage.addChild(model as PIXI.DisplayObject);
    this.fitModel();
  }

  /** 模型缩放/落位：宽占 ~46%，底边贴屏幕下缘（pet 形态）。 */
  private fitModel(): void {
    if (!this.app || !this.model || !this.bounds) return;
    const { screen } = this.app.renderer;
    const b = this.bounds;
    const scale = Math.min((screen.width * 0.46) / b.width, (screen.height * 0.9) / b.height);
    this.model.scale.set(scale);
    this.model.x = (screen.width - b.width * scale) / 2 - b.x * scale;
    this.model.y = screen.height - (b.y + b.height) * scale;
  }

  /** 口型：写 ParamMouthOpenY（0..1），由 player worklet 的 RMS 驱动。 */
  setMouthOpen(value: number): void {
    const core = this.model?.internalModel?.coreModel;
    if (!core) return;
    const v = Math.min(1, Math.max(0, value));
    if (typeof core.setParameterValueById === "function") {
      core.setParameterValueById("ParamMouthOpenY", v);
    }
  }
}
