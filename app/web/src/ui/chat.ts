// 文本气泡、输入框（momo-app.md §前端）。
// 无框架原生 DOM：#chat 气泡列表 + #composer 表单。
// output.sentence.emotion → 气泡情感位（8 个固定 tag，样式表映射）。

import type { EmotionTag } from "../net/protocol";

/** 初始化聊天 UI；返回后的 onSubmit 回调由 main.ts 接到 ws-client。 */
export function initChat(root: HTMLElement): {
  onSubmit: ((text: string) => void) | null;
  addBubble: (role: "user" | "assistant", text: string, emotion?: EmotionTag) => void;
} {
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
        div.appendChild(tag);
      }
      const body = document.createElement("span");
      body.className = "bubble-text";
      body.textContent = text;
      div.appendChild(body);
      root.appendChild(div);
      root.scrollTop = root.scrollHeight;
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
