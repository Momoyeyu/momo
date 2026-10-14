// 设置面：provider 配置（/api/settings）+ 形象（localStorage + stage 热替换）。
//
// provider 单槽位（ADR-028）：每类能力一个 endpoint，app 不做多配置槽/容灾。

import { apiBase, avatarModelUrl, uiToken } from "../config";
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

const AVATAR_KEY = "momo.avatar_url";

function authHeaders(): Record<string, string> {
  const token = uiToken();
  return token ? { Authorization: `Bearer ${token}` } : {};
}

async function apiGet(): Promise<ApiSettings> {
  const resp = await fetch(`${apiBase()}/api/settings`, { headers: authHeaders() });
  return (await resp.json()) as ApiSettings;
}

async function apiPut(body: unknown): Promise<ApiSettings> {
  const resp = await fetch(`${apiBase()}/api/settings`, {
    method: "PUT",
    headers: { "content-type": "application/json", ...authHeaders() },
    body: JSON.stringify(body),
  });
  return (await resp.json()) as ApiSettings;
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
  const avatar = document.querySelector<HTMLInputElement>("#set-avatar");
  if (!form || !llmEp || !llmModel || !llmKey || !asrEp || !ttsEp || !avatar) {
    throw new Error("settings form markup incomplete");
  }
  let returnTo: View = "menu";

  function say(text: string, isError = false): void {
    if (!msgEl) return;
    msgEl.textContent = text;
    msgEl.classList.toggle("error", isError);
  }

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
      avatar.value = window.localStorage.getItem(AVATAR_KEY) ?? "";
    } catch (err) {
      say(`读取设置失败：${err instanceof Error ? err.message : String(err)}`, true);
    }
  };

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

      const url = avatar.value.trim();
      const prev = window.localStorage.getItem(AVATAR_KEY) ?? "";
      if (url !== prev) {
        if (url) {
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
