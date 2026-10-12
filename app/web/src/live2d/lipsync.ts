// RMS → ParamMouthOpenY 平滑；情感 tag → 表情/动作（momo-app.md §前端）。
// 纯函数，是 vitest 单测重点。

import type { EmotionTag } from "../net/protocol";

/** 计算一段 Float32 采样的 RMS 能量（0..~1）。 */
export function rmsOf(samples: Float32Array): number {
  if (samples.length === 0) return 0;
  let sum = 0;
  for (let i = 0; i < samples.length; i++) sum += samples[i] * samples[i];
  return Math.sqrt(sum / samples.length);
}

/** 把 RMS 映射到张嘴幅度并做 attack/release 平滑（返回 0..1）。 */
export function mouthOpenY(
  rms: number,
  prev: number,
  attack = 0.6,
  release = 0.25,
): number {
  const target = Math.min(1, Math.max(0, rms * 3));
  const k = target > prev ? attack : release;
  return prev + (target - prev) * k;
}

/** emotion tag → Live2D 表情/动作组名。
 *
 * 具体映射表是前端实现细节（protocol §2，不入协议）；
 * 正式表由 MOMO_EMOTION_MAP 文件驱动，此处给骨架默认。
 */
export function emotionToExpression(tag: EmotionTag): string {
  switch (tag) {
    case "happy":
      return "smile";
    case "sad":
      return "sad";
    case "angry":
      return "angry";
    case "surprised":
      return "surprised";
    case "thinking":
      return "thinking";
    case "shy":
      return "shy";
    case "sleepy":
      return "sleepy";
    case "neutral":
    default:
      return "neutral";
  }
}
