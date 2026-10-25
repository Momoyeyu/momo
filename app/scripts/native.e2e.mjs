import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import { spawn, execFileSync } from 'node:child_process';

describe('installed native application', () => {
  it('loads the local WebView and authenticates its owned backend', async () => {
    await browser.waitUntil(async () => browser.execute(async () => {
      try {
        const runtime = await window.__TAURI__.core.invoke('get_runtime');
        const response = await fetch(`${runtime.baseUrl}/api/health`, {headers: {'x-momo-token': runtime.token}});
        return response.ok && (await response.json()).ok;
      } catch { return false; }
    }), {timeout: 60000, timeoutMsg: 'Packaged backend did not become ready'});
    await browser.waitUntil(async () => browser.execute(() => !document.getElementById('startup') && !document.getElementById('root')?.inert),
      {timeout: 60000, timeoutMsg: 'Native startup did not release the prepared game scene'});
    const result = await browser.execute(async () => {
      const runtime = await window.__TAURI__.core.invoke('get_runtime');
      const headers = {'x-momo-token': runtime.token};
      const resources = await fetch(`${runtime.baseUrl}/api/resources/status`, {headers}).then(r => r.json());
      const owner = JSON.parse(sessionStorage.getItem('momo.meeting-owner.v1') || 'null');
      const clientId = owner?.clientId || `native-e2e-${crypto.randomUUID()}`;
      const meeting = await fetch(`${runtime.baseUrl}/api/meeting/${owner ? 'heartbeat' : 'enter'}`, {
        method: 'POST', headers: {...headers, 'content-type': 'application/json'}, body: JSON.stringify(owner || {clientId}),
      }).then(r => r.json());
      const interrupted = await fetch(`${runtime.baseUrl}/api/chat/interrupt`, {
        method: 'POST', headers: {...headers, 'content-type': 'application/json'}, body: JSON.stringify({clientId, leaseId: meeting.leaseId})});
      const generation = interrupted.headers.get('x-momo-generation');
      const leaving = owner ? {ok: true, status: 200} : await fetch(`${runtime.baseUrl}/api/meeting/leave`, {
        method: 'POST', headers: {...headers, 'content-type': 'application/json'}, body: JSON.stringify({clientId, leaseId: meeting.leaseId}),
      });
      const rejected = await fetch(`${runtime.baseUrl}/api/health`).then(r => r.status);
      const core = await fetch('/live2d/live2dcubismcore.min.js').then(r => r.text());
      return {origin: location.origin, text: document.body.innerText.length,
        tokenBytes: runtime.token.length / 2, loopback: /^http:\/\/127\.0\.0\.1:\d+$/.test(runtime.baseUrl),
        asrReady: resources.asr.ready, embeddingReady: ['ready', 'loaded'].includes(resources.embedding.state),
        interruptStatus: interrupted.status, generationHeader: generation,
        meetingMode: meeting.mode, meetingDetail: meeting.detail, leavingStatus: leaving.status,
        generationReadable: interrupted.status === 204 && generation !== null && /^\d+$/.test(generation),
        coordinatedMeeting: meeting.mode === 'meeting' && Boolean(meeting.leaseId) && leaving.ok,
        rejected, bundledCore: core.includes('Live2DCubismCore')};
    });
    assert.match(result.origin, /^(tauri:\/\/localhost|https?:\/\/tauri\.localhost)$/);
    assert.equal(result.tokenBytes, 32);
    assert.equal(result.loopback, true);
    assert.ok(result.text > 0);
    assert.equal(result.asrReady, true);
    assert.equal(result.embeddingReady, true);
    assert.equal(result.rejected, 403);
    assert.equal(result.generationReadable, true, JSON.stringify(result));
    assert.equal(result.coordinatedMeeting, true);
    assert.equal(result.bundledCore, true);
    const output = path.resolve('dist/native-e2e');
    await fs.mkdir(output, {recursive: true});
    await browser.waitUntil(async () => browser.execute(() => {
      const dialog = document.querySelector('.dialog-zone');
      return !dialog || (!document.querySelector('.chat-page')?.inert && Number(getComputedStyle(dialog).opacity) > .99);
    }), {timeout: 10000, timeoutMsg: 'The native dialog did not finish its entry animation'});
    await browser.saveScreenshot(path.join(output, 'native-window.png'));
  });
  it('permits local model blobs and microphone worklet modules under CSP', async () => {
    const result = await browser.execute(async () => {
      const url = URL.createObjectURL(new Blob(['momo model bytes']));
      let data;
      try { data = await fetch(url).then(response => response.text()); }
      finally { URL.revokeObjectURL(url); }
      const context = new AudioContext();
      const script = URL.createObjectURL(new Blob([
        'registerProcessor("momo-csp-smoke",class extends AudioWorkletProcessor{process(){return false}})'
      ], {type: 'text/javascript'}));
      try {
        await context.audioWorklet.addModule(script);
        return {data, worklet: true};
      } finally {
        URL.revokeObjectURL(script);
        await context.close();
      }
    });
    assert.deepEqual(result, {data: 'momo model bytes', worklet: true});
  });
  it('keeps browser persistence on the fixed native origin after reload', async () => {
    await browser.execute(() => localStorage.setItem('momo-native-e2e-sentinel', 'persisted'));
    await browser.refresh();
    await browser.waitUntil(async () => browser.execute(() => document.readyState === 'complete'));
    assert.equal(await browser.execute(() => localStorage.getItem('momo-native-e2e-sentinel')), 'persisted');
    await browser.execute(() => localStorage.removeItem('momo-native-e2e-sentinel'));
  });
  it('uses its bundled Node host and refuses a second native instance', async () => {
    if (process.platform !== 'darwin') return;
    const binary = process.env.MOMO_E2E_APP;
    assert.ok(binary, 'Set MOMO_E2E_APP to the isolated native preview binary');
    const resources = path.resolve(path.dirname(binary), '../Resources');
    const node = path.join(resources, 'node/bin/node');
    assert.equal(execFileSync(node, ['--version'], {encoding: 'utf8'}).trim(), 'v22.23.3');
    const manifest = JSON.parse(await fs.readFile(path.join(resources, 'plugin-host/runtime-manifest.json'), 'utf8'));
    assert.equal(manifest.nodeArchiveSha256, '23b25245dcfb9af7262f8ff142e9e2e0af025368117329e7a7458a51e5922f53');
    const children = executable => execFileSync('/usr/bin/pgrep', ['-f', executable], {encoding: 'utf8'}).trim().split('\n').map(Number);
    const backend = path.join(resources, 'backend/momo-backend');
    const before = {backend: children(backend), host: children(node)};
    assert.equal(before.backend.length, 1);
    assert.equal(before.host.length, 1);
    const duplicate = spawn(binary, [], {stdio: 'ignore'});
    const status = await new Promise((resolve, reject) => {
      const timer = setTimeout(() => { duplicate.kill(); reject(new Error('Second instance failed to exit')); }, 10000);
      duplicate.once('error', error => { clearTimeout(timer); reject(error); });
      duplicate.once('exit', (code, signal) => { clearTimeout(timer); resolve({code, signal}); });
    });
    assert.equal(status.code, 0);
    assert.deepEqual({backend: children(backend), host: children(node)}, before);
    const output = path.resolve('dist/native-e2e');
    await fs.mkdir(output, {recursive: true});
    await fs.writeFile(path.join(output, 'v3-runtime.json'), JSON.stringify({nodeVersion: manifest.nodeVersion, nodeArchiveSha256: manifest.nodeArchiveSha256, singleInstance: true, ownedProcesses: before}, null, 2));
  });
});
