// kindLabel / memoryStatus 单测：徽章中文映射与失效优先级（deleted > superseded）。

import { describe, expect, it } from "vitest";
import { kindLabel, memoryStatus, statusLabel } from "./memory";

describe("kindLabel", () => {
  it("maps known kinds to Chinese labels", () => {
    expect(kindLabel("semantic")).toBe("事实");
    expect(kindLabel("episodic")).toBe("经历");
    expect(kindLabel("procedural")).toBe("习惯");
  });

  it("passes through unknown kinds", () => {
    expect(kindLabel("custom")).toBe("custom");
  });
});

describe("memoryStatus", () => {
  it("valid when neither flag is set", () => {
    expect(memoryStatus({ superseded_by: null, deleted_at: null })).toBe("valid");
  });

  it("superseded when superseded_by is set", () => {
    expect(memoryStatus({ superseded_by: "mem_x", deleted_at: null })).toBe(
      "superseded",
    );
  });

  it("deleted wins over superseded", () => {
    expect(
      memoryStatus({ superseded_by: "void:extract", deleted_at: "2026-01-01" }),
    ).toBe("deleted");
  });
});

describe("statusLabel", () => {
  it("labels invalid states, empty for valid", () => {
    expect(statusLabel("superseded")).toBe("已失效");
    expect(statusLabel("deleted")).toBe("已删除");
    expect(statusLabel("valid")).toBe("");
  });
});
