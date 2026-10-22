"""Small HTTP boundary around the pinned upstream inference handler.

No upstream FastAPI routes are mounted. Reference files belong to the response,
including the upstream worker/generator cleanup, rather than to the route call.
"""
from __future__ import annotations

import asyncio
import base64
import binascii
from contextlib import asynccontextmanager
from dataclasses import dataclass
import hmac
import json
import logging
import os
from pathlib import Path
import shutil
import tempfile
from typing import Any, Callable, Literal

import anyio
from fastapi import FastAPI, HTTPException, Request
from pydantic import BaseModel, ConfigDict, Field, ValidationError, field_validator
from starlette.responses import JSONResponse, Response

logger = logging.getLogger('momo.tts')
MAX_AUDIO_BYTES = 10 * 1024 * 1024


class TTSRequest(BaseModel):
    model_config = ConfigDict(extra='forbid', strict=True)
    text: str = Field(min_length=1, max_length=4000)
    text_lang: Literal['zh', 'en', 'ja', 'all_zh', 'all_ja', 'auto']
    ref_audio: str = Field(min_length=1)
    ref_audio_name: str = Field(min_length=1, max_length=120)
    prompt_text: str = Field(default='', max_length=4000)
    prompt_lang: Literal['zh', 'en', 'ja', 'all_zh', 'all_ja', 'auto']
    media_type: Literal['wav'] = 'wav'
    streaming_mode: bool | Literal[0, 1, 2, 3] = True
    text_split_method: Literal['cut0', 'cut1', 'cut2', 'cut3', 'cut4', 'cut5'] = 'cut5'
    parallel_infer: bool = True
    top_k: int = Field(default=15, ge=1, le=100)
    top_p: float = Field(default=1.0, gt=0, le=1)
    temperature: float = Field(default=1.0, gt=0, le=2)
    speed_factor: float = Field(default=1.0, ge=0.5, le=2)
    seed: int = Field(default=-1, ge=-1, le=2**32-1)

    @field_validator('streaming_mode', mode='before')
    @classmethod
    def strict_streaming_type(cls, value):
        if type(value) not in (bool, int):
            raise ValueError('streaming_mode must be a boolean or integer')
        return value

    @field_validator('text')
    @classmethod
    def nonblank(cls, value):
        if not value.strip():
            raise ValueError('text must not be blank')
        return value

    @field_validator('ref_audio_name')
    @classmethod
    def safe_filename(cls, value):
        if '/' in value or '\\' in value or ':' in value or '\x00' in value:
            raise ValueError('ref_audio_name must be a filename, not a path')
        if Path(value).suffix.lower() not in {'.wav', '.mp3', '.flac', '.ogg', '.m4a', '.aac'}:
            raise ValueError('supported reference formats: wav, mp3, flac, ogg, m4a, aac')
        return value


@dataclass
class Engine:
    handler: Callable | None = None
    state: str = 'loading'
    error: str | None = None


class OwnedResponse(Response):
    """Keep the reference until the upstream generator AND background finish."""
    def __init__(self, upstream: Response, directory: Path):
        super().__init__(status_code=upstream.status_code)
        self.upstream = upstream
        self.directory = directory
        self.background = upstream.background
        upstream.background = None

    async def __call__(self, scope, receive, send):
        try:
            await self.upstream(scope, receive, send)
        finally:
            # Upstream tts_handle shields dispatched worker calls. On disconnect,
            # its background callback also handles an iterator never started.
            with anyio.CancelScope(shield=True):
                try:
                    iterator = getattr(self.upstream, 'body_iterator', None)
                    close = getattr(iterator, 'aclose', None)
                    try:
                        if close:
                            await close()
                    finally:
                        if self.background:
                            await self.background()
                finally:
                    shutil.rmtree(self.directory)


def decode_audio(value: TTSRequest, maximum: int) -> bytes:
    if len(value.ref_audio) > 4 * ((maximum + 2) // 3):
        raise HTTPException(413, 'reference audio exceeds size limit')
    try:
        raw = base64.b64decode(value.ref_audio, validate=True)
    except (ValueError, binascii.Error):
        raise HTTPException(422, 'ref_audio must be strict base64') from None
    if not raw or len(raw) > maximum:
        raise HTTPException(413, 'reference audio exceeds size limit')
    suffix = Path(value.ref_audio_name).suffix.lower()
    valid = {
        '.wav': raw.startswith(b'RIFF') and raw[8:12] == b'WAVE',
        '.flac': raw.startswith(b'fLaC'),
        '.ogg': raw.startswith(b'OggS'),
        '.mp3': raw.startswith(b'ID3') or (len(raw) > 1 and raw[0] == 255 and raw[1] & 224 == 224),
        '.m4a': len(raw) >= 12 and raw[4:8] == b'ftyp',
        '.aac': len(raw) > 1 and raw[0] == 255 and raw[1] & 246 == 240,
    }
    if not valid[suffix]:
        raise HTTPException(422, 'reference audio does not match its format')
    # The upstream decoder validates the actual container/length before inference.
    return raw


def create_app(engine: Engine | None = None, *, loader: Callable | None = None,
               temp_root: Path | None = None, token: str = '',
               max_audio_bytes: int = MAX_AUDIO_BYTES) -> FastAPI:
    engine = engine or Engine()
    temp_root = Path(temp_root or tempfile.gettempdir())
    temp_root.mkdir(mode=0o700, parents=True, exist_ok=True)

    @asynccontextmanager
    async def lifespan(_app):
        task = None
        if loader:
            async def load():
                try:
                    engine.handler = await asyncio.to_thread(loader)
                    engine.state = 'ready'
                except Exception as error:
                    logger.exception('TTS initialization failed')
                    engine.state = 'failed'
                    engine.error = f'{type(error).__name__}: {error}'
            task = asyncio.create_task(load())
        try:
            yield
        finally:
            if task:
                # Model initialization must finish before its process exits.
                await task

    app = FastAPI(lifespan=lifespan, docs_url=None, redoc_url=None, openapi_url=None)

    def authorize(request):
        if token and not hmac.compare_digest(request.headers.get('authorization', '').encode(), f'Bearer {token}'.encode()):
            raise HTTPException(401, 'Bearer token required', headers={'WWW-Authenticate': 'Bearer'})

    @app.get('/healthz')
    async def health():
        return {'status': 'alive'}

    @app.get('/readyz')
    async def ready(request: Request):
        authorize(request)
        ok = engine.state == 'ready' and engine.handler is not None
        return JSONResponse({'status': engine.state, 'ready': ok, 'error': engine.error}, status_code=200 if ok else 503)

    @app.post('/tts')
    async def synthesize(request: Request):
        authorize(request)
        if engine.state != 'ready' or engine.handler is None:
            raise HTTPException(503, 'TTS model is not ready; check /readyz and container logs')
        maximum_body = 4 * ((max_audio_bytes + 2) // 3) + 64 * 1024
        body = bytearray()
        async for chunk in request.stream():
            if len(body) + len(chunk) > maximum_body:
                raise HTTPException(413, 'request body exceeds size limit')
            body.extend(chunk)
        try:
            value = TTSRequest.model_validate(json.loads(body))
        except (ValueError, ValidationError, UnicodeDecodeError):
            # Do not echo base64 audio, text or credentials through validation errors.
            raise HTTPException(422, 'invalid TTS request fields or values') from None
        raw = decode_audio(value, max_audio_bytes)
        directory = Path(tempfile.mkdtemp(prefix='momo-reference-', dir=temp_root))
        try:
            path = directory / ('reference' + Path(value.ref_audio_name).suffix.lower())
            path.write_bytes(raw)
            arguments = value.model_dump(exclude={'ref_audio', 'ref_audio_name'})
            arguments['ref_audio_path'] = str(path)
            response = await engine.handler(arguments, raw_streaming_mode=value.streaming_mode)
            return OwnedResponse(response, directory)
        except BaseException as error:
            shutil.rmtree(directory)
            if isinstance(error, Exception):
                logger.exception('TTS request failed')
                raise HTTPException(503, 'TTS inference failed; check container logs') from None
            raise

    return app


def production_app():
    from runtime import load_upstream
    return create_app(loader=load_upstream, token=os.environ.get('TTS_API_KEY', ''),
                      temp_root=Path(os.environ.get('TTS_TEMP_DIR', '/tmp/momo-tts')))
