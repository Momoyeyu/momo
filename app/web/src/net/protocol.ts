
export const PROTOCOL_NAME = "momo-ws";
export const FLAG_EOS = 0x0001;

export interface AudioHeader {
  flags: number;
}

  const frame = new Uint8Array(AUDIO_HEADER_LEN + payload.length);
  const view = new DataView(frame.buffer);
  view.setUint8(0, AUDIO_VER);
  frame.set(payload, AUDIO_HEADER_LEN);
  return frame;
}

export function decodeAudioFrame(
  frame: ArrayBuffer | Uint8Array,
): { header: AudioHeader; payload: Uint8Array } | null {
  const bytes = frame instanceof Uint8Array ? frame : new Uint8Array(frame);
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  };
}


export type ServerRole = "orchestrator" | "asr" | "tts";
export type VadEvent = "speech_start" | "speech_end";
export type TurnState = "idle" | "listening" | "thinking" | "speaking" | "interrupted";

export type EmotionTag =
  | "neutral"
  | "happy"
  | "sad"
  | "angry"
  | "surprised"
  | "thinking"
  | "shy"
  | "sleepy";

export function normalizeEmotion(tag: string): EmotionTag {
  switch (tag) {
    case "happy":
    case "sad":
    case "angry":
    case "surprised":
    case "thinking":
    case "shy":
    case "sleepy":
      return tag;
    default:
      return "neutral";
  }
}

export type ErrorCode =
  | "VERSION_MISMATCH"
  | "BAD_MESSAGE"
  | "BACKPRESSURE_OVERFLOW"
  | "MODEL_UNREACHABLE"
  | "ASR_FAILED"
  | "TTS_FAILED"


}

}

export interface HelloMsg {
  protocol: string;
  version: string;
  client: ClientKind;
}

export interface HelloOkMsg {
  version: string;
  session_id: string;
  role: ServerRole;
}

  id: string;
}

  text: string;
  lang: string;
}

  event: VadEvent;
}

  text: string;
  final: boolean;
}

}

}

  turn_id: string;
  seq: number;
}

}

}

}

  turn_id: string;
}

}

}

export interface ErrorMsg {
  code: ErrorCode;
  message: string;
}

// ---------- type 多态联合（文本帧 JSON） ----------

export type ServerMsg =

/** 服务端未知 type → orch 回 BAD_MESSAGE；客户端同理容错解析。 */
export function parseServerMsg(text: string): ServerMsg | null {
  try {
    const msg = JSON.parse(text) as ServerMsg;
    return typeof msg.type === "string" ? msg : null;
  } catch {
    return null;
  }
}


export const CLOSE_NORMAL = 1000;
export const CLOSE_VERSION_MISMATCH = 4001;
export const CLOSE_PROTOCOL_VIOLATION = 4400;
export const CLOSE_INTERNAL = 1011;
