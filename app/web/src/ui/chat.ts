// 文本气泡、输入框（momo-app.md §模块结构）。
// 无框架原生 DOM：#chat 气泡列表 + #composer 表单。
// output.sentence.emotion → 气泡情感位（8 个固定 tag，样式表映射）。
// input.audio.segment：partial → 灰字斜体草稿气泡原地更新；final → 定稿
// 为正式 user 气泡（#147）；capture 取消/中断时 clearDraft 丢弃草稿。

import type { EmotionTag } from "../net/protocol";

/** 初始化聊天 UI；返回后的 onSubmit 回调由 main.ts 接到 ws-client。 */
export function initChat(root: HTMLElement): {
  onSubmit: ((text: string) => void) | null;
  addBubble: (role: "user" | "assistant", text: string, emotion?: EmotionTag) => void;
  /** ASR partial 预览：同一 capture 的后续 partial 原地更新草稿气泡。 */
  setDraft: (captureId: string, text: string) => void;
  /** ASR final：草稿定稿为正式 user 气泡；空文本只丢草稿。 */
  commitDraft: (captureId: string, text: string) => void;
  /** capture 取消/拒绝：丢弃草稿；带 captureId 时仅清除匹配的草稿。 */
  clearDraft: (captureId?: string) => void;
} {
  /** 当前草稿气泡与其 capture 归属；无草稿时均为 null。 */
  let draftEl: HTMLElement | null = null;
  let draftCaptureId: string | null = null;

  function removeDraft(): void {
    draftEl?.remove();
    draftEl = null;
    draftCaptureId = null;
  }

  const chat = {
    onSubmit: null as ((text: string) => void) | null,
    addBubble(role: "user" | "assistant", text: string, emotion?: EmotionTag): void {
      const div = document.createElement("div");
      div.className = `bubble ${role}`;
      if (emotion) {
        // 情感位：tag 文本 + emotion-* 样式类（style.css 定义 8 tag 映射）。
        const tag = document.createElement("span");
        tag.className = `emotion-tag emotion-${emotion}`;
        tag.textContent = emotion;
      }
      const body = document.createElement("span");
      body.className = "bubble-text";
      body.textContent = text;
      root.appendChild(div);
      root.scrollTop = root.scrollHeight;
    },
    setDraft(captureId: string, text: string): void {
      if (!text) return;
      // 新 capture 顶掉旧草稿（上一 capture 若未 final 即作废）。
      if (draftEl && draftCaptureId !== captureId) removeDraft();
      if (!draftEl) {
        draftEl = document.createElement("div");
        draftEl.className = "bubble user draft";
        const body = document.createElement("span");
        body.className = "bubble-text";
        draftEl.appendChild(body);
        root.appendChild(draftEl);
      }
      draftCaptureId = captureId;
      const body = draftEl.firstElementChild;
      if (body) body.textContent = text;
      root.scrollTop = root.scrollHeight;
    },
    commitDraft(_captureId: string, text: string): void {
      // 无论草稿归属：final 到达即作废草稿（同 capture 定稿、异 capture 丢弃）。
      removeDraft();
      const trimmed = text.trim();
      if (trimmed) chat.addBubble("user", trimmed);
    },
    clearDraft(captureId?: string): void {
      // 指定 capture 时不误清其他 capture 的新草稿。
      if (captureId !== undefined && draftCaptureId !== captureId) return;
      removeDraft();
    },
  };

  const form = document.querySelector<HTMLFormElement>("#composer");
  const input = document.querySelector<HTMLInputElement>("#composer-input");
  form?.addEventListener("submit", (e) => {
    e.preventDefault();
    const text = input?.value.trim();
    if (!text) return;
    chat.addBubble("user", text);
    chat.onSubmit?.(text);
    if (input) input.value = "";
  });

  return chat;
}
