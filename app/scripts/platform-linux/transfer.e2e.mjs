import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import JSZip from '../../frontend/node_modules/jszip/lib/index.js';

describe('installed native backup download', () => {
  it('saves a usable archive through the actual export button', async () => {
    const downloads = process.env.MOMO_NATIVE_DOWNLOAD_DIR ?? path.join(os.homedir(), 'Downloads');
    await fs.mkdir(downloads, {recursive:true});
    const sentinel = path.join(downloads, 'momo-backup.zip');
    await fs.writeFile(sentinel, 'existing backup must survive');
    const previous = new Set(await fs.readdir(downloads));
    await browser.waitUntil(async () => browser.execute(async () => {
      try {
        const runtime = await window.__TAURI__.core.invoke('get_runtime');
        return (await fetch(runtime.baseUrl + '/api/health', {headers:{'x-momo-token':runtime.token}})).ok;
      } catch { return false; }
    }), {timeout:60000});
    await browser.execute(() => {
      sessionStorage.removeItem('transferRequests');
      const original = window.fetch.bind(window);
      window.fetch = async (...args) => {
        const response = await original(...args);
        const records = JSON.parse(sessionStorage.getItem('transferRequests') || '[]');
        records.push({path:new URL(String(args[0]),location.href).pathname,status:response.status});
        sessionStorage.setItem('transferRequests',JSON.stringify(records.slice(-40)));
        return response;
      };
    });
    const generation = await browser.execute(async () => {
      const runtime = await window.__TAURI__.core.invoke('get_runtime');
      const response = await fetch(runtime.baseUrl + '/api/chat/interrupt', {method:'POST',
        headers:{'x-momo-token':runtime.token,'content-type':'application/json'},
        body:JSON.stringify({user:'synthetic header probe',assistant:''})});
      return response.headers.get('x-momo-generation');
    });
    assert.match(generation ?? '', /^\d+$/, 'Native cross-origin generation header is not exposed');
    await browser.execute(() => { location.hash = '/settings'; });
    await $('.settings-page').waitForDisplayed();
    for (const button of await $$('.tabstrip button')) {
      if ((await button.getText()).trim() === '通用') { await button.click(); break; }
    }
    let exportButton;
    for (const button of await $$('.settings-page button')) {
      if ((await button.getText()).trim() === '导出备份') { exportButton = button; break; }
    }
    assert.ok(exportButton, 'Native export control is missing');
    await exportButton.click();
    let saved;
    try {
      await browser.waitUntil(async () => {
        for (const name of await fs.readdir(downloads)) {
          if (previous.has(name) || !/^momo-backup.*\.zip$/.test(name)) continue;
          const candidate = path.join(downloads, name);
          if ((await fs.stat(candidate)).size > 100) { saved = candidate; return true; }
        }
        return false;
      }, {timeout:30000, timeoutMsg:'The native WebView did not save its backup download'});
    } catch (error) {
      await fs.mkdir('/workspace/evidence/native-backup', {recursive:true});
      await browser.saveScreenshot('/workspace/evidence/native-backup/download-failure.png');
      await fs.writeFile('/workspace/evidence/native-backup/download-failure.json', JSON.stringify(await browser.execute(() => ({text:document.body.innerText,origin:location.origin})),null,2));
      throw error;
    }
    assert.equal(await fs.readFile(sentinel, 'utf8'), 'existing backup must survive');
    assert.notEqual(saved, sentinel);
    const zip = await JSZip.loadAsync(await fs.readFile(saved));
    const manifest = JSON.parse(await zip.file('manifest.json').async('string'));
    assert.equal(manifest.format, 'momo-data');
    assert.equal(manifest.version, 1);
    assert.ok(zip.file('momo.db'));
    assert.equal((await zip.file('momo.db').async('nodebuffer')).subarray(0,16).toString(), 'SQLite format 3\0');
    await fs.mkdir('/workspace/evidence/native-backup', {recursive:true});
    await fs.writeFile('/workspace/evidence/native-backup/result.json', JSON.stringify({downloaded:true,size:(await fs.stat(saved)).size,entries:Object.keys(manifest.files)},null,2));
    // Exercise the actual React file-change/confirmation/import path in this native WebView.
    // The test driver supplies a standard File fixture; the normal OS picker is reviewed separately.
    console.log('Transfer phase: download verified; adding post-export memory');
    const marker = await browser.execute(async () => {
      const runtime = await window.__TAURI__.core.invoke('get_runtime');
      const response = await fetch(runtime.baseUrl + '/api/memories', {method:'POST',
        headers:{'x-momo-token':runtime.token,'content-type':'application/json'},
        body:JSON.stringify({content:'native-backup mutation after export'})});
      return (await response.json()).id;
    });
    assert.ok(marker);
    console.log('Transfer phase: memory created; scheduling file selection');
    const encoded = (await fs.readFile(saved)).toString('base64');
    const beforeReload = await browser.execute(() => performance.timeOrigin);
    await browser.execute(encoded => {
      const bytes = Uint8Array.from(atob(encoded), character => character.charCodeAt(0));
      const files = new DataTransfer();
      files.items.add(new File([bytes], 'momo-backup.zip', {type:'application/zip'}));
      const input = document.querySelector('.settings-page input[type="file"][accept=".zip"]');
      input.files = files.files;
      setTimeout(() => input.dispatchEvent(new Event('change', {bubbles:true})), 1000);
    }, encoded);
    console.log('Transfer phase: file scheduled; awaiting native confirmation');
    await browser.waitUntil(async () => {
      try { return (await browser.getAlertText()).includes('恢复备份'); } catch { return false; }
    }, {timeout:10000});
    console.log('Transfer phase: accepting native confirmation');
    await browser.acceptAlert();
    // A promise evaluated in the old document can be orphaned by location.reload
    // in the embedded driver. Observe the new document synchronously first.
    await browser.waitUntil(async () => browser.execute(previous =>
      performance.timeOrigin !== previous && document.readyState === 'complete', beforeReload),
      {timeout:30000,timeoutMsg:'Native restore did not reload its document'});
    try { await browser.waitUntil(async () => browser.execute(async id => {
      try {
        const runtime = await window.__TAURI__.core.invoke('get_runtime');
        const response = await fetch(runtime.baseUrl + '/api/memories', {headers:{'x-momo-token':runtime.token}});
        const rows = await response.json();
        sessionStorage.setItem('transferPoll',JSON.stringify({status:response.status,ids:rows.map(row=>row.id),marker:id}));
        return !rows.some(row => row.id === id);
      } catch (error) { sessionStorage.setItem('transferPoll',String(error)); return false; }
    }, marker), {timeout:30000, timeoutMsg:'Native restore did not replace the database'});
    } catch (error) {
      await browser.saveScreenshot('/workspace/evidence/native-backup/restore-failure.png');
      await fs.writeFile('/workspace/evidence/native-backup/restore-failure.json', JSON.stringify(await browser.execute(() => ({text:document.body.innerText,html:document.querySelector('[role=alert]')?.outerHTML,requests:sessionStorage.getItem('transferRequests'),poll:sessionStorage.getItem('transferPoll')})),null,2));
      throw error;
    }
    await fs.writeFile('/workspace/evidence/native-backup/restore-result.json', JSON.stringify({restored:true,confirmationAccepted:true,fixture:'DOM File',downloadDirectory:downloads},null,2));
    // Keep the archive in this disposable container only; it may contain local service keys.
  });
});
