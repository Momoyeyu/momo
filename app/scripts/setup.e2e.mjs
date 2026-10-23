import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import {createServer} from 'node:http';
import {waitForRouteReady} from './ui-ready.mjs';

// Run on a fresh native-e2e profile. Credentials are opt-in and never logged.
async function api(path) {
  return browser.execute(async path => {
    const runtime = await window.__TAURI__.core.invoke('get_runtime');
    const response = await fetch(runtime.baseUrl + path, {headers: {'x-momo-token': runtime.token}});
    if (!response.ok) throw new Error(`Native API HTTP ${response.status}`);
    return response.json();
  }, path);
}
async function fillConnection(config) {
  for (const field of ['url', 'key', 'model']) await $(`#first-meeting-${field}`).setValue(config[field] ?? '');
}

(process.env.MOMO_E2E_SETUP === '1' ? describe : describe.skip)('first native model connection', () => {
  before(async () => {
    await browser.waitUntil(async () => browser.execute(() => !!document.querySelector('.app-stage') && !document.querySelector('#startup')));
    await browser.execute(() => { location.hash = '/chat'; });
    await waitForRouteReady('chat');
  });
  afterEach(async function () {
    if (this.currentTest.state !== 'failed') return;
    await browser.saveScreenshot('dist/native-e2e/setup-failed.png');
    console.log('Setup diagnostic', await browser.execute(() => ({
      route: location.hash,
      status: document.querySelector('.chat-status')?.textContent,
      error: document.querySelector('.chat-error')?.textContent,
      draftMessage: document.querySelector('.input-row input')?.value,
      setupVisible: !!document.querySelector('.first-meeting-setup'),
    })));
  });
  it('keeps a rejected model in setup without saving or starting dialogue', async () => {
    await $('.first-meeting-setup').waitForDisplayed({timeout:60000});
    assert.equal(await $('.first-meeting-help').isDisplayed(), false);
    const provider = createServer((request, response) => {
      request.resume();
      request.on('end', () => {
        response.writeHead(request.method === 'GET' ? 200 : 400, {'content-type':'application/json'});
        response.end(JSON.stringify(request.method === 'GET' ? {data:[]} : {error:{message:'Invalid model'}}));
      });
    });
    await new Promise(resolve => provider.listen(0, '127.0.0.1', resolve));
    try {
      await fillConnection({url:`http://127.0.0.1:${provider.address().port}`, key:'fixture-key', model:'WrongCase'});
      await $('.first-meeting-setup button[type="submit"]').click();
      await browser.waitUntil(async () => (await $('.first-meeting-error').getText()).includes('HTTP 400'));
      assert.equal(await $('#first-meeting-model').getValue(), 'WrongCase');
      assert.equal((await api('/api/onboarding')).configured, false);
      assert.equal((await api('/api/checkpoints/live')).messages.length, 0);
      await $('.first-meeting-help-toggle').click();
      assert.equal(await $('.first-meeting-provider a').getAttribute('href'), 'https://platform.deepseek.com/');
      await $('.first-meeting-help-toggle').click();
      await fs.mkdir('dist/native-e2e', {recursive:true});
      await browser.saveScreenshot('dist/native-e2e/setup-rejected-model.png');
    } finally {
      provider.closeAllConnections();
      await new Promise(resolve => provider.close(resolve));
    }
  });

  (process.env.MOMO_E2E_LLM_CONFIG ? it : it.skip)('validates the web provider configuration before opening, then completes real dialogue', async () => {
    const llm = JSON.parse(await fs.readFile(process.env.MOMO_E2E_LLM_CONFIG, 'utf8'));
    await fillConnection(llm);
    await $('.first-meeting-setup button[type="submit"]').click();
    await $('.first-meeting-setup').waitForExist({reverse:true, timeout:30000});
    await browser.waitUntil(async () => (await $('.subtitle').getText()).includes('怎么称呼'));
    await $('button[aria-label="发送"]').waitForExist({timeout:30000});
    const input = $('.input-row input');
    await input.setValue('叫我原生验证');
    await $('button[aria-label="发送"]').click();
    await browser.waitUntil(async () => (await api('/api/onboarding')).stage === 'completed', {timeout:90000});
    await browser.waitUntil(async () => $('button[aria-label="发送"]').isExisting(), {timeout:90000});
    await input.setValue('momo，请用一句话和我打个招呼。');
    await $('button[aria-label="发送"]').click();
    await browser.waitUntil(async () => {
      const messages = (await api('/api/checkpoints/live')).messages;
      return messages.some(message => message.role === 'user' && message.text.includes('一句话和我打个招呼')) && messages.at(-1)?.role === 'assistant';
    }, {timeout:90000});
    await browser.waitUntil(async () => $('button[aria-label="发送"]').isExisting(), {timeout:90000});
    assert.equal(await $('.chat-error').isExisting(), false);
    const answer = (await api('/api/checkpoints/live')).messages.at(-1).text;
    assert.ok(answer.trim().length > 0);
    assert.ok(!answer.includes('<|ACT') && !answer.includes('<think>'));
    await browser.saveScreenshot('dist/native-e2e/setup-real-dialogue.png');
  });
});
