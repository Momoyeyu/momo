import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import {createServer} from 'node:http';

// Real GPU synthesis and real WebView audio; the language provider only supplies
// deterministic dialogue. Private reference audio/configuration never enters git.
const enabled = Boolean(process.env.MOMO_E2E_TTS_CONFIG);
async function runtime() { return browser.execute(() => window.__TAURI__.core.invoke('get_runtime')); }
async function api(path, method = 'GET', value) {
  const rt = await runtime();
  const response = await fetch(rt.baseUrl + path, {method,
    headers: {'x-momo-token':rt.token, ...(value instanceof FormData ? {} : {'content-type':'application/json'})},
    ...(value == null ? {} : {body:value instanceof FormData ? value : JSON.stringify(value)})});
  if (!response.ok) throw new Error(`Native API HTTP ${response.status}`);
  const text = await response.text(); return text ? JSON.parse(text) : null;
}

(enabled ? describe : describe.skip)('real TTS through the installed native app', () => {
  let provider;
  let previous;
  before(async () => {
    await $('.title-screen, .chat-page').waitForDisplayed({timeout:60000});
    provider = createServer((request, response) => {
      let raw = '';
      request.on('data', chunk => raw += chunk);
      request.on('end', () => {
        if (!request.url?.endsWith('/chat/completions')) { response.writeHead(404).end(); return; }
        const body = JSON.parse(raw);
        if (!body.stream) {
          response.setHeader('content-type','application/json');
          response.end(JSON.stringify({choices:[{message:{role:'assistant',content:'[]'}}]})); return;
        }
        const text = '<|ACT {"expression":{"name":"happy","intensity":0.7}}|>你好，很高兴和你一起看星星。今天的月亮也很漂亮。';
        response.setHeader('content-type','text/event-stream');
        response.write(`data: ${JSON.stringify({choices:[{delta:{content:text}}]})}\n\n`);
        response.end('data: [DONE]\n\n');
      });
    });
    await new Promise(resolve => provider.listen(0,'127.0.0.1',resolve));
    previous = await api('/api/state');
    const fixture = JSON.parse(await fs.readFile(process.env.MOMO_E2E_TTS_CONFIG,'utf8'));
    const form = new FormData();
    form.append('file',new Blob([await fs.readFile(fixture.reference)]),fixture.referenceName ?? 'reference.wav');
    const uploaded = await api('/api/tts/ref','POST',form);
    await api('/api/state','PUT',{...previous,settings:{...previous.settings,
      llm:{url:`http://127.0.0.1:${provider.address().port}`,key:'',model:'native-tts-fixture',thinking:false},
      tts:{url:fixture.url,key:fixture.key ?? '',voice:uploaded.name,prompt:fixture.prompt ?? ''},
      general:{...previous.settings?.general,lang:'zh',chatLang:'zh',voiceInput:false,voiceVolume:15,textSpeed:100,sentenceSpeed:100}}});
    assert.equal((await api('/api/tts/status')).reachable,true);
    await browser.refresh();
    await browser.execute(() => {location.hash='/chat';});
    await $('.input-row input').waitForDisplayed();
    await browser.execute(() => {
      const stats = window.__momoAudioEvidence = {samples:0,clears:0,peak:0,lastRms:0,drains:0,rates:[],responses:[]};
      const originalFetch = window.fetch.bind(window);
      window.fetch = async (...args) => {
        const response = await originalFetch(...args);
        const input = args[0];
        const url = typeof input === 'string' ? input : input.url ?? String(input);
        if (url.includes('/api/tts/speech')) stats.responses.push({status:response.status,sampleRate:response.headers.get('x-sample-rate')});
        return response;
      };
      // Observe actual worklet input/output without replacing audio or inference.
      window.AudioWorkletNode = new Proxy(window.AudioWorkletNode,{construct(Target,args) {
        const node = Reflect.construct(Target,args);
        if (args[1] === 'pcm-player') {
          stats.rates.push(args[0].sampleRate);
          const post = node.port.postMessage.bind(node.port);
          node.port.postMessage = (message,...rest) => {
            if (message.type === 'push') stats.samples += message.samples.length;
            if (message.type === 'clear') stats.clears++;
            return post(message,...rest);
          };
          node.port.addEventListener('message',event => {
            if (event.data.type === 'rms') {
              stats.lastRms = event.data.rms ?? 0;
              stats.peak = Math.max(stats.peak,stats.lastRms);
            }
            if (event.data.type === 'drain') stats.drains++;
          });
          node.port.start();
        }
        return node;
      }});
    });
  });
  after(async () => {
    if (previous) {
      await api('/api/chat/interrupt','POST',{});
      await api('/api/state','PUT',previous);
    }
    if (provider) await new Promise(resolve => provider.close(resolve));
  });
  it('plays real PCM at its actual sample rate and recovers after interruption', async function () {
    this.timeout(300000);
    await $('.input-row input').setValue('语音联调');
    await browser.keys('Enter');
    await browser.waitUntil(async () => browser.execute(() => window.__momoAudioEvidence.peak > 0),{timeout:120000});
    await fs.mkdir('dist/native-e2e',{recursive:true});
    await browser.saveScreenshot('dist/native-e2e/real-tts-playing.png');
    await browser.waitUntil(async () => await $('button[aria-label="发送"]').isExisting(),{timeout:120000});
    const played = await browser.execute(() => window.__momoAudioEvidence);
    await fs.writeFile('dist/native-e2e/real-tts-evidence.json',JSON.stringify(played,null,2)+'\n');
    assert.ok(played.samples > 16000);
    assert.ok(played.responses.length > 0 && played.responses.every(response => response.status === 200 && response.sampleRate === '32000'),JSON.stringify(played.responses));
    assert.ok(played.rates.length > 0 && played.rates.every(rate => rate === 32000),JSON.stringify(played.rates));
    assert.ok(played.drains > 0);
    assert.ok(!(await $('.chat-error').isExisting()));
    const before = played.samples;
    await $('.input-row input').setValue('再说一次，测试打断');
    await browser.keys('Enter');
    await browser.waitUntil(async () => browser.execute(before => window.__momoAudioEvidence.samples > before,before),{timeout:120000});
    await $('button[aria-label="打断"]').click();
    await browser.waitUntil(async () => await $('button[aria-label="发送"]').isExisting());
    assert.ok((await browser.execute(() => window.__momoAudioEvidence)).clears > played.clears);
    await browser.waitUntil(async () => browser.execute(() => window.__momoAudioEvidence.lastRms === 0));
    const stopped = (await browser.execute(() => window.__momoAudioEvidence)).samples;
    await $('.input-row input').setValue('打断后继续');
    await browser.keys('Enter');
    await browser.waitUntil(async () => browser.execute(stopped => window.__momoAudioEvidence.samples > stopped,stopped),{timeout:120000});
    await browser.waitUntil(async () => await $('button[aria-label="发送"]').isExisting(),{timeout:120000});
    assert.ok((await browser.execute(() => window.__momoAudioEvidence)).samples > stopped);
    console.log('Real native TTS: PCM32k, nonzero worklet output, drain, interrupt and recovery passed');
  });
  it('recognizes actual synthesized speech using the packaged SenseVoice model', async function () {
    this.timeout(120000);
    const rt = await runtime();
    const response = await fetch(rt.baseUrl+'/api/tts/speech',{method:'POST',
      headers:{'x-momo-token':rt.token,'content-type':'application/json'},
      body:JSON.stringify({input:'你好，我们一起看星星。',lang:'zh'})});
    assert.equal(response.status,200);
    assert.equal(response.headers.get('x-sample-rate'),'32000');
    const audio = await response.arrayBuffer();
    // Feed controlled speech, never ambient microphone audio. ASR accepts 16kHz
    // PCM; decimate the known 32kHz fixture and append 500ms silence for VAD.
    const count = Math.floor(audio.byteLength/4);
    const source = new DataView(audio), pcm = new Uint8Array(count*2+16000);
    const target = new DataView(pcm.buffer);
    for(let index=0;index<count;index++)target.setInt16(index*2,source.getInt16(index*4,true),true);
    const text = await new Promise((resolve,reject) => {
      const socket = new WebSocket(rt.baseUrl.replace('http:','ws:')+'/api/asr/stream');
      const results = [];
      const timer = setTimeout(()=>{socket.close();reject(new Error('Packaged speech recognition timed out'));},90000);
      socket.onopen = ()=>socket.send(JSON.stringify({type:'authenticate',token:rt.token}));
      socket.onerror = ()=>{clearTimeout(timer);reject(new Error('Packaged ASR WebSocket failed'));};
      socket.onclose = ()=>{clearTimeout(timer);reject(new Error('Packaged ASR closed before finalizing'));};
      socket.onmessage = event=>{
        const message = JSON.parse(event.data);
        if(message.type==='ready'){socket.send(pcm);socket.send(JSON.stringify({type:'finalize'}));}
        if(message.type==='transcript')results.push(message.text);
        if(message.type==='error'){clearTimeout(timer);socket.close();reject(new Error('Packaged ASR returned an error'));}
        if(message.type==='finalized'){clearTimeout(timer);socket.close();resolve(results.join(''));}
      };
    });
    assert.match(text,/你好|星星/);
    await fs.writeFile('dist/native-e2e/real-asr-evidence.json',JSON.stringify({source:'synthetic TTS',inputRate:32000,asrRate:16000,recognized:text},null,2)+'\n');
    console.log('Packaged ASR recognized controlled synthesized Chinese speech');
  });
});
