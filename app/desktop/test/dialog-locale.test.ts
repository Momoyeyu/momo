import test from "node:test";
import assert from "node:assert/strict";
import { createDialogTexts, dialogLocale } from "../main/dialog-locale.ts";

test("OS locales fall back before a runtime exists", async () => {
  for (const [locale, expected] of [
    ["zh-CN", "取消"],
    ["ja_JP", "キャンセル"],
    ["fr-FR", "Cancel"],
  ]) {
    assert.equal(
      (await createDialogTexts(locale, () => undefined)()).cancel,
      expected,
    );
  }
  assert.equal(dialogLocale("ZH-TW"), "zh");
});

test("reads live preference with authenticated request, retaining language on failure", async () => {
  let value: unknown = "ja";
  const request: typeof fetch = async (url, options) => {
    assert.equal(url, "http://127.0.0.1:1234/api/state");
    assert.deepEqual(options?.headers, {
      Authorization: "Bearer fixture-token",
    });
    assert.equal(options?.redirect, "error");
    assert.ok(options?.signal);
    if (value === null) throw new Error("offline");
    return Response.json({ settings: { general: { lang: value } } });
  };
  const texts = createDialogTexts(
    "zh-CN",
    () => ({ baseUrl: "http://127.0.0.1:1234", token: "fixture-token" }),
    request,
  );
  assert.equal((await texts()).quit, "終了");
  value = "en";
  assert.equal((await texts()).quit, "Quit");
  value = "unsupported";
  assert.equal((await texts()).quit, "Quit");
  value = null;
  assert.equal((await texts()).quit, "Quit");
});

test("stalled state request aborts and preserves fallback", async () => {
  const request: typeof fetch = async (_url, options) =>
    new Promise((_resolve, reject) => {
      options!.signal!.addEventListener("abort", () =>
        reject(new Error("timeout")),
      );
    });
  const keepAlive = setTimeout(() => {}, 1000);
  try {
    const texts = createDialogTexts(
      "ja",
      () => ({ baseUrl: "http://127.0.0.1:1", token: "fixture" }),
      request,
      10,
    );
    assert.equal((await texts()).retry, "再試行");
  } finally {
    clearTimeout(keepAlive);
  }
});
