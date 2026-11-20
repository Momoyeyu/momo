export interface RuntimeConnection {
  baseUrl: string;
  token: string;
}
export interface WindowState {
  mode: "chat" | "pet";
  clickThrough: boolean;
  panel?: { width: number; petWidth: number };
}
export type DesktopPermissionKind = "screen" | "input";
export interface PermissionGuideCopy {
  title: string;
  instructions: string;
  dragLabel: string;
  doneLabel: string;
  grantedLabel: string;
  closeLabel: string;
  openSettingsLabel?: string;
  revealLabel?: string;
}
export interface DesktopBridge {
  requestSystemPermission(kind: DesktopPermissionKind): Promise<void>;
  openPermissionGuide(
    kind: DesktopPermissionKind,
    copy: PermissionGuideCopy,
  ): Promise<void>;
  closePermissionGuide(): Promise<void>;
  setPermissionGuideGranted(granted: boolean): Promise<void>;
  onPermissionGuideClosed(callback: () => void): () => void;
  setGameOverlay(enabled: boolean): Promise<void>;
  setPetPanel(open: boolean): Promise<WindowState>;
  dragPetWindow(
    phase: "start" | "move" | "end",
    point?: { x: number; y: number },
  ): Promise<void>;
  setPetHitRegions(
    regions: { x: number; y: number; width: number; height: number }[],
  ): Promise<void>;
  getWindowState(): Promise<WindowState>;
  setWindowMode(mode: "chat" | "pet"): Promise<WindowState>;
  setPetClickThrough(enabled: boolean): Promise<WindowState>;
  onWindowStateChanged(callback: (state: WindowState) => void): () => void;
  getRuntime(): Promise<RuntimeConnection>;
}
