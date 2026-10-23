import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import {waitForRouteReady, reloadNativeApp} from './ui-ready.mjs';

// Credentials are supplied through a private ignored fixture, never source or
// test output. This is an opt-in real provider check, separate from fixtures.
const enabled = Boolean(process.env.MOMO_E2E_LLM_CONFIG);
async function api(path, method = 'GET', body = null) {
  return browser.execute(async (path, method, body) => {
    const runtime = await window.__TAURI__.core.invoke('get_runtime');
    const response = await fetch(runtime.baseUrl + path, {method,
      headers:{'x-momo-token':runtime.token,'content-type':'application/json'},
      ...(method === 'GET' || body == null ? {} : {body:JSON.stringify(body)})});
    if (!response.ok) throw new Error(`Native API HTTP ${response.status}`);
    const text = await response.text(); return text ? JSON.parse(text) : null;
  }, path, method, body);
}
(enabled ? describe : describe.skip)('real provider through the installed native app', () => {
  it('receives clean dialogue over real HTTPS and displays it in the WebView', async () => {
    await $('.title-screen, .chat-page').waitForDisplayed({timeout:60000});
    const llm = JSON.parse(await fs.readFile(process.env.MOMO_E2E_LLM_CONFIG,'utf8'));
    const previous = await api('/api/state');
    try {
      await api('/api/state','PUT',{...previous,settings:{...previous.settings,llm}});
      await reloadNativeApp();
      await browser.execute(()=>{location.hash='/chat';});
      await $('.input-row input').waitForDisplayed();
      await waitForRouteReady('chat');
      await $('.input-row input').setValue('你好 momo，今天想和你打个招呼。简单回应一句就好。');
      await browser.keys('Enter');
      await browser.waitUntil(async()=>{
        const live=await api('/api/checkpoints/live');
        return live.messages.some(message=>message.role==='user' && message.text.includes('简单回应一句')) &&
          live.messages.at(-1)?.role==='assistant';
      },{timeout:90000});
      await browser.waitUntil(async()=>await $('button[aria-label="发送"]').isExisting(),{timeout:90000});
      const live=await api('/api/checkpoints/live');
      const answer=live.messages.at(-1).text;
      assert.ok(answer.trim().length>0);
      assert.ok(!answer.includes('<|ACT') && !answer.includes('<think>'));
      assert.ok(!(await $('.chat-error').isExisting()));
      await fs.mkdir('dist/native-e2e',{recursive:true});
      await browser.saveScreenshot('dist/native-e2e/real-provider.png');
      console.log('Real native provider: HTTPS/SSE, persisted clean dialogue and playback completion passed');
    } finally {
      await api('/api/chat/interrupt','POST',{});
      await api('/api/state','PUT',previous);
    }
  });
});
