export interface RuntimeConnection {
  baseUrl: string;
  token: string;
}
export interface WindowState {
  mode: "chat" | "pet";
  clickThrough: boolean;
}
export interface DesktopBridge {
  getWindowState(): Promise<WindowState>;
  setWindowMode(mode: "chat" | "pet"): Promise<WindowState>;
  setPetClickThrough(enabled: boolean): Promise<WindowState>;
  onWindowStateChanged(callback: (state: WindowState) => void): () => void;
  getRuntime(): Promise<RuntimeConnection>;
}
