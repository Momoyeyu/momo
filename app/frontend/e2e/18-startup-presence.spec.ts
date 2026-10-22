import { test, expect, type Page } from '@playwright/test';

async function fixture(page: Page, waitForState: Promise<void> = Promise.resolve(), language = 'zh') {
  await page.route('**/api/**', async route => {
    const url = new URL(route.request().url());
    let body: unknown = {};
    if (url.pathname === '/api/state') {
      if (route.request().method() === 'GET') await waitForState;
      body = {settings:{general:{lang:language,voiceInput:false},llm:{url:'',model:'',key:''},tts:{url:'',key:'',voice:'',prompt:''}}};
    }
    if (url.pathname === '/api/checkpoints/live') body = {summary:'',messages:[]};
    if (url.pathname === '/api/onboarding') body = {stage:'completed',busy:false,generation:0,configured:false};
    if (url.pathname === '/api/checkpoints' || url.pathname === '/api/memories') body = [];
    await route.fulfill({json:body});
  });
}

test('static presence precedes React; real startup stages animate then release the game', async ({page,request}, info) => {
  const html=await (await request.get('/')).text();
  expect(html).toContain('id="startup"');
  expect(html).toContain('href="/startup.css"');
  let restore!:()=>void;
  const restoring=new Promise<void>(resolve=>{restore=resolve;});
  await fixture(page,restoring);
  await page.addInitScript(()=>{
    const target=window as unknown as {__startupReady:boolean;__TAURI_INTERNALS__:unknown};
    target.__startupReady=false;
    target.__TAURI_INTERNALS__={invoke:async()=>{
      if(!target.__startupReady)throw 'Backend is starting';
      return {baseUrl:location.origin,token:'synthetic-startup-test'};
    }};
  });
  await page.goto('/');
  const screen=page.locator('#startup');
  await expect(screen).toHaveAttribute('data-stage','connecting');
  await expect(page.locator('#root')).toHaveAttribute('inert','');
  await expect.poll(()=>page.locator('.startup-art img').evaluate((image)=>image instanceof HTMLImageElement && image.complete && image.naturalWidth>0)).toBe(true);
  expect(await screen.evaluate(el=>getComputedStyle(el).backgroundColor)).toBe('rgb(24, 21, 31)');
  const movement=await page.locator('.startup-track-light').evaluate(el=>getComputedStyle(el).transform);
  await page.waitForTimeout(250); // Observe actual CSS motion while the backend remains pending.
  expect(await page.locator('.startup-track-light').evaluate(el=>getComputedStyle(el).transform)).not.toBe(movement);
  await page.screenshot({path:info.outputPath('startup-connecting.png')});
  await page.evaluate(()=>{(window as unknown as {__startupReady:boolean}).__startupReady=true;});
  await expect(screen).toHaveAttribute('data-stage','restoring');
  await expect(page.locator('.startup-message')).toHaveText('正在找回相处的片刻');
  restore();
  await expect(page.locator('.title-screen')).toBeVisible();
  await expect(screen).toHaveCount(0);
  await expect(page.locator('#root')).not.toHaveAttribute('inert');
  await expect(page.locator('#root')).not.toHaveAttribute('aria-hidden');
  await page.locator('.menu-item').filter({hasText:'设置'}).click();
  await expect(page.locator('.settings-page')).toBeVisible();
});

test('failed native startup stops the scene, hides private errors, and retries successfully', async ({page},info)=>{
  await fixture(page);
  await page.addInitScript(()=>{
    (window as unknown as {__TAURI_INTERNALS__:unknown}).__TAURI_INTERNALS__={invoke:async()=>{
      if(!sessionStorage.getItem('startup-failed-once')){
        sessionStorage.setItem('startup-failed-once','yes');
        throw 'private runtime failure: token=must-not-appear';
      }
      return {baseUrl:location.origin,token:'synthetic-startup-test'};
    }};
  });
  await page.goto('/');
  await expect(page.locator('#startup')).toHaveAttribute('data-stage','error');
  await expect(page.getByRole('alert')).toHaveText('这次见面还没准备好');
  await expect(page.locator('body')).not.toContainText('must-not-appear');
  expect(await page.locator('.startup-art img').evaluate(el=>getComputedStyle(el).animationPlayState)).toBe('paused');
  await page.screenshot({path:info.outputPath('startup-error.png')});
  await page.getByRole('button',{name:'再试一次'}).click();
  await expect(page.locator('.title-screen')).toBeVisible();
  await expect(page.locator('#startup')).toHaveCount(0);
});

test('reduced motion and saved language apply without waiting for optional services', async ({page})=>{
  await page.emulateMedia({reducedMotion:'reduce'});
  let finish!:()=>void;
  const hold=new Promise<void>(resolve=>{finish=resolve;});
  await fixture(page,hold,'ja');
  await page.addInitScript(()=>localStorage.setItem('momo-web.v1',JSON.stringify({settings:{general:{lang:'ja',voiceInput:false}}})));
  await page.goto('/');
  await expect(page.locator('#startup')).toHaveAttribute('data-stage','restoring');
  await expect(page.locator('.startup-message')).toHaveText('一緒に過ごした時間を読み込んでいます');
  expect(await page.locator('.startup-art img').evaluate(el=>getComputedStyle(el).animationName)).toBe('none');
  expect(await page.locator('.startup-track-light').evaluate(el=>getComputedStyle(el).animationName)).toBe('none');
  finish();
  await expect(page.locator('.title-screen')).toBeVisible();
  await expect(page.locator('#startup')).toHaveCount(0);
  await expect(page.locator('html')).toHaveAttribute('lang','ja');
});
