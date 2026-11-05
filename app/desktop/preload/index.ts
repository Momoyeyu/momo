import { contextBridge, ipcRenderer } from "electron";
import type { DesktopBridge } from "../contracts.ts";
const bridge: DesktopBridge = Object.freeze({
  getRuntime: () => ipcRenderer.invoke("momo:get-runtime"),
});
contextBridge.exposeInMainWorld("momoDesktop", bridge);
