import type { BrowserWindow, Rectangle } from "electron";
import type { WindowState } from "../contracts.ts";
type ModeWindow = Pick<
  BrowserWindow,
  | "getBounds"
  | "setBounds"
  | "setMinimumSize"
  | "setAlwaysOnTop"
  | "setIgnoreMouseEvents"
  | "setBackgroundColor"
  | "setWindowButtonVisibility"
  | "setResizable"
  | "setHasShadow"
  | "isFullScreen"
  | "isMaximized"
  | "show"
  | "focus"
  | "isMinimized"
  | "restore"
>;
export class WindowModeController {
  private state: WindowState = { mode: "chat", clickThrough: false };
  private regions: Rectangle[] = [];
  private regionsAt = 0;
  private ignored = false;
  private pointerTimer: ReturnType<typeof setInterval> | undefined;
  private dragOrigin:
    | { point: { x: number; y: number }; bounds: Rectangle }
    | undefined;
  private chatBounds: Rectangle | undefined;
  constructor(
    private window: ModeWindow,
    private workArea: () => Rectangle,
    private changed: (state: WindowState) => void,
    private cursor?: () => { x: number; y: number },
    private now: () => number = Date.now,
  ) {}
  getState(): WindowState {
    return { ...this.state };
  }
  setMode(mode: unknown): WindowState {
    if (mode !== "chat" && mode !== "pet") throw Error("Invalid window mode");
    if (mode === this.state.mode) {
      if (mode === "chat") this.fitChat(this.window.getBounds());
      return this.getState();
    }
    if (
      mode === "pet" &&
      (this.window.isFullScreen() || this.window.isMaximized())
    )
      throw Error("Exit fullscreen or maximized mode before desktop mode");
    this.stopTracking();
    this.dragOrigin = undefined;
    this.applyIgnore(false, true);
    this.state = { mode, clickThrough: false };
    const pet = mode === "pet";
    if (pet) this.chatBounds = this.window.getBounds();
    const area = this.workArea();
    this.window.setMinimumSize(
      Math.min(pet ? 320 : 960, area.width),
      Math.min(pet ? 440 : 540, area.height),
    );
    this.window.setResizable(!pet);
    this.window.setAlwaysOnTop(pet);
    this.window.setHasShadow(!pet);
    if (process.platform === "darwin")
      this.window.setWindowButtonVisibility(!pet);
    this.window.setBackgroundColor(pet ? "#00000000" : "#18151f");
    if (pet) {
      const a = this.workArea();
      const width = Math.min(360, a.width),
        height = Math.min(520, a.height);
      this.window.setBounds({
        x: a.x + Math.max(0, a.width - width - 16),
        y: a.y + Math.max(0, a.height - height - 16),
        width,
        height,
      });
    } else this.fitChat(this.chatBounds ?? this.window.getBounds());
    this.changed(this.getState());
    return this.getState();
  }
  setClickThrough(value: unknown): WindowState {
    if (typeof value !== "boolean" || (value && this.state.mode !== "pet"))
      throw Error("Invalid click-through state");
    this.state.clickThrough = value;
    this.dragOrigin = undefined;
    this.stopTracking();
    this.refreshPointer();
    if (value && this.cursor) {
      this.pointerTimer = setInterval(() => this.refreshPointer(), 16);
      this.pointerTimer.unref?.();
    }
    this.changed(this.getState());
    return this.getState();
  }
  dragPet(phase: unknown): void {
    if (phase !== "start" && phase !== "move" && phase !== "end")
      throw Error("Invalid drag phase");
    if (phase === "end") {
      this.dragOrigin = undefined;
      return;
    }
    if (this.state.mode !== "pet" || this.state.clickThrough || !this.cursor)
      return;
    const point = this.cursor();
    if (phase === "start") {
      this.dragOrigin = { point, bounds: this.window.getBounds() };
      return;
    }
    if (!this.dragOrigin) return;
    const { point: start, bounds } = this.dragOrigin,
      area = this.workArea();
    this.window.setBounds({
      ...bounds,
      x: Math.max(
        area.x,
        Math.min(
          area.x + area.width - bounds.width,
          bounds.x + point.x - start.x,
        ),
      ),
      y: Math.max(
        area.y,
        Math.min(
          area.y + area.height - bounds.height,
          bounds.y + point.y - start.y,
        ),
      ),
    });
  }
  /** Renderer reports normalized interactive rectangles, never arbitrary native input. */
  setInteractiveRegions(value: unknown): void {
    if (!Array.isArray(value) || value.length > 64)
      throw Error("Invalid hit regions");
    const valid = value.every(
      (r) =>
        r &&
        [r.x, r.y, r.width, r.height].every(
          (n) =>
            typeof n === "number" && Number.isFinite(n) && n >= 0 && n <= 1,
        ) &&
        r.x + r.width <= 1.001 &&
        r.y + r.height <= 1.001,
    );
    if (!valid) throw Error("Invalid hit regions");
    this.regions = value.map((r) => ({
      x: r.x,
      y: r.y,
      width: r.width,
      height: r.height,
    }));
    this.regionsAt = this.now();
    this.refreshPointer();
  }
  refreshPointer(): void {
    // Fail open on a stalled renderer or missing geometry: controls must recover.
    if (
      !this.state.clickThrough ||
      this.state.mode !== "pet" ||
      !this.cursor ||
      !this.regions.length ||
      this.now() - this.regionsAt > 1000
    ) {
      this.applyIgnore(false);
      return;
    }
    const bounds = this.window.getBounds(),
      point = this.cursor();
    const x = (point.x - bounds.x) / bounds.width,
      y = (point.y - bounds.y) / bounds.height;
    const interactive = this.regions.some(
      (r) => x >= r.x && x <= r.x + r.width && y >= r.y && y <= r.y + r.height,
    );
    this.applyIgnore(!interactive);
  }
  private applyIgnore(value: boolean, force = false): void {
    if (!force && value === this.ignored) return;
    this.ignored = value;
    this.window.setIgnoreMouseEvents(value, { forward: true });
  }
  private stopTracking(): void {
    clearInterval(this.pointerTimer);
    this.pointerTimer = undefined;
  }
  dispose(): void {
    this.stopTracking();
  }
  private fitChat(bounds: Rectangle) {
    const area = this.workArea();
    const width = Math.min(bounds.width, area.width);
    const height = Math.min(bounds.height, area.height);
    this.window.setMinimumSize(
      Math.min(960, area.width),
      Math.min(540, area.height),
    );
    this.window.setBounds({
      x: Math.max(area.x, Math.min(bounds.x, area.x + area.width - width)),
      y: Math.max(area.y, Math.min(bounds.y, area.y + area.height - height)),
      width,
      height,
    });
  }
  recover() {
    if (this.window.isMinimized()) this.window.restore();
    this.setMode("chat");
    this.setClickThrough(false);
    this.window.show();
    this.window.focus();
  }
}
