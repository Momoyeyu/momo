"""Readiness and short real-synthesis check; prints metrics, never input/audio."""
from __future__ import annotations
import argparse
import base64
import json
import http.client
import os
from pathlib import Path
import struct
import time
import urllib.error
import urllib.request
import wave


def read_exact(stream, length):
    parts = bytearray()
    while len(parts) < length:
        value = stream.read(length - len(parts))
        if not value:
            raise ValueError('truncated WAV header')
        parts.extend(value)
    return bytes(parts)


def inspect_audio_stream(stream, minimum_seconds=.5, *, started=None, capture=False):
    if minimum_seconds <= 0:
        raise ValueError('minimum_seconds must be positive')
    started = started or time.monotonic()
    header = read_exact(stream, 12)
    if header[:4] != b'RIFF' or header[8:] != b'WAVE':
        raise ValueError('expected a RIFF/WAVE response')
    rate = None; scanned = 12
    while True:
        kind, length = struct.unpack('<4sI', read_exact(stream, 8)); scanned += 8
        if kind == b'data':
            declared_length = length
            break
        if length > 65536 or scanned + length > 65536:
            raise ValueError('WAV metadata exceeds header size limit')
        value = read_exact(stream, length + length % 2); scanned += len(value)
        if kind == b'fmt ':
            if len(value) < 16:
                raise ValueError('invalid WAV format header')
            encoding, channels, rate, _, _, bits = struct.unpack('<HHIIHH', value[:16])
            if encoding != 1 or channels != 1 or bits != 16 or not 8000 <= rate <= 192000:
                raise ValueError('expected mono PCM16 at a valid sample rate')
    if rate is None:
        raise ValueError('WAV response has no format information')
    count = 0; first_pcm = None; captured = bytearray() if capture else None
    while True:
        block = stream.read(2 if first_pcm is None else 16384)
        if not block:
            break
        if first_pcm is None:
            first_pcm = time.monotonic() - started
        count += len(block)
        if count > 64 * 1024 * 1024:
            raise ValueError('short synthesis check exceeded 64 MiB')
        if capture:
            captured.extend(block)
    seconds = count / (rate * 2)
    if count % 2 or seconds < minimum_seconds:
        raise ValueError(f'audio too short or truncated: {seconds:.3f}s; expected at least {minimum_seconds:.3f}s')
    if declared_length not in [0, 0xffffffff] and count != declared_length:
        raise ValueError('truncated WAV payload does not match its declared length')
    result = dict(sample_rate=rate, pcm_bytes=count, seconds=seconds, first_pcm_seconds=first_pcm)
    if capture:
        result['pcm'] = captured
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--url', default='http://127.0.0.1:9880')
    parser.add_argument('--ready-only', action='store_true')
    parser.add_argument('--reference', type=Path)
    parser.add_argument('--text', default='你好，很高兴认识你。')
    parser.add_argument('--lang', choices=['zh', 'en', 'ja'], default='zh')
    parser.add_argument('--prompt-text', default='')
    parser.add_argument('--prompt-lang', choices=['zh', 'en', 'ja'], help='reference language; defaults to --lang')
    parser.add_argument('--minimum-seconds', type=float, default=.5)
    parser.add_argument('--output', type=Path, help='optional local WAV output; contains reference-derived generated speech')
    args = parser.parse_args()
    token = os.environ.get('TTS_API_KEY', '')
    headers = {'Authorization': f'Bearer {token}'} if token else {}
    try:
        ready_request = urllib.request.Request(args.url.rstrip('/') + '/readyz', headers=headers)
        with urllib.request.urlopen(ready_request, timeout=10) as response:
            status = json.load(response)
        if status.get('ready') is not True:
            raise ValueError('model is not ready; inspect container logs')
        if args.ready_only:
            print(json.dumps(status));return
        if not args.reference:
            parser.error('--reference is required for synthesis; use --ready-only for readiness')
        if args.reference.stat().st_size > 10 * 1024 * 1024:
            raise ValueError('reference audio exceeds 10 MiB')
        value = dict(text=args.text, text_lang=args.lang, prompt_lang=args.prompt_lang or args.lang,
                     prompt_text=args.prompt_text, ref_audio_name=args.reference.name,
                     ref_audio=base64.b64encode(args.reference.read_bytes()).decode('ascii'),
                     streaming_mode=True, media_type='wav', text_split_method='cut5', parallel_infer=True)
        request = urllib.request.Request(args.url.rstrip('/') + '/tts', data=json.dumps(value).encode(),
                                         headers={**headers, 'Content-Type': 'application/json'})
        started = time.monotonic()
        with urllib.request.urlopen(request, timeout=180) as response:
            result = inspect_audio_stream(response, args.minimum_seconds, started=started, capture=bool(args.output))
        result['total_seconds'] = time.monotonic() - started
        result['real_time_factor'] = result['total_seconds'] / result['seconds']
        pcm = result.pop('pcm', None)
        if args.output:
            with wave.open(str(args.output), 'wb') as output:
                output.setparams((1, 2, result['sample_rate'], 0, 'NONE', 'not compressed'))
                output.writeframes(pcm)
        print(json.dumps(result, indent=2))
    except urllib.error.HTTPError as error:
        details = error.read(512).decode('utf-8', errors='replace')
        parser.exit(1, f'TTS check failed: HTTP {error.code}; {details}\n')
    except (OSError, ValueError, urllib.error.URLError, http.client.HTTPException) as error:
        parser.exit(1, f'TTS check failed: {error}\n')


if __name__ == '__main__':
    main()
