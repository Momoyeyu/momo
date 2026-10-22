import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';

describe('installed native application', () => {
  it('loads the local WebView and authenticates its owned backend', async () => {
    await browser.waitUntil(async () => browser.execute(async () => {
      try {
        const runtime = await window.__TAURI__.core.invoke('get_runtime');
        const response = await fetch(`${runtime.baseUrl}/api/health`, {headers: {'x-momo-token': runtime.token}});
        return response.ok && (await response.json()).ok;
      } catch { return false; }
    }), {timeout: 60000, timeoutMsg: 'Packaged backend did not become ready'});
    const result = await browser.execute(async () => {
      const runtime = await window.__TAURI__.core.invoke('get_runtime');
      const headers = {'x-momo-token': runtime.token};
      const resources = await fetch(`${runtime.baseUrl}/api/resources/status`, {headers}).then(r => r.json());
      const interrupted = await fetch(`${runtime.baseUrl}/api/chat/interrupt`, {
        method: 'POST', headers: {...headers, 'content-type': 'application/json'}, body: '{}'});
      const generation = interrupted.headers.get('x-momo-generation');
      const rejected = await fetch(`${runtime.baseUrl}/api/health`).then(r => r.status);
      const core = await fetch('/live2d/live2dcubismcore.min.js').then(r => r.text());
      return {origin: location.origin, text: document.body.innerText.length,
        tokenBytes: runtime.token.length / 2, loopback: /^http:\/\/127\.0\.0\.1:\d+$/.test(runtime.baseUrl),
        asrReady: resources.asr.ready, embeddingReady: ['ready', 'loaded'].includes(resources.embedding.state),
        generationReadable: interrupted.status === 204 && generation !== null && /^\d+$/.test(generation),
        rejected, bundledCore: core.includes('Live2DCubismCore')};
    });
    assert.match(result.origin, /^(tauri:\/\/localhost|https?:\/\/tauri\.localhost)$/);
    assert.equal(result.tokenBytes, 32);
    assert.equal(result.loopback, true);
    assert.ok(result.text > 0);
    assert.equal(result.asrReady, true);
    assert.equal(result.embeddingReady, true);
    assert.equal(result.rejected, 403);
    assert.equal(result.generationReadable, true);
    assert.equal(result.bundledCore, true);
    const output = path.resolve('dist/native-e2e');
    await fs.mkdir(output, {recursive: true});
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
});
