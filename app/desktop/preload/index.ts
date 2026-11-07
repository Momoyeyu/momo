import { contextBridge, ipcRenderer } from "electron";
import type { DesktopBridge } from "../contracts.ts";
const bridge: DesktopBridge = Object.freeze({
  dragPetWindow: (phase: "start" | "move" | "end") =>
    ipcRenderer.invoke("momo:drag-pet-window", phase),
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
