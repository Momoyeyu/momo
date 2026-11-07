export interface RuntimeConnection {
  baseUrl: string;
  token: string;
}
export interface WindowState {
  mode: "chat" | "pet";
  clickThrough: boolean;
}
export interface DesktopBridge {
  dragPetWindow(phase: "start" | "move" | "end"): Promise<void>;
  setPetHitRegions(
    regions: { x: number; y: number; width: number; height: number }[],
  ): Promise<void>;
  getWindowState(): Promise<WindowState>;
  setWindowMode(mode: "chat" | "pet"): Promise<WindowState>;
  setPetClickThrough(enabled: boolean): Promise<WindowState>;
  onWindowStateChanged(callback: (state: WindowState) => void): () => void;
  getRuntime(): Promise<RuntimeConnection>;
}
