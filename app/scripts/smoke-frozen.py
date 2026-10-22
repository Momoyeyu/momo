"""Exercise the installed sidecar without importing developer backend modules."""
import argparse
import json
import os
from pathlib import Path
import secrets
import socket
import re
import sys
import subprocess
import tempfile
import asyncio
import time
import urllib.request
import urllib.error

parser = argparse.ArgumentParser()
parser.add_argument('resources', type=Path)
parser.add_argument('--require-offline', action='store_true')
args = parser.parse_args()
if args.require_offline:
    try:
        with socket.create_connection(('1.1.1.1', 443), timeout=2):
            raise AssertionError('External network remains available')
    except PermissionError:
        print('PASS operating system denies external networking')
root = args.resources.resolve()
assert (root / 'backend/_internal/resources/soul.md').read_text(encoding='utf-8').strip(), 'Bundled fixed soul is missing'
assert b'Live2DCubismCore' in (root / 'models/live2d/live2dcubismcore.min.js').read_bytes(), 'Bundled Core is missing'
print('PASS bundled fixed soul and Cubism Core')
token, nonce = secrets.token_hex(32), secrets.token_hex(32)
env = {**os.environ, 'MOMO_DESKTOP_TOKEN': token, 'MOMO_DESKTOP_NONCE': nonce}
with tempfile.TemporaryDirectory(prefix='momo frozen 中文 ') as data:
    process = subprocess.Popen([str(root / 'backend' / ('momo-backend.exe' if os.name == 'nt' else 'momo-backend')),
        '--data-dir', data, '--resources-dir', str(root / 'models'), '--port', '0'],
        stdin=subprocess.PIPE, stdout=subprocess.PIPE, env=env, text=True, cwd=data)
    try:
        ready = json.loads(process.stdout.readline())
        assert ready['type'] == 'momo-ready' and ready['nonce'] == nonce and ready['pid'] == process.pid
        base = f"http://127.0.0.1:{ready['port']}"
        def request(path, authorized=True, origin='tauri://localhost'):
            headers = {'Origin': origin}
            if authorized:
                headers['x-momo-token'] = token
            return urllib.request.build_opener(urllib.request.ProxyHandler({})).open(urllib.request.Request(base + path, headers=headers), timeout=30)
        try:
            request('/api/health', False)
            raise AssertionError('Unauthenticated request accepted')
        except urllib.error.HTTPError as exc:
            assert exc.code in (401, 403)
            assert exc.headers.get('Access-Control-Allow-Origin') == 'tauri://localhost'
        try:
            request('/api/health', origin='https://untrusted.invalid')
            raise AssertionError('Untrusted browser origin accepted')
        except urllib.error.HTTPError as exc:
            assert exc.code == 403 and exc.headers.get('Access-Control-Allow-Origin') is None
        print('PASS trusted-origin CORS on denial and untrusted-origin rejection')
        with request('/api/health') as response:
            exposed = {name.strip().lower() for name in response.headers.get('Access-Control-Expose-Headers', '').split(',')}
            assert {'x-sample-rate', 'x-momo-generation'} <= exposed, exposed
        print('PASS native sample-rate and generation header exposure')
        for path in ['/api/health', '/api/asr/status', '/api/state', '/api/memories']:
            with request(path) as response:
                value = json.load(response)
                print(path, json.dumps(value, ensure_ascii=False)[:400])
        memory = urllib.request.Request(base + '/api/memories', headers={'x-momo-token': token, 'content-type': 'application/json'},
            data=json.dumps({'content': '我们喜欢在夜晚一起看星星'}).encode())
        urllib.request.build_opener(urllib.request.ProxyHandler({})).open(memory, timeout=30).close()
        for _ in range(120):
            with request('/api/resources/status') as response:
                status = json.load(response)
            if status['index']['indexed'] == 1:
                break
            if status['index']['state'] == 'degraded':
                raise AssertionError(status)
            time.sleep(.5)
        assert status['embedding']['state'] == 'loaded' and status['index']['indexed'] == 1, status
        print('PASS actual frozen E5 inference and sqlite-vec index')
        async def asr():
            import websockets
            async with websockets.connect(base.replace('http:', 'ws:') + '/api/asr/stream', origin='tauri://localhost', proxy=None) as ws:
                await ws.send(json.dumps({'type': 'authenticate', 'token': token}))
                ready = json.loads(await asyncio.wait_for(ws.recv(), timeout=60))
                assert ready['type'] == 'ready', ready
                await ws.send(bytes(32000))
                await ws.send(json.dumps({'type': 'finalize'}))
                for _ in range(5):
                    msg = json.loads(await asyncio.wait_for(ws.recv(), timeout=30))
                    assert msg['type'] != 'error', msg
                    if msg['type'] == 'finalized':
                        break
                else:
                    raise AssertionError('ASR failed to finalize')
                await ws.send(json.dumps({'type': 'close'}))
        asyncio.run(asr())
        print('PASS actual frozen SenseVoice/VAD session and silence finalization')
        if os.name == 'nt':
            powershell = Path(os.environ['SystemRoot']) / 'System32/WindowsPowerShell/v1.0/powershell.exe'
            query = ("[Console]::OutputEncoding=[System.Text.UTF8Encoding]::new(); "
                f"(Get-Process -Id {process.pid}).Modules | "
                "Where-Object { $_.ModuleName -match '^(vcruntime[0-9]|msvcp[0-9]|onnxruntime[.]dll$|python[0-9])' } | "
                "ForEach-Object { $s=Get-AuthenticodeSignature $_.FileName; "
                "[pscustomobject]@{name=$_.ModuleName;path=$_.FileName;version=$_.FileVersionInfo.FileVersion;"
                "signature=[string]$s.Status;signer=[string]$s.SignerCertificate.Subject} } | ConvertTo-Json -Compress")
            modules = json.loads(subprocess.check_output([str(powershell), '-NoProfile', '-Command', query]))
            if isinstance(modules, dict):
                modules = [modules]
            assert any(module['name'].lower() == 'onnxruntime.dll' for module in modules)
            subprocess.run([sys.executable, str(Path(__file__).with_name('audit-windows.py')), str(root)], check=True)
            manifest = json.loads((root / 'backend/licenses/windows-runtime-provenance.json').read_text(encoding='utf-8'))
            canonical = {record['name'].lower() for record in manifest['files']}
            minimum = tuple(map(int, manifest['version'].split('.')))
            system32 = (Path(os.environ['SystemRoot']) / 'System32').resolve()
            for module in modules:
                file = Path(module['path']).resolve()
                if file.is_relative_to(root / 'backend'):
                    continue
                # Windows may preload a compatible signed system CRT before
                # the application starts. The packaged fallback must still
                # pass exact hash and imported-symbol closure checks above.
                assert module['name'].lower() in canonical and file.parent == system32, module
                assert module['signature'] == 'Valid' and 'o=microsoft corporation' in module['signer'].lower(), module
                version = re.search(r'\d+\.\d+\.\d+(?:\.\d+)?', module['version'])
                assert version and tuple(map(int, version.group().split('.'))) >= minimum, module
            print('Observed native module provenance: ' + json.dumps(modules))
            print('PASS packaged Python/ORT and canonical CRT closure; compatible system CRT observations retained')
        process.stdin.close()
        assert process.wait(timeout=10) == 0
        print('PASS frozen startup, auth, resources, SQLite, Unicode directory, EOF shutdown')
    finally:
        if process.poll() is None:
            process.kill()
            process.wait()
