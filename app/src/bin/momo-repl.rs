//! `momo-repl`：dev-only 打字→TTS→出声 bring-up 工具（issue #23）。
//! 不挂 LLM/Session/Turn；`/quit` 或 EOF 退出。

use std::io::Write as _;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Instant;

use futures_util::StreamExt;
use momo_app::{
    config::Config,
    error::AppError,
    pipeline::{
        splitter::SentenceSplitter,
    },
    proto::message::new_id,
};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio_util::sync::CancellationToken;
use url::Url;

/// 下行音频契约口径（protocol §6：pcm_s16le 24kHz mono）。
const SAMPLE_RATE: u32 = 24_000;

#[derive(Debug, Default)]
struct Opts {
    endpoint: Option<Url>,
    out: Option<PathBuf>,
    no_play: bool,
    help: bool,
}

fn usage() -> &'static str {
}

fn parse_args() -> Result<Opts, String> {
    let mut opts = Opts::default();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--endpoint" => {
                let v = args.next().ok_or("--endpoint requires a value")?;
                opts.endpoint = Some(v.parse().map_err(|e| format!("bad --endpoint: {e}"))?);
            }
            "--out" => {
                let v = args.next().ok_or("--out requires a value")?;
                opts.out = Some(PathBuf::from(v));
            }
            "--no-play" => opts.no_play = true,
            "-h" | "--help" => opts.help = true,
            other => return Err(format!("unknown arg {other:?}")),
        }
    }
    Ok(opts)
}

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), AppError> {
    let opts = match parse_args() {
        Ok(opts) => opts,
        Err(err) => {
            eprintln!("{err}\n{}", usage());
            std::process::exit(2);
        }
    };
    if opts.help {
        eprintln!("{}", usage());
        return Ok(());
    }
    let config = Config::from_env()?;
    let endpoint = opts.endpoint.unwrap_or_else(|| config.tts_endpoint.clone());

    tts.connect().await?;
    eprintln!(
        "momo-repl: connected to {endpoint} (playback {})",
        if opts.no_play { "off" } else { "on" }
    );

    let player = if opts.no_play {
        None
    } else {
        Some(Player::try_new()?)
    };
    let mut captured: Vec<u8> = Vec::new();

    // Ctrl-C → 取消在途合成（client 发 control.cancel{target:turn}）并退出。
    let cancel = CancellationToken::new();
    tokio::spawn({
        let cancel = cancel.clone();
        async move {
            let _ = tokio::signal::ctrl_c().await;
            cancel.cancel();
        }
    });

    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    prompt();
    loop {
        let line = tokio::select! {
            biased;
            () = cancel.cancelled() => break,
            line = lines.next_line() => line?,
        };
        let Some(line) = line else { break };
        let line = line.trim();
        if line.is_empty() {
            prompt();
            continue;
        }
        if line == "/quit" {
            break;
        }
        speak_line(line, &tts, player.as_ref(), &mut captured, &cancel, &config).await;
        prompt();
        if cancel.is_cancelled() {
            break;
        }
    }

    if let Some(path) = opts.out.as_ref() {
        write_wav(path, &captured)?;
        eprintln!(
            "wrote {} captured: {} bytes pcm",
            path.display(),
            captured.len()
        );
    }
    Ok(())
}

fn prompt() {
    print!("momo> ");
    let _ = std::io::stdout().flush();
}

/// 一行 = 一回合：切句 → 逐句 synth_stream → 逐 chunk 送扬声器/WAV 缓冲。
async fn speak_line(
    line: &str,
    player: Option<&Player>,
    captured: &mut Vec<u8>,
    cancel: &CancellationToken,
    config: &Config,
) {
    let t0 = Instant::now();
    let mut splitter = SentenceSplitter::new(config.sent_soft_chars, config.sent_max_chars);
    let mut sentences = splitter.push(line);
    if let Some(rest) = splitter.flush() {
        sentences.push(rest);
    }
    let split_ms = t0.elapsed();
    if sentences.is_empty() {
        eprintln!("[line] nothing to speak (split {split_ms:.1?})");
        return;
    }

    let turn_id = new_id("t");
    for sent in &sentences {
        let t_synth = Instant::now();
        let req = SynthRequest {
            turn_id: turn_id.clone(),
            lang: "auto".into(),
            sentence: sent.clone(),
        };
        let mut stream = match tts.synth_stream(req, cancel.child_token()).await {
            Ok(s) => s,
            Err(err) => {
                eprintln!("[sent {}] synth request failed: {err}", sent.index);
                continue;
            }
        };

        let mut first_ms = None;
        let mut frames = 0usize;
        let mut bytes = 0usize;
        while let Some(item) = stream.next().await {
            match item {
                Ok(chunk) => {
                    first_ms.get_or_insert_with(|| t_synth.elapsed());
                    frames += 1;
                    bytes += chunk.payload.len();
                    captured.extend_from_slice(&chunk.payload);
                    if let Some(p) = player {
                        p.push(&chunk.payload);
                    }
                    if chunk.eos {
                        break;
                    }
                }
                Err(err) => {
                    eprintln!("[sent {}] stream error: {err}", sent.index);
                    break;
                }
            }
        }
        let synth_ms = t_synth.elapsed();

        let t_play = Instant::now();
        if let Some(p) = player {
            p.drain().await;
        }
        let play_ms = t_play.elapsed();

        let audio_s = bytes as f64 / 2.0 / SAMPLE_RATE as f64;
        eprintln!(
            "[sent {}] {:?}\n  split: {:.1?}\n  first chunk: {}\n  synth total: {:.1?} (frames={} pcm={}B audio={:.2}s)\n  play drain: {:.1?}",
            sent.index,
            sent.text,
            split_ms,
            first_ms.map_or("n/a".into(), |d| format!("{d:.1?}")),
            synth_ms,
            frames,
            bytes,
            audio_s,
            play_ms,
        );
    }
    eprintln!("[line] e2e {:.1?}", t0.elapsed());
}

/// 24kHz s16le mono → 默认输出设备（rodio `Sink` 内部重采样到设备格式）。
struct Player {
    sink: Arc<rodio::Sink>,
    // 持有即保活；drop 即停输出设备。
    _stream: rodio::OutputStream,
}

impl Player {
    fn try_new() -> Result<Self, AppError> {
        let (stream, handle) = rodio::OutputStream::try_default().map_err(|e| {
            AppError::Internal(format!(
                "no default audio output ({e}); use --no-play and/or --out FILE.wav"
            ))
        })?;
        let sink = rodio::Sink::try_new(&handle)
            .map_err(|e| AppError::Internal(format!("audio sink init: {e}")))?;
        Ok(Self {
            sink: Arc::new(sink),
            _stream: stream,
        })
    }

    fn push(&self, pcm: &[u8]) {
        let samples: Vec<i16> = pcm
            .chunks_exact(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]))
            .collect();
        self.sink
            .append(rodio::buffer::SamplesBuffer::new(1, SAMPLE_RATE, samples));
    }

    /// 等播放队列排空（spawn_blocking：sleep_until_end 是阻塞调用）。
    async fn drain(&self) {
        let sink = self.sink.clone();
        let _ = tokio::task::spawn_blocking(move || sink.sleep_until_end()).await;
    }
}

/// 裸 s16le PCM 包一层 RIFF/WAVE 头（24kHz mono，与 models `wav_wrap` 同口径）。
fn write_wav(path: &PathBuf, pcm: &[u8]) -> Result<(), AppError> {
    let mut header = [0u8; 44];
    let data_len = pcm.len() as u32;
    header[0..4].copy_from_slice(b"RIFF");
    header[4..8].copy_from_slice(&(36 + data_len).to_le_bytes());
    header[8..12].copy_from_slice(b"WAVE");
    header[12..16].copy_from_slice(b"fmt ");
    header[16..20].copy_from_slice(&16u32.to_le_bytes()); // fmt chunk 长度
    header[20..22].copy_from_slice(&1u16.to_le_bytes()); // PCM
    header[22..24].copy_from_slice(&1u16.to_le_bytes()); // mono
    header[24..28].copy_from_slice(&SAMPLE_RATE.to_le_bytes());
    header[28..32].copy_from_slice(&(SAMPLE_RATE * 2).to_le_bytes()); // byte rate
    header[32..34].copy_from_slice(&2u16.to_le_bytes()); // block align
    header[34..36].copy_from_slice(&16u16.to_le_bytes()); // bits
    header[36..40].copy_from_slice(b"data");
    header[40..44].copy_from_slice(&data_len.to_le_bytes());

    let mut f = std::fs::File::create(path)?;
    f.write_all(&header)?;
    f.write_all(pcm)?;
    Ok(())
}
