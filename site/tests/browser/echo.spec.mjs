import { test, expect } from '@playwright/test'
import { mkdirSync } from 'node:fs'
const evidence=process.env.MOMO_SITE_EVIDENCE||'.playwright-evidence'
mkdirSync(evidence,{recursive:true})
async function seek(page,p) {
  await page.evaluate(p=>{const s=document.querySelector('#voice');const stage=s.querySelector('.chapter-stage');scrollTo({top:scrollY+s.getBoundingClientRect().top+(s.offsetHeight-stage.offsetHeight)*p,behavior:'instant'})},p)
  await expect.poll(async()=>Math.abs(Number(await page.locator('.echo-canvas').getAttribute('data-frame'))-p)).toBeLessThan(.004)
}
for (const [width,height] of [[1938,1280],[1440,1000],[667,807]]) test(`desktop: echo exchange reverses and holds at ${width}x${height}`,async({page})=>{
  await page.setViewportSize({width,height});await page.goto('./')
  await expect(page.locator('.product-chapters')).toHaveClass(/echo-enhanced/)
  for (const p of [.22,.40,.60,.85]) {await seek(page,p);await page.screenshot({path:`${evidence}/echo-${width}-${p}.png`})}
  const finished=await page.locator('.echo-canvas').evaluate(c=>c.toDataURL())
  await seek(page,.6);const middle=await page.locator('.echo-canvas').evaluate(c=>c.toDataURL())
  await seek(page,.85);expect(await page.locator('.echo-canvas').evaluate(c=>c.toDataURL())).toEqual(finished)
  await seek(page,.6);expect(await page.locator('.echo-canvas').evaluate(c=>c.toDataURL())).toEqual(middle)
  expect(await page.locator('html').evaluate(el=>el.scrollWidth<=innerWidth)).toBe(true)
  await expect(page.locator('#voice h2')).toBeInViewport()
  await expect(page.locator('#voice .chapter-footnote')).toBeVisible()
  await page.evaluate(()=>{window.echoPaints=0;const c=document.querySelector('.echo-canvas').getContext('2d');const old=c.fillText.bind(c);c.fillText=(...args)=>{window.echoPaints++;return old(...args)}})
  await page.waitForTimeout(300);expect(await page.evaluate(()=>window.echoPaints)).toBe(0)
})
for (const mode of ['reduce','save-data','no-canvas','short']) test(`desktop: echo static fallback for ${mode}`,async({page})=>{
  await page.setViewportSize({width:1440,height:mode==='short'?540:1000})
  if(mode==='reduce')await page.emulateMedia({reducedMotion:'reduce'})
  if(mode==='save-data')await page.addInitScript(()=>Object.defineProperty(navigator,'connection',{value:{saveData:true,addEventListener(){},removeEventListener(){}}}))
  if(mode==='no-canvas')await page.addInitScript(()=>{const original=HTMLCanvasElement.prototype.getContext;HTMLCanvasElement.prototype.getContext=function(...args){return this.classList.contains('echo-canvas')?null:original.apply(this,args)}})
  await page.goto('./#voice');await expect(page.locator('.product-chapters')).not.toHaveClass(/echo-enhanced/)
  await expect(page.locator('.echo-question')).toBeVisible()
  await expect(page.locator('#voice .chapter-footnote')).toBeVisible()
  expect(await page.locator('#voice .chapter-stage').evaluate(el=>getComputedStyle(el).position)).toBe('relative')
})
