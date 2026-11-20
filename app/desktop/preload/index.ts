import { contextBridge, ipcRenderer } from "electron";
import type {
  DesktopBridge,
  DesktopPermissionKind,
  PermissionGuideCopy,
} from "../contracts.ts";
const bridge: DesktopBridge = Object.freeze({
  requestSystemPermission: (kind: DesktopPermissionKind) =>
    ipcRenderer.invoke("momo:request-system-permission", kind),
  openPermissionGuide: (
    kind: DesktopPermissionKind,
    copy: PermissionGuideCopy,
  ) => ipcRenderer.invoke("momo:open-permission-guide", kind, copy),
  closePermissionGuide: () => ipcRenderer.invoke("momo:close-permission-guide"),
  setPermissionGuideGranted: (granted: boolean) =>
    ipcRenderer.invoke("momo:set-permission-guide-granted", granted),
  onPermissionGuideClosed: (callback: () => void) => {
    const listener = () => callback();
    ipcRenderer.on("momo:permission-guide-closed", listener);
    return () => {
      ipcRenderer.removeListener("momo:permission-guide-closed", listener);
    };
  },
  setGameOverlay: (enabled: boolean) =>
    ipcRenderer.invoke("momo:set-game-overlay", enabled),
  setPetPanel: (open: boolean) =>
    ipcRenderer.invoke("momo:set-pet-panel", open),
  dragPetWindow: (
    phase: "start" | "move" | "end",
    point?: { x: number; y: number },
  ) => ipcRenderer.invoke("momo:drag-pet-window", { phase, point }),
  setPetHitRegions: (
    regions: { x: number; y: number; width: number; height: number }[],
  ) => ipcRenderer.invoke("momo:set-pet-hit-regions", regions),
  getWindowState: () => ipcRenderer.invoke("momo:get-window-state"),
  setWindowMode: (mode: "chat" | "pet") =>
    ipcRenderer.invoke("momo:set-window-mode", mode),
  setPetClickThrough: (enabled: boolean) =>
    ipcRenderer.invoke("momo:set-pet-click-through", enabled),
  onWindowStateChanged: (
    callback: (state: import("../contracts.ts").WindowState) => void,
  ) => {
    const listener = (
      _event: Electron.IpcRendererEvent,
      state: import("../contracts.ts").WindowState,
    ) => callback(state);
    ipcRenderer.on("momo:window-state", listener);
    return () => {
      ipcRenderer.removeListener("momo:window-state", listener);
    };
  },
  getRuntime: () => ipcRenderer.invoke("momo:get-runtime"),
});
contextBridge.exposeInMainWorld("momoDesktop", bridge);
