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
  private chatBounds: Rectangle | undefined;
  constructor(
    private window: ModeWindow,
    private workArea: () => Rectangle,
    private changed: (state: WindowState) => void,
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
    this.window.setIgnoreMouseEvents(false);
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
    this.window.setIgnoreMouseEvents(value, { forward: true });
    this.state.clickThrough = value;
    this.changed(this.getState());
    return this.getState();
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
