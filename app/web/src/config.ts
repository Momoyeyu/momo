//
// 协议不变是前提：前端永远只跟 ws://127.0.0.1:7800/ws/ui 说话；
// 浏览器 dev 态经 vite proxy（/ws → momod），Tauri 壳内直连。

declare global {
  interface Window {
    /** desktop 启动时经 IPC/注入脚本下发的 per-process UI token（protocol §4）。 */
    __MOMO_UI_TOKEN__?: string;
  }
}

/** Tauri 壳探测（`window.__TAURI__`）。 */
export function isTauri(): boolean {
  return typeof window !== "undefined" && window.__TAURI__ !== undefined;
}

/** UI 链路 ws 地址。 */
export function wsUrl(): string {
  if (isTauri()) {
  }
  const proto = window.location.protocol === "https:" ? "wss:" : "ws:";
  return `${proto}//${window.location.host}/ws/ui`;
}

/** 资源 base（Live2D 模型、表情映射等）。 */
export function assetBase(): string {
  // Tauri 打包后读应用资源目录；浏览器走 public/。
  return isTauri() ? "/" : "/";
}

/**
 * `hello.auth_token`：desktop 壳经 `window.__MOMO_UI_TOKEN__` 注入；
 * browser dev 用 `.env.local` 的 `VITE_MOMO_UI_TOKEN`（gitignored 本地文件，
 * token 不放 query URL/日志/header——protocol §4）。
 * standalone dev（momod 未配 MOMO_UI_TOKEN）不比对，字段给空串即可（必填仍在）。
 */
export function uiToken(): string {
  if (typeof window !== "undefined" && window.__MOMO_UI_TOKEN__ !== undefined) {
    return window.__MOMO_UI_TOKEN__;
  }
  return import.meta.env.VITE_MOMO_UI_TOKEN ?? "";
}
