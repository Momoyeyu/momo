// 记忆面：用户画像（/api/memory/human-block）+ 条目审计
// （/api/memories，docs/03-features/memory.md §API）。
//
// 编辑走 supersede（旧条失效、新条 provenance=manual），删除走软删；
// 「含已失效」打开审计视图（all=1），失效/已删条目只读展示。

import { apiBase, uiToken } from "../config";
import { showView, type View } from "./views";

export interface ApiMemory {
  id: string;
  kind: string;
  content: string;
  created_at: string;
  updated_at: string;
  provenance_turn_id: string | null;
  confidence: number;
  superseded_by: string | null;
  deleted_at: string | null;
}

interface ApiMemories {
  schema: number;
  memories?: ApiMemory[];
  error?: { code: string; message: string };
}

interface ApiHumanBlock {
  schema: number;
  content?: string;
  updated_at?: string;
  error?: { code: string; message: string };
}

/** kind → 中文徽章（semantic=事实、episodic=经历、procedural=习惯）。 */
export function kindLabel(kind: string): string {
  switch (kind) {
    case "semantic":
      return "事实";
    case "episodic":
      return "经历";
    case "procedural":
      return "习惯";
    default:
      return kind;
  }
}

/** 条目状态：deleted 优先于 superseded（两者并存按删除展示）。 */
export function memoryStatus(m: {
  superseded_by: string | null;
  deleted_at: string | null;
}): "valid" | "superseded" | "deleted" {
  if (m.deleted_at) return "deleted";
  if (m.superseded_by) return "superseded";
  return "valid";
}

/** 失效/删除徽标文本。 */
export function statusLabel(status: "valid" | "superseded" | "deleted"): string {
  switch (status) {
    case "superseded":
      return "已失效";
    case "deleted":
      return "已删除";
    default:
      return "";
  }
}

function authHeaders(): Record<string, string> {
  const token = uiToken();
  return token ? { Authorization: `Bearer ${token}` } : {};
}

async function apiJson<T>(path: string, init?: RequestInit): Promise<T> {
  const resp = await fetch(`${apiBase()}${path}`, {
    ...init,
    headers: {
      ...(init?.body ? { "content-type": "application/json" } : {}),
      ...authHeaders(),
      ...init?.headers,
    },
  });
  return (await resp.json()) as T;
}

export function initMemory(): { open(from: View): Promise<void> } {
  const humanEl = document.querySelector<HTMLTextAreaElement>("#memory-human");
  const humanSave = document.querySelector<HTMLButtonElement>("#memory-human-save");
  const kindSel = document.querySelector<HTMLSelectElement>("#memory-kind");
  const allBox = document.querySelector<HTMLInputElement>("#memory-all");
  const listEl = document.querySelector<HTMLUListElement>("#memory-list");
  const msgEl = document.querySelector<HTMLElement>("#memory-msg");
  const backBtn = document.querySelector<HTMLButtonElement>("#memory-back");
  if (!humanEl || !humanSave || !kindSel || !allBox || !listEl || !backBtn) {
    throw new Error("memory panel markup incomplete");
  }
  let returnTo: View = "menu";

  function say(text: string, isError = false): void {
    if (!msgEl) return;
    msgEl.textContent = text;
    msgEl.classList.toggle("error", isError);
  }

  async function loadHumanBlock(): Promise<void> {
    const res = await apiJson<ApiHumanBlock>("/api/memory/human-block");
    if (res.error) throw new Error(res.error.message);
    if (humanEl) humanEl.value = res.content ?? "";
  }

  async function loadList(): Promise<void> {
    if (!listEl || !kindSel || !allBox) return;
    const params = new URLSearchParams({ limit: "200" });
    if (kindSel.value) params.set("kind", kindSel.value);
    if (allBox.checked) params.set("all", "1");
    const res = await apiJson<ApiMemories>(`/api/memories?${params}`);
    if (res.error) throw new Error(res.error.message);
    renderList(res.memories ?? []);
  }

  function renderList(memories: ApiMemory[]): void {
    if (!listEl) return;
    listEl.replaceChildren();
    if (memories.length === 0) {
      const empty = document.createElement("li");
      empty.className = "memory-empty";
      empty.textContent = "还没有记忆条目";
      listEl.appendChild(empty);
      return;
    }
    for (const m of memories) {
      listEl.appendChild(renderItem(m));
    }
  }

  function renderItem(m: ApiMemory): HTMLLIElement {
    const li = document.createElement("li");
    const status = memoryStatus(m);
    li.className = `memory-item${status === "valid" ? "" : " invalid"}`;

    const head = document.createElement("div");
    head.className = "memory-item-head";
    const badge = document.createElement("span");
    badge.className = `memory-kind kind-${m.kind}`;
    badge.textContent = kindLabel(m.kind);
    head.appendChild(badge);
    const st = statusLabel(status);
    if (st) {
      const stEl = document.createElement("span");
      stEl.className = "memory-status";
      stEl.textContent = st;
      head.appendChild(stEl);
    }
    const meta = document.createElement("span");
    meta.className = "memory-meta";
    const source = m.provenance_turn_id ?? "extract";
    meta.textContent = `${source} · ${(m.created_at ?? "").slice(0, 10)}`;
    head.appendChild(meta);
    li.appendChild(head);

    const body = document.createElement("p");
    body.className = "memory-content";
    body.textContent = m.content;
    li.appendChild(body);

    if (status === "valid") {
      const actions = document.createElement("div");
      actions.className = "memory-item-actions";
      const editBtn = document.createElement("button");
      editBtn.type = "button";
      editBtn.textContent = "编辑";
      editBtn.addEventListener("click", () => startEdit(li, m));
      const delBtn = document.createElement("button");
      delBtn.type = "button";
      delBtn.className = "danger";
      delBtn.textContent = "删除";
      delBtn.addEventListener("click", () => void removeItem(m));
      actions.append(editBtn, delBtn);
      li.appendChild(actions);
    }
    return li;
  }

  /** 行内编辑：正文换 textarea + 保存/取消（PUT = supersede 旧条）。 */
  function startEdit(li: HTMLLIElement, m: ApiMemory): void {
    const body = li.querySelector<HTMLElement>(".memory-content");
    const actions = li.querySelector<HTMLElement>(".memory-item-actions");
    if (!body || !actions) return;
    const area = document.createElement("textarea");
    area.className = "memory-edit-area";
    area.value = m.content;
    area.maxLength = 2000;
    area.rows = 3;
    const row = document.createElement("div");
    row.className = "memory-item-actions";
    const saveBtn = document.createElement("button");
    saveBtn.type = "button";
    saveBtn.textContent = "保存";
    const cancelBtn = document.createElement("button");
    cancelBtn.type = "button";
    cancelBtn.textContent = "取消";
    row.append(saveBtn, cancelBtn);
    body.replaceWith(area);
    actions.replaceWith(row);
    area.focus();
    cancelBtn.addEventListener("click", () => void refresh());
    saveBtn.addEventListener("click", () => {
      void (async () => {
        const content = area.value.trim();
        if (!content) {
          say("记忆内容不能为空", true);
          return;
        }
        try {
          const res = await apiJson<{ error?: { message: string } }>(
            `/api/memories/${encodeURIComponent(m.id)}`,
            { method: "PUT", body: JSON.stringify({ content }) },
          );
          if (res.error) throw new Error(res.error.message);
          say("已保存");
          await refresh();
        } catch (err) {
          say(`保存失败：${err instanceof Error ? err.message : String(err)}`, true);
        }
      })();
    });
  }

  async function removeItem(m: ApiMemory): Promise<void> {
    if (!window.confirm("删除这条记忆？（软删除，审计视图仍可见）")) return;
    try {
      const res = await apiJson<{ error?: { message: string } }>(
        `/api/memories/${encodeURIComponent(m.id)}`,
        { method: "DELETE" },
      );
      if (res.error) throw new Error(res.error.message);
      say("已删除");
      await refresh();
    } catch (err) {
      say(`删除失败：${err instanceof Error ? err.message : String(err)}`, true);
    }
  }

  async function refresh(): Promise<void> {
    try {
      await loadList();
    } catch (err) {
      say(`读取记忆失败：${err instanceof Error ? err.message : String(err)}`, true);
    }
  }

  const open = async (from: View): Promise<void> => {
    returnTo = from;
    showView("memory");
    say("");
    try {
      await loadHumanBlock();
    } catch (err) {
      say(`读取画像失败：${err instanceof Error ? err.message : String(err)}`, true);
    }
    await refresh();
  };

  humanSave.addEventListener("click", () => {
    void (async () => {
      say("保存中…");
      try {
        const res = await apiJson<ApiHumanBlock>("/api/memory/human-block", {
          method: "PUT",
          body: JSON.stringify({ content: humanEl?.value ?? "" }),
        });
        if (res.error) throw new Error(res.error.message);
        say("已保存");
      } catch (err) {
        say(`保存失败：${err instanceof Error ? err.message : String(err)}`, true);
      }
    })();
  });
  kindSel.addEventListener("change", () => void refresh());
  allBox.addEventListener("change", () => void refresh());
  backBtn.addEventListener("click", () => showView(returnTo));

  return { open };
}
