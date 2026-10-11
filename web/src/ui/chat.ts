// 文本气泡、输入框（momo-app.md §前端）。
// 无框架原生 DOM：#chat 气泡列表 + #composer 表单。

/** 初始化聊天 UI；返回后的 onSubmit 回调由 main.ts 接到 ws-client。 */
export function initChat(root: HTMLElement): {
  onSubmit: ((text: string) => void) | null;
  addBubble: (role: "user" | "assistant", text: string) => void;
} {
  const chat = {
    onSubmit: null as ((text: string) => void) | null,
    addBubble(role: "user" | "assistant", text: string): void {
      const div = document.createElement("div");
      div.className = `bubble ${role}`;
      div.textContent = text;
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
