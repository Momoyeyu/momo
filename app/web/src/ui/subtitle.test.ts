// SubtitleController 单测（#147）：假 sink 记录 show/hide 序列，
// 假时钟推进 text-only 短显计时；覆盖语音字幕、text-only 判定与
// cancel/乱序事件路径。

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { SubtitleController, TEXT_ONLY_FLASH_MS } from "./subtitle";
import type { SubtitleSink } from "./subtitle";
import type {
  AudioEndMsg,
  AudioOpenMsg,
  EmotionTag,
  OutputSentenceMsg,
} from "../net/protocol";

/** 记录 show/hide 调用序列的假 sink。 */
class FakeSink implements SubtitleSink {
  calls: ({ kind: "show"; text: string; emotion: EmotionTag; textOnly: boolean } | { kind: "hide" })[] =
    [];

  show(text: string, emotion: EmotionTag, textOnly: boolean): void {
    this.calls.push({ kind: "show", text, emotion, textOnly });
  }

  hide(): void {
    this.calls.push({ kind: "hide" });
  }

  /** 最近一次 show 的载荷；无 show 返回 null。 */
  lastShow() {
    for (let i = this.calls.length - 1; i >= 0; i--) {
      const c = this.calls[i];
      if (c.kind === "show") return c;
    }
    return null;
  }

  /** 当前是否处于显示态（最后一条调用是 show）。 */
  get visible(): boolean {
    return this.calls[this.calls.length - 1]?.kind === "show";
  }
}

let seq = 0;
function sentence(turn: string, n: number, text = `s${n}`): OutputSentenceMsg {
  return {
    type: "output.sentence",
    session_id: "s",
    event_id: `e${seq++}`,
    at_ms: 0,
    event_seq: seq,
    turn_id: turn,
    seq: n,
    text,
    emotion: "happy",
    lang: "zh",
  };
}

let streamSeq = 0;
function open(turn: string, n: number, direction: "uplink" | "downlink" = "downlink"): AudioOpenMsg {
  return {
    type: "audio.open",
    session_id: "s",
    event_id: `e${seq++}`,
    at_ms: 0,
    stream_id: `st_${turn}_${n}_${streamSeq++}`,
    direction,
    turn_id: direction === "downlink" ? turn : undefined,
    capture_id: direction === "uplink" ? `cap_${n}` : undefined,
    sentence_seq: direction === "downlink" ? n : undefined,
    codec: "pcm_s16le",
    sample_rate: direction === "downlink" ? 24000 : 16000,
    channels: 1,
  };
}

function end(openMsg: AudioOpenMsg): AudioEndMsg {
  return {
    type: "audio.end",
    session_id: "s",
    event_id: `e${seq++}`,
    at_ms: 0,
    stream_id: openMsg.stream_id,
    last_chunk_seq: 3,
  };
}

function make() {
  const sink = new FakeSink();
  return { sink, ctl: new SubtitleController(sink) };
}

describe("SubtitleController 语音字幕", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("audio.open 显示对应句字幕，audio.end 消隐", () => {
    const { sink, ctl } = make();
    ctl.onSentence(sentence("t1", 1, "你好"));
    ctl.onSentence(sentence("t1", 2, "世界"));
    const o2 = open("t1", 2);
    ctl.onAudioOpen(o2);
    expect(sink.lastShow()).toMatchObject({ text: "世界", textOnly: false });
    expect(sink.visible).toBe(true);
    ctl.onAudioEnd(end(o2));
    expect(sink.calls.at(-1)).toEqual({ kind: "hide" });
  });

  it("audio.open 先于 output.sentence 到达：句文本登记后上屏", () => {
    const { sink, ctl } = make();
    const o1 = open("t1", 1);
    ctl.onAudioOpen(o1);
    expect(sink.visible).toBe(false);
    ctl.onSentence(sentence("t1", 1, "迟到文本"));
    expect(sink.lastShow()).toMatchObject({ text: "迟到文本" });
  });

  it("一句多条 stream（帧交错重开）：字幕跟随最新 open 的句", () => {
    const { sink, ctl } = make();
    ctl.onSentence(sentence("t1", 1, "一"));
    ctl.onSentence(sentence("t1", 2, "二"));
    const o1a = open("t1", 1);
    ctl.onAudioOpen(o1a);
    const o2 = open("t1", 2);
    ctl.onAudioOpen(o2); // tag 交错：sink close 旧 stream 再 open
    expect(sink.lastShow()).toMatchObject({ text: "二" });
    const o1b = open("t1", 1); // seq1 第二条 stream
    ctl.onAudioOpen(o1b);
    expect(sink.lastShow()).toMatchObject({ text: "一" });
    ctl.onAudioEnd(end(o1b));
    expect(sink.calls.at(-1)).toEqual({ kind: "hide" });
  });

  it("uplink audio.open 不驱动字幕", () => {
    const { sink, ctl } = make();
    ctl.onAudioOpen(open("t1", 0, "uplink"));
    expect(sink.calls).toHaveLength(0);
  });

  it("无关 stream 的 audio.end 不消隐当前字幕", () => {
    const { sink, ctl } = make();
    ctl.onSentence(sentence("t1", 1, "x"));
    const o1 = open("t1", 1);
    ctl.onAudioOpen(o1);
    ctl.onAudioEnd(end(open("t1", 9))); // 另一条 stream
    expect(sink.visible).toBe(true);
  });
});

describe("SubtitleController text-only 短显", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("turn.finished 时未绑定 stream 的句按序各短显 ~3s", () => {
    const { sink, ctl } = make();
    ctl.onSentence(sentence("t1", 1, "甲"));
    ctl.onSentence(sentence("t1", 2, "乙"));
    ctl.onTurnFinished("t1");
    expect(sink.lastShow()).toMatchObject({ text: "甲", textOnly: true });
    vi.advanceTimersByTime(TEXT_ONLY_FLASH_MS);
    expect(sink.lastShow()).toMatchObject({ text: "乙", textOnly: true });
    vi.advanceTimersByTime(TEXT_ONLY_FLASH_MS);
    expect(sink.calls.at(-1)).toEqual({ kind: "hide" });
  });

  it("中途降级句（有 open 的句不闪，缺的句在 finished 闪）", () => {
    const { sink, ctl } = make();
    ctl.onSentence(sentence("t1", 1, "a"));
    ctl.onSentence(sentence("t1", 2, "b"));
    ctl.onSentence(sentence("t1", 3, "c"));
    const o1 = open("t1", 1);
    ctl.onAudioOpen(o1);
    ctl.onAudioEnd(end(o1));
    const o3 = open("t1", 3);
    ctl.onAudioOpen(o3);
    ctl.onAudioEnd(end(o3));
    ctl.onTurnFinished("t1");
    expect(sink.lastShow()).toMatchObject({ text: "b", textOnly: true });
    vi.advanceTimersByTime(TEXT_ONLY_FLASH_MS);
    expect(sink.calls.at(-1)).toEqual({ kind: "hide" });
    // 仅 seq2 闪一次，之后队列空。
    vi.advanceTimersByTime(10 * TEXT_ONLY_FLASH_MS);
    expect(sink.calls.filter((c) => c.kind === "show" && c.textOnly)).toHaveLength(1);
  });

  it("短显被语音字幕抢占：音频结束后从队头重显", () => {
    const { sink, ctl } = make();
    // 纯文字 turn：两句待闪。
    ctl.onSentence(sentence("t1", 1, "old1"));
    ctl.onSentence(sentence("t1", 2, "old2"));
    ctl.onTurnFinished("t1");
    expect(sink.lastShow()).toMatchObject({ text: "old1" });
    // 短显 1s 后新 turn 的音频到达 → 抢占。
    vi.advanceTimersByTime(1000);
    ctl.onSentence(sentence("t2", 1, "new"));
    const on = open("t2", 1);
    ctl.onAudioOpen(on);
    expect(sink.lastShow()).toMatchObject({ text: "new", textOnly: false });
    ctl.onAudioEnd(end(on));
    // 被打断的 old1 重新计满时长再轮到 old2。
    expect(sink.lastShow()).toMatchObject({ text: "old1", textOnly: true });
    vi.advanceTimersByTime(TEXT_ONLY_FLASH_MS);
    expect(sink.lastShow()).toMatchObject({ text: "old2", textOnly: true });
  });
});

describe("SubtitleController cancel/stale 事件", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("control.cancel{turn} 立即清空字幕与待闪队列，finished 不再重播", () => {
    const { sink, ctl } = make();
    ctl.onSentence(sentence("t1", 1, "x"));
    ctl.onSentence(sentence("t1", 2, "y"));
    const o1 = open("t1", 1);
    ctl.onAudioOpen(o1);
    ctl.onCancel("turn", "t1");
    expect(sink.calls.at(-1)).toEqual({ kind: "hide" });
    ctl.onTurnFinished("t1"); // interrupted 终态随后到达
    vi.advanceTimersByTime(10 * TEXT_ONLY_FLASH_MS);
    expect(sink.calls.filter((c) => c.kind === "show" && c.textOnly)).toHaveLength(0);
  });

  it("cancel 后迟到的 output.sentence/audio.open 按 stale 丢弃", () => {
    const { sink, ctl } = make();
    ctl.onSentence(sentence("t1", 1, "x"));
    ctl.onCancel("turn", "t1");
    ctl.onSentence(sentence("t1", 2, "late"));
    ctl.onAudioOpen(open("t1", 2));
    ctl.onTurnFinished("t1");
    vi.advanceTimersByTime(10 * TEXT_ONLY_FLASH_MS);
    expect(sink.calls.filter((c) => c.kind === "show")).toHaveLength(0);
  });

  it("control.cancel{stream} 停当前句字幕，不影响其他 turn 账本", () => {
    const { sink, ctl } = make();
    ctl.onSentence(sentence("t1", 1, "x"));
    ctl.onSentence(sentence("t1", 2, "y"));
    const o1 = open("t1", 1);
    ctl.onAudioOpen(o1);
    ctl.onCancel("stream", o1.stream_id);
    expect(sink.calls.at(-1)).toEqual({ kind: "hide" });
    // seq2 仍 pending：finished 时按 text-only 短显（其音频永不到来）。
    ctl.onTurnFinished("t1");
    expect(sink.lastShow()).toMatchObject({ text: "y", textOnly: true });
  });
});
