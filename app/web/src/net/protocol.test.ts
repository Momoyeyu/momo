
import { describe, expect, it } from "vitest";
import {
  AUDIO_HEADER_LEN,
  FLAG_EOS,
  decodeAudioFrame,
  encodeAudioFrame,
  newId,
  parseServerMsg,
} from "./protocol";

  it("编码/解码 roundtrip", () => {
    const payload = new Uint8Array([1, 2, 3, 4]);
    expect(frame.length).toBe(AUDIO_HEADER_LEN + payload.length);
    const decoded = decodeAudioFrame(frame);
    expect(Array.from(decoded!.payload)).toEqual([1, 2, 3, 4]);
  });

  });
});

    const msg = parseServerMsg(
      JSON.stringify({
        type: "hello.ok",
        role: "orchestrator",
      }),
    );
    expect(msg?.type).toBe("hello.ok");
    if (msg?.type === "hello.ok") {
      expect(msg.role).toBe("orchestrator");
      expect(msg.session_id).toMatch(/^sess_/);
    }
  });

  });

  });
});
