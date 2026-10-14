// 设置面：provider 配置（/api/settings）+ 形象（/api/avatars manifest +
// localStorage + stage 热替换，docs/01-architecture/settings.md §形象 manifest）。
//
// provider 单槽位（ADR-028）：每类能力一个 endpoint，app 不做多配置槽/容灾。

import type { Live2DStage } from "../live2d/stage";
import { showView, type View } from "./views";

interface ApiSettings {
  schema: number;
  settings?: {
    llm?: { endpoint?: string; model?: string; api_key_set?: boolean };
    asr?: { endpoint?: string };
    tts?: { endpoint?: string };
  };
  error?: { code: string; message: string };
}

interface ApiAvatars {
  schema: number;
  avatars?: { id?: string; name?: string; url?: string }[];
  error?: { code: string; message: string };
}

const AVATAR_KEY = "momo.avatar_url";
const CUSTOM_VALUE = "__custom__";
const BUILTIN_AVATAR = {
  id: "builtin",
  name: "Hiyori（内置）",
  url: "/live2d/hiyori/Hiyori.model3.json",
};

function authHeaders(): Record<string, string> {
  const token = uiToken();
  return token ? { Authorization: `Bearer ${token}` } : {};
}

async function apiGet(): Promise<ApiSettings> {
  return (await resp.json()) as ApiSettings;
}

async function apiPut(body: unknown): Promise<ApiSettings> {
    method: "PUT",
    headers: { "content-type": "application/json", ...authHeaders() },
    body: JSON.stringify(body),
  });
  return (await resp.json()) as ApiSettings;
}

async function apiGetAvatars(): Promise<ApiAvatars> {
  const resp = await fetch("/api/avatars", { headers: authHeaders() });
  return (await resp.json()) as ApiAvatars;
}

/**
 * 形象入口校验（保存前置）：`.model3.json` 可拉取且为合法 JSON、
 * 含非空 `FileReferences.Moc`；moc 相对入口解析后 HEAD 须可达。
 * 通过返回 void，失败 throw 英文 message（UI 层再包中文提示）。
 */
export async function validateAvatarModel(url: string): Promise<void> {
  let resp: Response;
  try {
    resp = await fetch(url);
  } catch (err) {
    throw new Error(
      `model3.json unreachable: ${err instanceof Error ? err.message : String(err)}`,
    );
  }
  if (!resp.ok) {
    throw new Error(`model3.json fetch failed: HTTP ${resp.status}`);
  }
  let doc: unknown;
  try {
    doc = await resp.json();
  } catch {
    throw new Error("model3.json is not valid JSON");
  }
  const moc = (doc as { FileReferences?: { Moc?: unknown } } | null)
    ?.FileReferences?.Moc;
  if (typeof moc !== "string" || moc.trim() === "") {
    throw new Error("model3.json missing FileReferences.Moc");
  }
  const base = typeof location === "undefined" ? "http://localhost/" : location.href;
  const mocUrl = new URL(moc, new URL(url, base)).href;
  let head: Response;
  try {
    head = await fetch(mocUrl, { method: "HEAD" });
  } catch (err) {
    throw new Error(
      `moc3 unreachable: ${err instanceof Error ? err.message : String(err)}`,
    );
  }
  if (!head.ok) {
    throw new Error(`moc3 fetch failed: HTTP ${head.status}`);
  }
}

export interface SettingsHooks {
  stage: Live2DStage;
}

export function initSettings(hooks: SettingsHooks): { open(from: View): Promise<void> } {
  const form = document.querySelector<HTMLFormElement>("#settings-form");
  const backBtn = document.querySelector<HTMLButtonElement>("#settings-back");
  const msgEl = document.querySelector<HTMLElement>("#settings-msg");
  const llmEp = document.querySelector<HTMLInputElement>("#set-llm-endpoint");
  const llmModel = document.querySelector<HTMLInputElement>("#set-llm-model");
  const llmKey = document.querySelector<HTMLInputElement>("#set-llm-key");
  const asrEp = document.querySelector<HTMLInputElement>("#set-asr-endpoint");
  const ttsEp = document.querySelector<HTMLInputElement>("#set-tts-endpoint");
  const avatarSel = document.querySelector<HTMLSelectElement>("#set-avatar");
  const avatarUrlRow = document.querySelector<HTMLElement>("#set-avatar-url-row");
  const avatarUrl = document.querySelector<HTMLInputElement>("#set-avatar-url");
  if (
    !form ||
    !llmEp ||
    !llmModel ||
    !llmKey ||
    !asrEp ||
    !ttsEp ||
    !avatarSel ||
    !avatarUrlRow ||
    !avatarUrl
  ) {
    throw new Error("settings form markup incomplete");
  }
  let returnTo: View = "menu";

  function say(text: string, isError = false): void {
    if (!msgEl) return;
    msgEl.textContent = text;
    msgEl.classList.toggle("error", isError);
  }

  /** 填下拉项 + 按已存 URL 选中；manifest 拉不到时只剩内置项兜底。 */
  const fillAvatarOptions = async (): Promise<void> => {
    let entries = [BUILTIN_AVATAR];
    try {
      const res = await apiGetAvatars();
      const list = (res.avatars ?? []).filter(
        (a): a is { id: string; name: string; url: string } =>
          typeof a?.id === "string" &&
          typeof a?.name === "string" &&
          typeof a?.url === "string",
      );
      if (list.length > 0) entries = list;
    } catch {
      // standalone / 旧版 momod 无此接口：内置项兜底，不挡设置页打开。
    }
    avatarSel.replaceChildren();
    for (const a of entries) {
      const opt = document.createElement("option");
      opt.value = a.url;
      opt.textContent = a.name;
      avatarSel.appendChild(opt);
    }
    const custom = document.createElement("option");
    custom.value = CUSTOM_VALUE;
    custom.textContent = "自定义 URL…";
    avatarSel.appendChild(custom);

    const saved = window.localStorage.getItem(AVATAR_KEY) ?? "";
    if (saved && entries.every((a) => a.url !== saved)) {
      // 已存 URL 不在 manifest：保持原值，落到自定义回退。
      avatarSel.value = CUSTOM_VALUE;
      avatarUrl.value = saved;
    } else {
      avatarSel.value = saved || entries[0].url;
      avatarUrl.value = "";
    }
    avatarUrlRow.hidden = avatarSel.value !== CUSTOM_VALUE;
  };

  const open = async (from: View): Promise<void> => {
    returnTo = from;
    showView("settings");
    say("");
    try {
      const res = await apiGet();
      const s = res.settings;
      if (!s) throw new Error(res.error?.message ?? "empty response");
      llmEp.value = s.llm?.endpoint ?? "";
      llmModel.value = s.llm?.model ?? "";
      llmKey.value = "";
      llmKey.placeholder = s.llm?.api_key_set ? "已设置（留空不变）" : "未设置";
      asrEp.value = s.asr?.endpoint ?? "";
      ttsEp.value = s.tts?.endpoint ?? "";
    } catch (err) {
      say(`读取设置失败：${err instanceof Error ? err.message : String(err)}`, true);
    }
    await fillAvatarOptions();
  };

  avatarSel.addEventListener("change", () => {
    avatarUrlRow.hidden = avatarSel.value !== CUSTOM_VALUE;
  });

  form.addEventListener("submit", (e) => {
    e.preventDefault();
    void (async () => {
      say("保存中…");
      const body = {
        llm: {
          endpoint: llmEp.value.trim() || undefined,
          model: llmModel.value.trim() || undefined,
          ...(llmKey.value !== "" ? { api_key: llmKey.value } : {}),
        },
        asr: { endpoint: asrEp.value.trim() || undefined },
        tts: { endpoint: ttsEp.value.trim() || undefined },
      };
      try {
        const res = await apiPut(body);
        if (res.error) throw new Error(res.error.message);
      } catch (err) {
        say(`保存失败：${err instanceof Error ? err.message : String(err)}`, true);
        return;
      }

      const url =
        avatarSel.value === CUSTOM_VALUE ? avatarUrl.value.trim() : avatarSel.value;
      const prev = window.localStorage.getItem(AVATAR_KEY) ?? "";
      if (url !== prev) {
        if (url) {
          try {
            await validateAvatarModel(url);
          } catch (err) {
            say(
              `设置已保存，但形象校验未通过：${err instanceof Error ? err.message : String(err)}`,
              true,
            );
            return;
          }
          window.localStorage.setItem(AVATAR_KEY, url);
        } else {
          window.localStorage.removeItem(AVATAR_KEY);
        }
        try {
          await hooks.stage.loadModel(avatarModelUrl());
        } catch (err) {
          say(
            `设置已保存，但模型加载失败：${err instanceof Error ? err.message : String(err)}`,
            true,
          );
          return;
        }
      }
      say("已保存");
    })();
  });

  backBtn?.addEventListener("click", () => showView(returnTo));

  return { open };
}
