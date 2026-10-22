import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import {createServer} from 'node:http';

// The provider is deterministic; requests still traverse native IPC, the packaged
// backend and real SQLite. No WebView network interception or source backend.
let provider;
let providerUrl;
async function api(path, method = 'GET', body) {
  return browser.execute(async (path, method, body) => {
    const runtime = await window.__TAURI__.core.invoke('get_runtime');
    const response = await fetch(runtime.baseUrl + path, {method,
      headers: {'x-momo-token': runtime.token, 'content-type': 'application/json'},
      ...(['GET', 'HEAD'].includes(method) || body == null ? {} : {body: JSON.stringify(body)})});
    return {status: response.status, text: await response.text()};
  }, path, method, body);
}
async function reloadReady() {
  const previous = await browser.execute(() => performance.timeOrigin);
  await browser.refresh();
  await browser.waitUntil(async () => browser.execute(previous =>
    performance.timeOrigin !== previous && document.readyState === 'complete' &&
    !!document.querySelector('.app-stage') && !document.querySelector('#startup') &&
    !document.querySelector('#root')?.hasAttribute('inert'), previous),
    {timeout:60000,timeoutMsg:'New document did not finish startup before navigation'});
}
async function route(name) {
  await browser.execute(name => { location.hash = '/' + name; }, name);
}
async function clickText(selector, label) {
  for (const element of await $$(selector)) {
    if ((await element.getText()).trim() === label) { await element.click(); return; }
  }
  throw new Error(`Missing visible control: ${label}`);
}
async function tab(label) { await clickText('.tabstrip button', label); }

describe('native player flows', () => {
  before(async () => {
    provider = createServer((request, response) => {
      let raw = '';
      request.on('data', data => raw += data);
      request.on('end', () => {
        if (request.url?.endsWith('/models')) {
          response.setHeader('content-type', 'application/json');
          response.end(JSON.stringify({data: [{id: 'native-fixture'}]})); return;
        }
        if (!request.url?.endsWith('/chat/completions')) { response.writeHead(404).end(); return; }
        const body = JSON.parse(raw);
        if (!body.stream) {
          const classify = body.tools?.some(tool => tool.function.name === 'first_meeting_result');
          const message = classify ? {role:'assistant', content:null, tool_calls:[{id:'name-1', type:'function',
            function:{name:'first_meeting_result', arguments:JSON.stringify({action:'name', name:'小星', quote:'小星'})}}]} : {role:'assistant', content:'[]'};
          response.setHeader('content-type','application/json');
          response.end(JSON.stringify({choices:[{message}]})); return;
        }
        response.setHeader('content-type', 'text/event-stream');
        const lastUser = [...body.messages].reverse().find(message => message.role === 'user');
        const echoTool = body.tools?.find(tool => tool.function.name.endsWith('_echo'));
        if (lastUser?.content === '拓展联调' && echoTool && body.messages.at(-1).role !== 'tool') {
          response.write(`data: ${JSON.stringify({choices:[{index:0,delta:{tool_calls:[{index:0,id:'native-echo',type:'function',function:{name:echoTool.function.name,arguments:'{"message":"native-test"}'}}]}}]})}\n\n`);
          response.end('data: [DONE]\n\n'); return;
        }
        const text = '<|ACT {"expression":{"name":"happy","intensity":0.7}}|>小星，很高兴认识你。今天想聊些什么？';
        for (const content of text.match(/.{1,5}/gs)) response.write(`data: ${JSON.stringify({choices:[{index:0,delta:{content}}]})}\n\n`);
        response.end('data: [DONE]\n\n');
      });
    });
    await new Promise(resolve => provider.listen(0, '127.0.0.1', resolve));
    providerUrl = `http://127.0.0.1:${provider.address().port}`;
  });
  after(async () => { await new Promise(resolve => provider.close(resolve)); });
  afterEach(async function () {
    if (this.currentTest.state !== 'failed') return;
    await fs.mkdir('dist/native-e2e', {recursive:true});
    await browser.saveScreenshot('dist/native-e2e/player-flow-failure.png');
    const diagnostic = await browser.execute(() => ({route: location.hash,
      subtitle:document.querySelector('.subtitle')?.textContent,
      error:document.querySelector('.chat-error')?.textContent,
      dialogs:[...document.querySelectorAll('[role="dialog"]')].map(element=>element.getAttribute('aria-label')),
      readyState:document.readyState,visibility:document.visibilityState,startup:document.querySelector('#startup')?.getAttribute('data-stage'),
      rootInert:document.querySelector('#root')?.hasAttribute('inert'),
      layers:[...document.querySelectorAll('.page-wrap,.dialog-zone,.input-row')].map(element=>({className:element.className,
        opacity:getComputedStyle(element).opacity,display:getComputedStyle(element).display,visibility:getComputedStyle(element).visibility,
        height:element.getBoundingClientRect().height})),
      animations:document.getAnimations().map(animation=>({name:animation.animationName,state:animation.playState,current:animation.currentTime})),
      core:!!window.Live2DCubismCore,canvases:document.querySelectorAll('canvas').length}));
    console.log('Native player diagnostic', JSON.stringify(diagnostic));
  });

  it('opens the first meeting, saves a name, streams clean dialogue and resumes it', async () => {
    await browser.waitUntil(async () => { try { return (await api('/api/health')).status === 200; } catch { return false; } });
    const existing = JSON.parse((await api('/api/state')).text);
    const state = {...existing, settings:{...existing.settings,
      llm:{url:providerUrl, model:'native-fixture', key:'', thinking:false},
      general:{...existing.settings?.general,lang:'zh',voiceInput:false,textSpeed:100,sentenceSpeed:100}}};
    assert.equal((await api('/api/state', 'PUT', state)).status,204);
    await reloadReady();
    await route('chat');
    await $('.input-row input').waitForDisplayed();
    // An existing E2E profile can be reused; a clean profile additionally checks
    // the scripted opening. Every run still checks the real provider roundtrip.
    const stage = JSON.parse((await api('/api/onboarding')).text).stage;
    if (stage === 'new' || stage === 'asking') {
      await browser.waitUntil(async () => (await $('.subtitle').getText()).includes('怎么称呼'), {timeout:30000});
    }
    await $('.input-row input').setValue('你好，叫我小星就好。');
    await browser.keys('Enter');
    await browser.waitUntil(async () => (await $('.subtitle').getText()).includes('今天想聊些什么'), {timeout:60000});
    const live = JSON.parse((await api('/api/checkpoints/live')).text);
    assert.ok(live.messages.some(message => message.role === 'user' && message.text.includes('小星')));
    assert.ok(live.messages.some(message => message.role === 'assistant' && message.text.includes('很高兴认识你')));
    assert.ok(!JSON.stringify(live).includes('<|ACT'));
    assert.equal(JSON.parse((await api('/api/onboarding')).text).stage, 'completed');
    const memories = JSON.parse((await api('/api/memories')).text);
    assert.ok(memories.some(memory => memory.content.includes('小星')));
    await reloadReady();
    await $('.input-row input').waitForDisplayed();
    assert.equal(JSON.parse((await api('/api/checkpoints/live')).text).messages.length, live.messages.length);
  });

  it('edits memory, opens resource preview, and saves/loads a checkpoint in native UI', async () => {
    await route('settings');
    await $('.settings-page').waitForDisplayed();
    await tab('记忆');
    await $('.mem-panel .list-toolbar .icon-btn').click();
    await $('.add-field').setValue('native-e2e 喜欢在夜晚看星星');
    await clickText('.modal-foot button', '保存');
    await browser.waitUntil(async () => (await $('.mem-panel').getText()).includes('native-e2e'));
    await tab('拓展');
    await $('.avatar-row').click();
    await $('.resource-preview canvas').waitForDisplayed({timeout:30000});
    assert.equal((await $$('.avatar-res-row')).length, 4);
    await $('.resource-play').click();
    await browser.waitUntil(async () => (await $('[data-testid="performance-face"]').getText()) === 'happy');
    assert.ok(await $('[role="progressbar"]').isExisting());
    await fs.mkdir('dist/native-e2e', {recursive:true});
    await browser.saveScreenshot('dist/native-e2e/live2d-preview.png');
    await $('.avatar-res-modal .modal-foot button').click();
    await route('chat');
    await $('button[aria-label="存档"]').click();
    await $('.slot-card.slot-empty').click();
    await $('.slot-card.slot-filled').waitForDisplayed();
    const checkpoints = JSON.parse((await api('/api/checkpoints')).text);
    const manual = checkpoints.find(checkpoint => !checkpoint.auto);
    assert.ok(manual?.id);
    assert.equal((await api('/api/checkpoints/live/new', 'POST')).status,204);
    assert.equal(JSON.parse((await api('/api/checkpoints/live')).text).messages.length,0);
    assert.equal((await api(`/api/checkpoints/${manual.id}/load`, 'POST')).status,200);
    assert.ok(JSON.parse((await api('/api/checkpoints/live')).text).messages.length > 0);
  });

  it('connects a real stdio MCP extension, calls its tool, and reaps its process', async () => {
    const folder = await fs.mkdtemp(path.join(os.tmpdir(), 'momo native MCP '));
    const script = path.join(folder, 'server.mjs');
    const pidFile = path.join(folder, 'server.pid');
    const marker = path.join(folder, 'called.txt');
    await fs.writeFile(script, `import fs from 'node:fs';
import readline from 'node:readline';
fs.writeFileSync(process.env.MOMO_E2E_PID_FILE, String(process.pid));
const input = readline.createInterface({input:process.stdin});
input.on('line', line => {
 const request=JSON.parse(line); if(request.id===undefined)return;
 let result={};
 if(request.method==='initialize') result={protocolVersion:request.params.protocolVersion,capabilities:{tools:{}},serverInfo:{name:'native-fixture',version:'1'}};
 else if(request.method==='tools/list') result={tools:[{name:'echo',description:'native fixture echo',inputSchema:{type:'object',properties:{message:{type:'string'}},required:['message']}}]};
 else if(request.method==='tools/call'){fs.writeFileSync(process.env.MOMO_E2E_CALL_FILE,request.params.arguments.message);result={content:[{type:'text',text:'native-test returned'}]};}
 process.stdout.write(JSON.stringify({jsonrpc:'2.0',id:request.id,result})+'\\n');
});
input.on('close',()=>process.exit(0));
`);
    const extension = {name:'native',enabled:true,transport:'stdio',command:process.execPath,args:[script],
      env:{MOMO_E2E_PID_FILE:pidFile,MOMO_E2E_CALL_FILE:marker}};
    const previous = JSON.parse((await api('/api/state')).text);
    try {
      const probe = JSON.parse((await api('/api/extensions/probe','POST',extension)).text);
      assert.equal(probe.ok,true,JSON.stringify(probe));
      assert.equal(probe.tools,1);
      const firstPid=Number(await fs.readFile(pidFile,'utf8'));
      await browser.waitUntil(async()=>{try{process.kill(firstPid,0);return false;}catch(error){return error.code==='ESRCH';}});
      await api('/api/state','PUT',{...previous,settings:{...previous.settings,extensions:[extension]}});
      await route('chat');
      await $('.input-row input').setValue('拓展联调');
      await browser.keys('Enter');
      await browser.waitUntil(async()=>{try{return await fs.readFile(marker,'utf8')==='native-test';}catch{return false;}},{timeout:30000});
      await browser.waitUntil(async()=>{const pid=Number(await fs.readFile(pidFile,'utf8'));try{process.kill(pid,0);return false;}catch(error){return error.code==='ESRCH';}},{timeout:30000});
    } finally {
      await api('/api/chat/interrupt','POST',{});
      await api('/api/state','PUT',previous);
      await fs.rm(folder,{recursive:true,force:true});
    }
  });

});
