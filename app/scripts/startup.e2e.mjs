import assert from 'node:assert/strict';
import fs from 'node:fs/promises';

describe('installed macOS startup presence', () => {
  afterEach(async function () {
    if (this.currentTest.state !== 'failed') return;
    const diagnostic = await browser.execute(() => ({route:location.hash,root:document.querySelector('#root')?.outerHTML.slice(0,300),
      body:document.body.innerText.slice(0,700),visibility:document.visibilityState,
      layers:[...document.querySelectorAll('.page-wrap,.dialog-zone,.title-screen,.title-menu')].map(element=>({name:element.className,
        opacity:getComputedStyle(element).opacity,display:getComputedStyle(element).display,rect:element.getBoundingClientRect().toJSON()})),
      animations:document.getAnimations().map(animation=>({name:animation.animationName,state:animation.playState,time:animation.currentTime}))}));
    console.log('Startup diagnostic',JSON.stringify(diagnostic));
    await fs.writeFile('dist/native-e2e/startup-diagnostic.json',JSON.stringify(diagnostic,null,2));
    await browser.saveScreenshot('dist/native-e2e/startup-installed-failure.png');
  });
  it('ships the early local scene and releases the actual game after startup and reload', async () => {
    const ready = async () => browser.waitUntil(async () => browser.execute(() =>
      !!document.querySelector('.app-stage') && !document.querySelector('#startup') &&
      !document.querySelector('#root')?.hasAttribute('inert') &&
      !document.querySelector('#root')?.hasAttribute('aria-hidden')
    ), {timeout:60000,timeoutMsg:'Startup scene did not release the installed game'});
    await ready();
    const result = await browser.execute(async () => {
      const [html,css,svg] = await Promise.all(['/', '/startup.css', '/brand/startup-presence.svg'].map(async path => {
        const response=await fetch(path);return {ok:response.ok,text:await response.text()};
      }));
      return {localScene:html.ok&&html.text.includes('id="startup"'),earlyStyle:html.text.includes('href="/startup.css"'),
        styleLoaded:css.ok&&css.text.includes('startup-paper-float'),
        artworkLoaded:svg.ok&&svg.text.includes('<svg'),reducedMotion:css.text.includes('prefers-reduced-motion'),
        removed:!document.querySelector('#startup'),interactive:!document.querySelector('#root').hasAttribute('inert')};
    });
    for(const [name,value] of Object.entries(result))assert.equal(value,true,name);
    // This spec may follow a settings/backup check in the same isolated profile.
    await browser.execute(() => {location.hash='/title';});
    await $('.title-screen').waitForDisplayed();
    const previous = await browser.execute(() => performance.timeOrigin);
    await browser.refresh();
    await browser.waitUntil(async () => browser.execute(previous => performance.timeOrigin !== previous && document.readyState === 'complete', previous));
    await ready();
    await browser.waitUntil(async () => {
      for (const target of await $$('.input-row input, .title-menu .menu-item')) if (await target.isDisplayed()) return true;
      return false;
    }, {timeout:30000,timeoutMsg:'Ready document did not show interactive game content'});
    await fs.mkdir('dist/native-e2e',{recursive:true});
    await fs.writeFile('dist/native-e2e/startup-installed-evidence.json',JSON.stringify({...result,reloadReady:true},null,2));
    await browser.saveScreenshot('dist/native-e2e/startup-installed-ready.png');
  });
});
