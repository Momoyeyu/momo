// Tauri 壳内行为：无边框窗口的空白区拖拽（desktop.md 拖拽阈值口径——
// 超阈值 → startDragging 由 OS 接管；未超 → 保留 click 语义给后续 tap reaction 档）。

import { isTauri } from "./config";

/** 移动多少 px 才算窗口拖拽而非点击。 */
const DRAG_THRESHOLD_PX = 4;

/** 交互控件与功能面板不触发窗口拖拽（其余空白区皆可拖）。 */
const INTERACTIVE = "button, input, textarea, select, a, form, #chat-panel";

/** desktop-only 装配；浏览器态 no-op。 */
export function initDesktopShell(): void {
  if (!isTauri()) return;
  const win = window.__TAURI__?.window?.getCurrentWindow?.();
  if (!win) {
    console.warn("desktop: __TAURI__.window API unavailable, drag disabled");
    return;
  }

  document.addEventListener("pointerdown", (e) => {
    if (e.button !== 0) return;
    const el = e.target;
    if (!(el instanceof Element) || el.closest(INTERACTIVE)) return;

    const x0 = e.clientX;
    const y0 = e.clientY;
    const cleanup = () => {
      document.removeEventListener("pointermove", onMove);
      document.removeEventListener("pointerup", cleanup);
      document.removeEventListener("pointercancel", cleanup);
    };
    const onMove = (ev: PointerEvent) => {
      if (Math.hypot(ev.clientX - x0, ev.clientY - y0) <= DRAG_THRESHOLD_PX) {
        return;
      }
      cleanup();
      void win.startDragging();
    };
    document.addEventListener("pointermove", onMove);
    document.addEventListener("pointerup", cleanup);
    document.addEventListener("pointercancel", cleanup);
  });
}
