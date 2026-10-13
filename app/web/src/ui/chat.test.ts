// initChat 单测：最小 DOM stub（无 jsdom 依赖），验证气泡追加、
// 历史裁剪上限与草稿气泡生命周期。

import { beforeEach, describe, expect, it, vi } from "vitest";
import { initChat } from "./chat";

/** 够 chat.ts 用的最小 HTMLElement 替身。 */
class FakeEl {
  className = "";
  textContent = "";
  hidden = false;
  children: FakeEl[] = [];
  scrollTop = 0;
  scrollHeight = 0;
  attrs: Record<string, string> = {};

  setAttribute(name: string, value: string): void {
    this.attrs[name] = value;
  }

  get firstElementChild(): FakeEl | null {
    return this.children[0] ?? null;
  }

  get childElementCount(): number {
    return this.children.length;
  }

  appendChild(child: FakeEl): void {
    this.children.push(child);
  }

  remove(): void {
    // 由父节点 unlink；测试里通过 parentRef 挂回。
    this.parentRef?.removeChild(this);
  }

  parentRef: FakeEl | null = null;

  private removeChild(child: FakeEl): void {
    const i = this.children.indexOf(child);
    if (i >= 0) this.children.splice(i, 1);
  }
}

function fakeRoot(): FakeEl {
  return new FakeEl();
}

beforeEach(() => {
  vi.stubGlobal("document", {
    createElement: () => new FakeEl(),
    querySelector: () => null,
    querySelectorAll: () => [],
    body: { classList: { toggle: () => {}, add: () => {} } },
  });
  vi.stubGlobal("window", { matchMedia: () => ({ matches: false }) });
});

// appendChild 需要回填 parentRef（remove 依赖它）；包一层实现。
function wrapAppend(root: FakeEl): void {
  const orig = root.appendChild.bind(root);
  root.appendChild = (c: FakeEl) => {
    c.parentRef = root;
    orig(c);
  };
}

describe("initChat", () => {
  it("addBubble appends bubbles with emotion tag", () => {
    const root = fakeRoot();
    wrapAppend(root);
    const chat = initChat(root as unknown as HTMLElement);
    chat.addBubble("assistant", "你好", "happy");
    chat.addBubble("user", "hi");
    expect(root.childElementCount).toBe(2);
    expect(root.children[0].className).toContain("assistant");
    expect(root.children[1].className).toContain("user");
  });

  it("trims oldest bubbles beyond the 120-bubble cap", () => {
    const root = fakeRoot();
    wrapAppend(root);
    const chat = initChat(root as unknown as HTMLElement);
    for (let i = 0; i < 130; i++) chat.addBubble("assistant", `m${i}`);
    expect(root.childElementCount).toBe(120);
    // 最旧的 m0..m9 被裁掉，最旧现存为 m10；文本在 bubble-content 内层。
    const content = root.children[0].children.at(-1);
    expect(content?.children.at(-1)?.textContent).toBe("m10");
  });

  it("draft survives trimming (always tail) and commits to a user bubble", () => {
    const root = fakeRoot();
    wrapAppend(root);
    const chat = initChat(root as unknown as HTMLElement);
    for (let i = 0; i < 119; i++) chat.addBubble("assistant", `m${i}`);
    chat.setDraft("cap1", "partial text");
    expect(root.childElementCount).toBe(120);
    chat.setDraft("cap1", "partial more");
    expect(root.childElementCount).toBe(120); // 原地更新，不新增
    chat.commitDraft("cap1", "final text");
    expect(root.childElementCount).toBe(120); // draft removed, final appended
    const last = root.children.at(-1);
    expect(last?.className).toContain("user");
    expect(last?.className).not.toContain("draft");
  });

  it("clearDraft drops the draft bubble", () => {
    const root = fakeRoot();
    wrapAppend(root);
    const chat = initChat(root as unknown as HTMLElement);
    chat.setDraft("cap1", "typing");
    expect(root.childElementCount).toBe(1);
    chat.clearDraft("cap1");
    expect(root.childElementCount).toBe(0);
  });
});
