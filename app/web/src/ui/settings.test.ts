// validateAvatarModel 单测：stub fetch，验证 model3.json 可达性、
// FileReferences.Moc 必填与 moc3 HEAD 检查的失败路径。

import { describe, expect, it, vi } from "vitest";
import { validateAvatarModel } from "./settings";

const MODEL_URL = "https://cdn.example.com/packs/haru/haru.model3.json";
const MODEL_BODY = JSON.stringify({
  Version: 3,
  FileReferences: { Moc: "haru.moc3" },
});

function stubFetch(handlers: {
  model?: () => Response;
  mocHead?: () => Response;
}): ReturnType<typeof vi.fn> {
  return vi.fn(async (input: unknown, init?: RequestInit) => {
    const url = String(input);
    if (init?.method === "HEAD") {
      return handlers.mocHead?.() ?? new Response(null, { status: 200 });
    }
    if (url === MODEL_URL) {
      return handlers.model?.() ?? new Response(MODEL_BODY, { status: 200 });
    }
    return new Response(null, { status: 404 });
  });
}

describe("validateAvatarModel", () => {
  it("passes when model3.json is valid and moc3 is reachable", async () => {
    vi.stubGlobal("fetch", stubFetch({}));
    await expect(validateAvatarModel(MODEL_URL)).resolves.toBeUndefined();
  });

  it("resolves moc3 relative to the model url before HEAD", async () => {
    const fetchMock = stubFetch({});
    vi.stubGlobal("fetch", fetchMock);
    await validateAvatarModel(MODEL_URL);
    const headCall = fetchMock.mock.calls.find(
      (c) => (c[1] as RequestInit | undefined)?.method === "HEAD",
    );
    expect(headCall?.[0]).toBe("https://cdn.example.com/packs/haru/haru.moc3");
  });

  it("rejects when model3.json is not fetchable", async () => {
    vi.stubGlobal(
      "fetch",
      stubFetch({ model: () => new Response(null, { status: 404 }) }),
    );
    await expect(validateAvatarModel(MODEL_URL)).rejects.toThrow(
      "model3.json fetch failed: HTTP 404",
    );
  });

  it("rejects on network error reaching model3.json", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => {
        throw new TypeError("fetch failed");
      }),
    );
    await expect(validateAvatarModel(MODEL_URL)).rejects.toThrow(
      "model3.json unreachable",
    );
  });

  it("rejects when model3.json is not valid JSON", async () => {
    vi.stubGlobal(
      "fetch",
      stubFetch({
        model: () => new Response("<html>oops</html>", { status: 200 }),
      }),
    );
    await expect(validateAvatarModel(MODEL_URL)).rejects.toThrow(
      "model3.json is not valid JSON",
    );
  });

  it("rejects when FileReferences.Moc is missing", async () => {
    vi.stubGlobal(
      "fetch",
      stubFetch({
        model: () =>
          new Response(JSON.stringify({ Version: 3, FileReferences: {} }), {
            status: 200,
          }),
      }),
    );
    await expect(validateAvatarModel(MODEL_URL)).rejects.toThrow(
      "model3.json missing FileReferences.Moc",
    );
  });

  it("rejects when moc3 HEAD is not ok", async () => {
    vi.stubGlobal(
      "fetch",
      stubFetch({ mocHead: () => new Response(null, { status: 404 }) }),
    );
    await expect(validateAvatarModel(MODEL_URL)).rejects.toThrow(
      "moc3 fetch failed: HTTP 404",
    );
  });
});
