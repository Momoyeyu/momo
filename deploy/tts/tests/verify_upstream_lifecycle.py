"""Exercise actual pinned tts_handle cancellation with fake model work (no GPU).

Usage: python tests/verify_upstream_lifecycle.py /path/to/clean/GPT-SoVITS
"""
import ast
import asyncio
import hashlib
from io import BytesIO
from pathlib import Path
import sys
import tempfile
import threading
import wave

import anyio
from starlette.background import BackgroundTask
from starlette.concurrency import run_in_threadpool
from starlette.responses import JSONResponse, Response, StreamingResponse

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from runtime import UPSTREAM_API_SHA256
from server import OwnedResponse


def upstream_functions(root):
    source = (root / 'api_v2.py').read_bytes()
    assert hashlib.sha256(source).hexdigest() == UPSTREAM_API_SHA256
    keep = {'_await_shielded', '_run_in_threadpool_shielded', '_next_or_end', '_next_tts_chunk', 'wave_header_chunk', 'tts_handle'}
    tree = ast.parse(source)
    tree.body = [node for node in tree.body if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)) and node.name in keep]
    namespace = dict(asyncio=asyncio, anyio=anyio, run_in_threadpool=run_in_threadpool,
                     _STREAM_END=object(), BytesIO=BytesIO, wave=wave,
                     Response=Response, JSONResponse=JSONResponse, StreamingResponse=StreamingResponse,
                     BackgroundTask=BackgroundTask, check_params=lambda req: None,
                     pack_audio=lambda *args: BytesIO(b'\0\0' * 1024))
    exec(compile(tree, 'pinned-api-v2-lifecycle', 'exec'), namespace)
    return namespace


async def verify(root):
    namespace = upstream_functions(root)
    for spec, block_on in [('2.3', 0), ('2.3', 1), ('2.4', 0), ('2.4', 1)]:
        with tempfile.TemporaryDirectory() as directory:
            reference = Path(directory) / 'request'
            reference.mkdir(); audio = reference / 'reference.wav'; audio.write_bytes(b'fixture')
            entered = threading.Event(); release = threading.Event(); closed = []
            class Chunk:
                size = 1024
            class Pipeline:
                def run(self, request):
                    try:
                        for index in range(3):
                            if index == block_on:
                                entered.set(); assert release.wait(5)
                            assert audio.exists()
                            yield 24000, Chunk()
                    finally:
                        assert audio.exists(); closed.append(True)
            lock = asyncio.Lock()
            namespace['_tts_pipeline_lock'] = lock
            namespace['tts_pipeline'] = Pipeline()
            result = await namespace['tts_handle']({'streaming_mode': True, 'media_type': 'wav', 'ref_audio_path': str(audio)})
            owned = OwnedResponse(result, reference)
            async def send(_): pass
            disconnect = asyncio.Event()
            async def receive():
                await disconnect.wait()
                return {'type': 'http.disconnect'}
            task = asyncio.create_task(owned({'type': 'http', 'asgi': {'spec_version': spec}}, receive, send))
            assert await asyncio.to_thread(entered.wait, 5)
            if spec == '2.3':
                disconnect.set()
            else:
                task.cancel()
            await asyncio.sleep(.02)
            assert audio.exists() and lock.locked() and not task.done()
            release.set()
            try:
                await task
                assert spec == '2.3', 'cancelled stream unexpectedly completed'
            except asyncio.CancelledError:
                assert spec == '2.4'
            assert closed and not lock.locked() and not reference.exists()
    print('Pinned upstream lifecycle: ASGI2.3 disconnect and ASGI2.4 cancellation, before first PCM and during later worker, all 4 passed')


if __name__ == '__main__':
    asyncio.run(verify(Path(sys.argv[1])))
