// ws url、资源路径：浏览器 / Tauri 两态（momo-app.md §前端、§Tauri 路线）。
//
// 协议不变是前提：前端永远只跟 ws://127.0.0.1:7800/ws/ui 说话；
// 浏览器 dev 态经 vite proxy（/ws → momod），Tauri 壳内直连。

declare global {
  interface Window {
    __TAURI__?: unknown;
  }
}

/** Tauri 壳探测（`window.__TAURI__`）。 */
export function isTauri(): boolean {
  return typeof window !== "undefined" && window.__TAURI__ !== undefined;
}

/** UI 链路 ws 地址。 */
export function wsUrl(): string {
  if (isTauri()) {
    return "ws://127.0.0.1:7800/ws/ui";
  }
  const proto = window.location.protocol === "https:" ? "wss:" : "ws:";
  return `${proto}//${window.location.host}/ws/ui`;
}

/** 资源 base（Live2D 模型、表情映射等）。 */
export function assetBase(): string {
  // Tauri 打包后读应用资源目录；浏览器走 public/。
  return isTauri() ? "/" : "/";
}
