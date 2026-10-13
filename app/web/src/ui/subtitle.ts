// 字幕条（#147）：Live2D 画面下方「正在说的这句」浮层，与聊天气泡的
// 全量历史并存。
//
// 驱动源（protocol §2/§6）：
//   - output.sentence：登记 turn 内 seq→{text,emotion} 映射并计入 pending；
//   - audio.open{downlink}：显示该句字幕（一句可有多条 stream——多路合成
//     帧交错时 sink 按 tag 切换反复 open/end，字幕跟随实际在播的句）；
//   - audio.end：正播 stream 收尾 → 字幕消隐；
//   - control.cancel{turn|stream}：与 player.stop() 同步立即清空；
//   - turn.finished：turn 终态，仍 pending 的句即 text-only，各短显 ~3s。
//
// text-only 判定：wire 上 output.sentence 不携带音频标记（内部
// PipelineEvent::Sentence.audio 不出站），只能由「stream 是否到来」推断；
// 合成并发使 open 跨句顺序不作保证，唯一可靠收口点是 turn.finished。
// control.cancel{turn} 后该 turn 记账作废（retired），其后的迟到
// output.sentence/audio.open/turn.finished 一律忽略，不打断后重播。

import type {
  AudioEndMsg,
  AudioOpenMsg,
  CancelTarget,
  EmotionTag,
  OutputSentenceMsg,
} from "../net/protocol";

/** text-only 句字幕停留时长（ms）。 */
export const TEXT_ONLY_FLASH_MS = 3000;

/** 已登记的输出句。 */
interface Sentence {
  text: string;
  emotion: EmotionTag;
}

/** 待短显的 text-only 句（队列项带 turn 归属，cancel 按 turn 清理）。 */
interface FlashItem {
  turnId: string;
  sentence: Sentence;
}

/** 一个进行中 turn 的字幕账本。 */
interface TurnTrack {
  /** seq → 句本体（audio.open 显示时查表）。 */
  sentences: Map<number, Sentence>;
  /** 尚未绑定 downlink stream 的 seq；finished 时残存者即 text-only。 */
  pending: Set<number>;
}

/** 当前正播的 downlink stream → 字幕句。 */
interface ActiveStream {
  streamId: string;
  turnId: string;
  seq: number;
}

/** 字幕渲染出口：initSubtitle 绑定 #subtitle DOM；测试注入假 sink。 */
export interface SubtitleSink {
  /** textOnly=true 为 TTS 降级短显（样式区分：斜体/灰）。 */
  show(text: string, emotion: EmotionTag, textOnly: boolean): void;
  hide(): void;
}

export class SubtitleController {
  private readonly turns = new Map<string, TurnTrack>();
  /** 已 cancel/finished 的 turn：迟到事件一律按 stale 丢弃。 */
  private readonly retired = new Set<string>();
  private active: ActiveStream | null = null;
  private flashQueue: FlashItem[] = [];
  /** 正在短显的句；被语音字幕抢占时回队头稍后重显。 */
  private flashing: FlashItem | null = null;
  private flashTimer: ReturnType<typeof setTimeout> | null = null;
  private readonly sink: SubtitleSink;

  constructor(sink: SubtitleSink) {
    this.sink = sink;
  }

  /** output.sentence：登记映射；audio=false 的 text-only 句立即进短显队列
   * （到达序≈句序，pumpFlash 只在无正播音频时推进，保序）；
   * audio=true 登记 pending 等 stream；若正是正播 stream 的句则立即上屏。 */
  onSentence(msg: OutputSentenceMsg): void {
    if (this.retired.has(msg.turn_id)) return;
    const track = this.track(msg.turn_id);
    const sentence: Sentence = { text: msg.text, emotion: msg.emotion };
    track.sentences.set(msg.seq, sentence);
    if (msg.audio === false) {
      this.flashQueue.push({ turnId: msg.turn_id, sentence });
      this.pumpFlash();
      return;
    }
    track.pending.add(msg.seq);
    if (this.active && this.active.turnId === msg.turn_id && this.active.seq === msg.seq) {
      this.sink.show(sentence.text, sentence.emotion, false);
    }
  }

  /** audio.open{downlink}：认领该句并显示字幕；uplink/迟到 open 忽略。 */
  onAudioOpen(msg: AudioOpenMsg): void {
    if (msg.direction !== "downlink") return;
    const turnId = msg.turn_id ?? "";
    const seq = msg.sentence_seq ?? 0;
    if (this.retired.has(turnId)) return;
    const track = this.track(turnId);
    track.pending.delete(seq);
    this.active = { streamId: msg.stream_id, turnId, seq };
    // 语音字幕抢占 text-only 短显；被打断的句回队头稍后重显。
    this.cutFlash();
    const sentence = track.sentences.get(seq);
    if (sentence) {
      this.sink.show(sentence.text, sentence.emotion, false);
    } else {
      // 防御：open 先于 output.sentence 到达——隐藏到句文本登记再上屏。
      this.sink.hide();
    }
  }

  /** audio.end：正播 stream 收尾 → 字幕消隐，推进 text-only 队列。 */
  onAudioEnd(msg: AudioEndMsg): void {
    if (this.active?.streamId !== msg.stream_id) return;
    this.active = null;
    this.sink.hide();
    this.pumpFlash();
  }

  /** control.cancel：turn 取消清账本 + 字幕；stream 取消只停当前句。 */
  onCancel(target: CancelTarget, id: string): void {
    if (target === "turn") {
      this.turns.delete(id);
      this.retired.add(id);
      // 该 turn 排队中的短显一并作废；正在短显的异 turn 句不受影响。
      this.flashQueue = this.flashQueue.filter((f) => f.turnId !== id);
      if (this.flashing?.turnId === id) this.stopFlash();
      if (this.active?.turnId === id) {
        this.active = null;
        this.sink.hide();
        this.pumpFlash();
      }
    } else if (target === "stream") {
      if (this.active?.streamId === id) {
        this.active = null;
        this.sink.hide();
        this.pumpFlash();
      }
    }
  }

  /** turn.finished：账本内仍 pending 的句 = text-only，按句序短显。 */
  onTurnFinished(turnId: string): void {
    const track = this.turns.get(turnId);
    this.turns.delete(turnId);
    this.retired.add(turnId);
    if (!track) return;
    for (const seq of [...track.pending].sort((a, b) => a - b)) {
      const sentence = track.sentences.get(seq);
      if (sentence) this.flashQueue.push({ turnId, sentence });
    }
    track.pending.clear();
    this.pumpFlash();
  }

  // ---- internal ----

  private track(turnId: string): TurnTrack {
    let t = this.turns.get(turnId);
    if (!t) {
      t = { sentences: new Map(), pending: new Set() };
      this.turns.set(turnId, t);
    }
    return t;
  }

  /** text-only 队列推进：只在无语音字幕且无短显进行中时取下一项。 */
  private pumpFlash(): void {
    if (this.active || this.flashing) return;
    const next = this.flashQueue.shift();
    if (!next) return;
    this.flashing = next;
    this.sink.show(next.sentence.text, next.sentence.emotion, true);
    this.flashTimer = setTimeout(() => {
      this.flashTimer = null;
      this.flashing = null;
      this.sink.hide();
      this.pumpFlash();
    }, TEXT_ONLY_FLASH_MS);
  }

  /** 语音字幕抢占：短显中断并回队头，音频结束后重显。 */
  private cutFlash(): void {
    if (this.flashTimer !== null) {
      clearTimeout(this.flashTimer);
      this.flashTimer = null;
    }
    if (this.flashing) {
      this.flashQueue.unshift(this.flashing);
      this.flashing = null;
    }
  }

  /** 丢弃正在短显的句（cancel 路径；不重显）。 */
  private stopFlash(): void {
    if (this.flashTimer !== null) {
      clearTimeout(this.flashTimer);
      this.flashTimer = null;
    }
    if (this.flashing) {
      this.flashing = null;
      this.sink.hide();
    }
  }
}

/** 逐字显现节奏（ms/字）。 */
const TYPEWRITER_MS = 26;

/** 绑定 #subtitle 对话框 DOM（名牌 + #subtitle-text）；返回控制器供 main.ts 接 ws 事件。 */
export function initSubtitle(el: HTMLElement): SubtitleController {
  const textEl = el.querySelector<HTMLElement>("#subtitle-text") ?? el;
  let typeTimer: number | null = null;

  /** 停打字机；finish=true 表示整句显完（露出续行提示）。 */
  function stopTyping(finish: boolean): void {
    if (typeTimer !== null) {
      window.clearInterval(typeTimer);
      typeTimer = null;
    }
    if (finish) el.classList.remove("typing");
  }

  return new SubtitleController({
    show(text, _emotion, textOnly) {
      stopTyping(false);
      el.classList.toggle("text-only", textOnly);
      el.classList.add("typing");
      el.hidden = false;
      textEl.textContent = "";
      let i = 0;
      const step = () => {
        i += 1;
        textEl.textContent = text.slice(0, i);
        if (i >= text.length) stopTyping(true);
      };
      step();
      if (i < text.length) {
        typeTimer = window.setInterval(step, TYPEWRITER_MS);
      }
    },
    hide() {
      stopTyping(true);
      el.hidden = true;
    },
  });
}
