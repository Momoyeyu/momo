import assert from 'node:assert/strict';
import fs from 'node:fs/promises';

describe('native dialogue entry sequencing',()=>{
  it('waits for ASCII or RoPE, then visibly rises before unlocking controls',async()=>{
    await browser.waitUntil(async()=>browser.execute(()=>!!document.querySelector('.app-stage')&&!document.querySelector('#startup')));
    await fs.mkdir('dist/native-e2e',{recursive:true});
    for(const origin of ['title','settings','checkpoint','backlog']){
      await browser.execute(route=>{location.hash=`/${route}`;},origin);
      await browser.waitUntil(async()=>browser.execute(()=>document.querySelectorAll('.page-wrap').length===1&&!document.querySelector('.page-wrap.is-front')));
      await browser.execute(()=>{
        const audit={frames:[],done:false};window.__dialogAudit=audit;
        const sample=()=>{
          if(window.__dialogAudit!==audit)return;
          const wrap=document.querySelector('.page-wrap.is-chat');
          const zone=wrap?.querySelector('.dialog-zone'),input=zone?.querySelector('input');
          if(zone&&input&&location.hash==='#/chat'){
            const frame={cover:wrap.classList.contains('is-front'),ink:!!document.querySelector('.chat-entry-canvas'),
              opacity:Number(getComputedStyle(zone).opacity),interactive:!input.closest('[inert]'),setup:!!document.querySelector('.first-meeting-setup')};
            audit.frames.push(frame);
            if(!frame.cover&&!frame.ink&&frame.opacity>.999&&frame.interactive){audit.done=true;return;}
          }
          requestAnimationFrame(sample);
        };
        requestAnimationFrame(sample);location.hash='/chat';
      });
      await browser.waitUntil(async()=>browser.execute(()=>window.__dialogAudit.done),{timeout:10000});
      const frames=await browser.execute(()=>window.__dialogAudit.frames);
      await fs.writeFile(`dist/native-e2e/dialog-from-${origin}.json`,JSON.stringify(frames,null,2));
      const covered=frames.filter(frame=>frame.cover||frame.ink);
      assert.ok(covered.length>0,`${origin}: capture the actual covering scene`);
      assert.ok(covered.every(frame=>frame.opacity<.001&&!frame.interactive&&!frame.setup),`${origin}: no premature dialogue or input`);
      const rising=frames.filter(frame=>!frame.cover&&!frame.ink&&frame.opacity>.02&&frame.opacity<.95);
      assert.ok(rising.length>0,`${origin}: visible rise after the scene`);
      assert.ok(rising.every(frame=>!frame.interactive),`${origin}: no input during the rise`);
      assert.equal(frames.at(-1).interactive,true);
    }
    await browser.saveScreenshot('dist/native-e2e/dialog-sequenced-ready.png');
  });
});
