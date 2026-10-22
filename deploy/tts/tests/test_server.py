import asyncio
import base64
import io
import json
import wave
from pathlib import Path

import httpx
import pytest
from fastapi.testclient import TestClient
from starlette.background import BackgroundTask
from starlette.responses import Response, StreamingResponse

from server import Engine, OwnedResponse, create_app


def audio():
    result = io.BytesIO()
    with wave.open(result, 'wb') as output:
        output.setparams((1, 2, 16000, 0, 'NONE', 'not compressed'))
        output.writeframes(b'\0\0' * 16000)
    return result.getvalue()


def payload(**updates):
    return dict(text='你好', text_lang='zh', prompt_text='', prompt_lang='zh',
                ref_audio=base64.b64encode(audio()).decode(), ref_audio_name='voice.wav',
                streaming_mode=True, media_type='wav', **updates)


def client(tmp_path, handler, **options):
    return TestClient(create_app(Engine(handler=handler, state='ready'), temp_root=tmp_path, **options))


def test_normal_request_owns_safe_unique_file_until_response_finishes(tmp_path):
    paths = []
    async def handler(request, **_):
        assert 'ref_audio' not in request
        assert 'ref_audio_name' not in request
        path = Path(request['ref_audio_path']); paths.append(path)
        assert path.read_bytes() == audio()
        assert path.name == 'reference.wav'
        async def chunks():
            assert path.exists(); yield b'RIFF'
            assert path.exists(); yield b'audio'
        async def cleanup():
            assert path.exists()
        return StreamingResponse(chunks(), media_type='audio/wav', background=BackgroundTask(cleanup))
    with client(tmp_path, handler) as api:
        for _ in range(2):
            assert api.post('/tts', json=payload()).content == b'RIFFaudio'
            assert list(tmp_path.iterdir()) == []
    assert paths[0] != paths[1]


@pytest.mark.parametrize('updates,status', [
    ({'ref_audio': 'bad***'}, 422), ({'ref_audio': ''}, 422),
    ({'ref_audio': base64.b64encode(b'not audio').decode()}, 422),
    ({'ref_audio_name': '../../evil.wav'}, 422), ({'ref_audio_name': 'C:\\evil.wav'}, 422),
    ({'ref_audio_name': 'voice.exe'}, 422), ({'ref_audio_path': '/etc/passwd'}, 422),
    ({'aux_ref_audio_paths': ['/etc/passwd']}, 422), ({'unknown': True}, 422),
    ({'streaming_mode': 9}, 422), ({'streaming_mode': 1.0}, 422), ({'text': ' '}, 422), ({'media_type': 'raw'}, 422),
])
def test_rejects_malformed_or_path_based_requests_without_tempfiles(tmp_path, updates, status):
    async def forbidden(*_, **__):
        raise AssertionError('invalid request reached inference')
    with client(tmp_path, forbidden) as api:
        request = payload(); request.update(updates)
        assert api.post('/tts', json=request).status_code == status
        assert list(tmp_path.iterdir()) == []


def test_size_and_auth_limits_are_applied_before_materializing(tmp_path):
    async def handler(*_, **__):
        return Response(b'audio')
    with client(tmp_path, handler, token='secret', max_audio_bytes=100) as api:
        assert api.post('/tts', json=payload()).status_code == 401
        headers = {'Authorization': 'Bearer secret'}
        assert api.post('/tts', json=payload(), headers=headers).status_code == 413
        assert api.get('/readyz', headers=headers).status_code == 200
        assert api.get('/readyz').status_code == 401
        assert api.get('/healthz').status_code == 200
        assert list(tmp_path.iterdir()) == []


def test_failures_and_nonstreaming_responses_release_files(tmp_path):
    async def failed(*_, **__):
        raise RuntimeError('inference failed')
    with client(tmp_path, failed) as api:
        assert api.post('/tts', json=payload()).status_code == 503
        assert list(tmp_path.iterdir()) == []
    async def upstream_error(*_, **__):
        return Response('invalid reference', status_code=400)
    with client(tmp_path, upstream_error) as api:
        result = api.post('/tts', json=payload())
        assert result.status_code == 400 and result.text == 'invalid reference'
        assert list(tmp_path.iterdir()) == []


def test_only_liveness_readiness_and_synthesis_are_exposed(tmp_path):
    engine = Engine(state='loading')
    with TestClient(create_app(engine, temp_root=tmp_path)) as api:
        assert api.get('/healthz').status_code == 200
        assert api.get('/readyz').status_code == 503
        assert api.post('/tts', json=payload()).status_code == 503
        for path in ['/docs', '/openapi.json', '/control', '/set_gpt_weights', '/set_sovits_weights']:
            assert api.get(path).status_code == 404
        assert api.get('/tts').status_code == 405


@pytest.mark.asyncio
async def test_disconnect_closes_upstream_then_deletes_reference(tmp_path):
    events = []; directory = tmp_path / 'request'; directory.mkdir()
    reference = directory / 'reference.wav'; reference.write_bytes(audio())
    async def stream():
        try:
            yield b'header'
            await asyncio.Event().wait()
        finally:
            assert reference.exists(); events.append('generator closed')
    async def background():
        assert reference.exists(); events.append('lock released')
    response = OwnedResponse(StreamingResponse(stream(), background=BackgroundTask(background)), directory)
    async def send(message):
        if message['type'] == 'http.response.body':
            raise OSError('client disconnected')
    async def receive():
        await asyncio.Event().wait()
    with pytest.raises(Exception):
        await response({'type':'http', 'asgi':{'spec_version':'2.4'}}, receive, send)
    assert events == ['generator closed', 'lock released']
    assert not directory.exists()


@pytest.mark.asyncio
async def test_cancellation_before_first_chunk_still_releases_upstream(tmp_path):
    directory = tmp_path / 'request'; directory.mkdir()
    events = []
    async def stream():
        await asyncio.Event().wait()
        yield b'unreachable'
    async def background():
        assert directory.exists(); events.append('released')
    response = OwnedResponse(StreamingResponse(stream(), background=BackgroundTask(background)), directory)
    async def ignore(_): pass
    async def receive(): await asyncio.Event().wait()
    task = asyncio.create_task(response({'type':'http','asgi':{'spec_version':'2.4'}},receive,ignore))
    await asyncio.sleep(0.01); task.cancel()
    with pytest.raises(asyncio.CancelledError): await task
    assert events == ['released']
    assert not directory.exists()


def test_desktop_aac_reference_format_is_forwarded_without_filename_paths(tmp_path):
    async def handler(request, **_):
        assert Path(request['ref_audio_path']).suffix == '.aac'
        return Response(b'audio')
    request=payload();request.update(ref_audio_name='voice.aac',ref_audio=base64.b64encode(b'\xff\xf1\x50\x80\x00\x1f\xfc').decode())
    with client(tmp_path,handler) as api:
        assert api.post('/tts',json=request).status_code==200


@pytest.mark.asyncio
async def test_simultaneous_same_name_uploads_remain_isolated(tmp_path):
    paths=[];both=asyncio.Event()
    async def handler(request,**_):
        path=Path(request['ref_audio_path']);paths.append(path)
        if len(paths)==2:both.set()
        await asyncio.wait_for(both.wait(),2)
        assert all(p.exists() for p in paths)
        async def chunks():
            assert path.read_bytes()==audio();yield b'audio'
        return StreamingResponse(chunks())
    app=create_app(Engine(handler=handler,state='ready'),temp_root=tmp_path)
    async with httpx.AsyncClient(transport=httpx.ASGITransport(app=app),base_url='http://test') as api:
        first,second=await asyncio.gather(api.post('/tts',json=payload()),api.post('/tts',json=payload()))
    assert first.status_code==second.status_code==200
    assert paths[0]!=paths[1]
    assert not list(tmp_path.iterdir())


@pytest.mark.asyncio
async def test_cancel_while_handler_is_waiting_releases_request_directory(tmp_path):
    entered=asyncio.Event()
    async def handler(request,**_):
        assert Path(request['ref_audio_path']).exists();entered.set()
        await asyncio.Event().wait()
    app=create_app(Engine(handler=handler,state='ready'),temp_root=tmp_path)
    async with httpx.AsyncClient(transport=httpx.ASGITransport(app=app),base_url='http://test') as api:
        task=asyncio.create_task(api.post('/tts',json=payload()))
        await asyncio.wait_for(entered.wait(),2);task.cancel()
        with pytest.raises(asyncio.CancelledError):await task
    assert not list(tmp_path.iterdir())
