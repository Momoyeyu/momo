import assert from 'node:assert/strict';
import fs from 'node:fs/promises';

describe('native approved silhouette and chat entry',()=>{
  it('reveals actual chat after ASCII and leaves other routes on RoPE',async()=>{
    await browser.execute(()=>{location.hash='/title';});
    await $('.title-presence').waitForDisplayed();
    assert.ok((await $('.title-presence').getAttribute('src')).endsWith('/brand/momo-presence-v2.webp'));
    await browser.waitUntil(async()=>browser.execute(()=>document.querySelector('.title-presence')?.naturalWidth===1024));
    await browser.waitUntil(async()=>browser.execute(()=>Number(getComputedStyle(document.querySelector('.title-menu button:last-child')).opacity)>.99));
    const titleLayout=await browser.execute(()=>{
      const rect=selector=>document.querySelector(selector).getBoundingClientRect();
      const brand=rect('.title-brand'),menu=rect('.title-menu'),art=rect('.title-presence'),stage=rect('.app-stage');
      return {below:menu.top>brand.bottom+16,separate:menu.right<art.left,
        left:menu.right<stage.left+stage.width/2,inside:menu.bottom<stage.bottom-16};
    });
    for(const value of Object.values(titleLayout))assert.equal(value,true);
    await fs.mkdir('dist/native-e2e',{recursive:true});
    await browser.saveScreenshot('dist/native-e2e/title-approved-silhouette.png');
    await browser.execute(()=>{location.hash='/chat';});
    await $('.chat-entry-canvas').waitForExist();
    await browser.waitUntil(async()=>browser.execute(()=>Number(document.querySelector('.chat-entry-canvas')?.dataset.cells)>1000));
    await browser.saveScreenshot('dist/native-e2e/ascii-entry.png');
    await $('.chat-entry-canvas').waitForExist({reverse:true,timeout:5000});
    await browser.waitUntil(async()=>browser.execute(()=>{
      const dialog=document.querySelector('.dialog-zone');
      return dialog && Number(getComputedStyle(dialog).opacity)>.95 && !document.querySelector('.page-wrap.is-booting');
    }));
    await browser.execute(()=>{location.hash='/settings';});
    await $('.page-wrap.is-front').waitForExist();
    await $('.page-wrap.is-front').waitForExist({reverse:true});
    assert.equal(await $('.chat-entry-canvas').isExisting(),false);
    const layout=await browser.execute(()=>{
      const panel=document.querySelector('.data-transfer-panel').getBoundingClientRect();
      const columns=document.querySelector('.settings-cols').getBoundingClientRect();
      const buttons=[...document.querySelectorAll('.data-transfer-actions button')].map(element=>element.getBoundingClientRect());
      return {below:panel.top>=columns.bottom,aligned:Math.abs(buttons[0].top-buttons[1].top)<1,gap:buttons[1].left-buttons[0].right};
    });
    assert.equal(layout.below,true);assert.equal(layout.aligned,true);assert.ok(layout.gap>=8);
    await browser.saveScreenshot('dist/native-e2e/balanced-settings-backup.png');
    // Returning from utility pages must never replay the title's ASCII plate.
    for(const route of ['settings','checkpoint','backlog']) {
      await browser.execute(target=>{location.hash=`/${target}`;},route);
      await $(`.${route}-page`).waitForExist();
      await $('.page-wrap.is-front').waitForExist({reverse:true});
      await browser.execute(()=>{
        document.body.dataset.returnAsciiEntries='0';
        new MutationObserver(records=>{
          for(const record of records)for(const node of record.addedNodes){
            if(node instanceof Element&&(node.matches('.chat-entry-canvas')||node.querySelector('.chat-entry-canvas')))
              document.body.dataset.returnAsciiEntries=String(Number(document.body.dataset.returnAsciiEntries)+1);
          }
        }).observe(document.querySelector('.app-stage'),{childList:true,subtree:true});
        location.hash='/chat';
      });
      await $('.page-wrap.is-chat.is-front').waitForExist();
      await $('.page-wrap.is-front').waitForExist({reverse:true});
      assert.equal(await $('body').getAttribute('data-return-ascii-entries'),'0');
      assert.equal(await $('.page-wrap.is-booting').isExisting(),false);
    }
  });
});
