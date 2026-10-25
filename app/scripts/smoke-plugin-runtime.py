"""Exercise the frozen Python → bundled Node lifecycle with isolated data."""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import secrets
import select
import signal
import subprocess
import tempfile
import time
import urllib.request


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('resources', type=Path)
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    resources = args.resources.resolve()
    node, backend = resources / 'node/bin/node', resources / 'backend/momo-backend'
    assert subprocess.check_output([str(node), '--version'], text=True).strip() == 'v22.23.3'
    env = {key: value for key, value in os.environ.items() if key not in {'MOMO_NODE', 'MOMO_PLUGIN_HOST'}}
    # A system Node cannot satisfy this test. The frozen runtime must use the
    # resource directory, including when launched from a clean desktop PATH.
    env.update(PATH='/usr/bin:/bin:/usr/sbin:/sbin', HF_HUB_OFFLINE='1',
               MOMO_DESKTOP_TOKEN=secrets.token_hex(32), MOMO_DESKTOP_NONCE=secrets.token_hex(32))
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    with tempfile.TemporaryDirectory(prefix='momo-v3-frozen-') as data:
        command = [str(backend), '--data-dir', data, '--resources-dir', str(resources / 'models'), '--port', '0']
        process = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                   stderr=subprocess.DEVNULL, text=True, env=env, cwd=data, start_new_session=True)
        node_pid = None
        try:
            assert select.select([process.stdout], [], [], 45)[0], 'Frozen backend readiness timed out'
            ready = json.loads(process.stdout.readline())
            assert ready['type'] == 'momo-ready' and ready['nonce'] == env['MOMO_DESKTOP_NONCE']
            assert ready['pid'] == process.pid
            base = f'http://127.0.0.1:{ready["port"]}'
            def request(path, body=None):
                headers = {'x-momo-token': env['MOMO_DESKTOP_TOKEN'], 'Origin': 'tauri://localhost'}
                raw = None if body is None else json.dumps(body).encode()
                if raw is not None: headers['content-type'] = 'application/json'
                with opener.open(urllib.request.Request(base + path, headers=headers, data=raw), timeout=10) as response:
                    return json.load(response)
            assert request('/api/health')['ok']
            children = subprocess.check_output(['/usr/bin/pgrep', '-P', str(process.pid)], text=True).split()
            assert len(children) == 1, 'Expected exactly one owned plugin host'
            node_pid = int(children[0])
            executable = subprocess.check_output(['/bin/ps', '-p', str(node_pid), '-o', 'command='], text=True)
            assert str(node) in executable and str(resources / 'plugin-host/src/index.mjs') in executable
            lease = request('/api/meeting/enter', {'clientId': 'native-frozen-smoke'})
            assert lease['mode'] == 'meeting' and request('/api/meeting')['mode'] == 'meeting'
            assert request('/api/meeting/leave', {'clientId': 'native-frozen-smoke', 'leaseId': lease['leaseId']})['ok']
            assert request('/api/meeting')['mode'] == 'daily'
            duplicate = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.DEVNULL,
                                         stderr=subprocess.DEVNULL, env=env, cwd=data, start_new_session=True)
            try:
                assert duplicate.wait(timeout=15) != 0, 'Second coordinator unexpectedly acquired the life directory'
            finally:
                if duplicate.poll() is None: os.killpg(duplicate.pid, signal.SIGKILL); duplicate.wait()
                duplicate.stdin.close()
            assert request('/api/health')['ok']
            process.stdin.close()
            assert process.wait(timeout=15) == 0, 'Frozen backend failed clean EOF shutdown'
            for _ in range(50):
                try: os.kill(node_pid, 0)
                except ProcessLookupError: break
                time.sleep(.1)
            else: raise AssertionError('Owned Node process survived backend shutdown')
            result = {'ok': True, 'nodeVersion': '22.23.3', 'systemNodeUnavailable': True,
                      'meetingRevokeGrant': True, 'secondCoordinatorDenied': True,
                      'eofStoppedOwnedHost': True, 'isolatedProfile': True}
            if args.output:
                args.output.parent.mkdir(parents=True, exist_ok=True)
                args.output.write_text(json.dumps(result, indent=2) + '\n')
            print(json.dumps(result))
        finally:
            if process.poll() is None:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait()


if __name__ == '__main__': main()
