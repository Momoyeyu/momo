
import { describe, expect, it } from "vitest";
import { emotionToExpression, mouthOpenY, rmsOf } from "./lipsync";
import { s16leToFloat32 } from "../audio/player";

describe("rmsOf", () => {
  it("silence is 0", () => {
    expect(rmsOf(new Float32Array(0))).toBe(0);
    expect(rmsOf(new Float32Array(64))).toBe(0);
  });

  it("full-scale square wave is 1", () => {
    expect(rmsOf(new Float32Array(16).fill(1))).toBeCloseTo(1);
  });
});

describe("mouthOpenY", () => {
  it("quiet trends to 0, loud trends to 1", () => {
    expect(mouthOpenY(0, 0.8)).toBeLessThan(0.8);
    expect(mouthOpenY(1, 0)).toBeGreaterThan(0);
    expect(mouthOpenY(10, 0)).toBeLessThanOrEqual(1);
  });

  it("attack is faster than release", () => {
    const up = mouthOpenY(0.5, 0); // 上升
    const down = 0.8 - mouthOpenY(0, 0.8); // 下降幅度
    expect(up).toBeGreaterThan(down);
  });
});

describe("s16leToFloat32", () => {
  it("decodes extremes", () => {
    const pcm = new Uint8Array(4);
    const v = new DataView(pcm.buffer);
    v.setInt16(0, 32767, true);
    v.setInt16(2, -32768, true);
    const out = s16leToFloat32(pcm);
    expect(out[0]).toBeCloseTo(32767 / 0x8000);
    expect(out[1]).toBe(-1);
  });
});

describe("emotionToExpression", () => {
  it("neutral fallback", () => {
    expect(emotionToExpression("neutral")).toBe("neutral");
    expect(emotionToExpression("happy")).toBe("smile");
  });
});
