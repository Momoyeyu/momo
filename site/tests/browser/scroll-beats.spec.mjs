import {test,expect} from '@playwright/test'

async function seek(page, selector, raw, opening=false) {
  await page.evaluate(({selector,raw,opening})=>{
    const section=document.querySelector(selector),stage=section.querySelector(opening?'.meeting-scene':'.chapter-stage')
    scrollTo({top:scrollY+section.getBoundingClientRect().top+(section.offsetHeight-stage.offsetHeight)*raw,behavior:'instant'})
  },{selector,raw,opening})
  const frame=opening?page.locator('.meeting-scene'):page.locator(selector)
  await expect.poll(async()=>Math.abs(Number(await frame.getAttribute('data-progress'))-raw)).toBeLessThan(.001)
}

test('desktop: each scene rests before and after its animation in both scroll directions',async({page})=>{
  await page.setViewportSize({width:1440,height:1000})
  await page.goto('./')
  await expect(page.locator('.product-chapters')).toHaveClass(/chapters-enhanced/)
  const scenes=[['.meeting-runway','.meeting-scene',true],['#together','#together'],['#memory','#memory'],['#voice','#voice'],['#extensions','#extensions']]
  for(const [section,frame,opening=false] of scenes){
    for(const raw of [.02,.1,.92,.98,.92,.1,.02]){
      await seek(page,section,raw,opening)
      await expect(page.locator(frame)).toHaveAttribute('data-animation-progress',raw<.5?'0.0000':'1.0000')
      const stage=page.locator(opening?'.meeting-scene':`${section} .chapter-stage`)
      expect((await stage.boundingBox()).y).toBeCloseTo(0,0)
    }
    await seek(page,section,.5,opening)
    const middle=Number(await page.locator(frame).getAttribute('data-animation-progress'))
    expect(middle).toBeGreaterThan(0);expect(middle).toBeLessThan(1)
  }
  // Preserve the original opening duration, adding reading distance around it.
  expect(await page.locator('.meeting-runway').evaluate(el=>(el.offsetHeight-innerHeight)*.64)).toBeGreaterThanOrEqual(1250)
})

test('desktop: the echo is pixel-stable through its landing hold, including a compact review pane',async({page})=>{
  await page.setViewportSize({width:667,height:807});await page.goto('./')
  await expect(page.locator('.product-chapters')).toHaveClass(/echo-enhanced/)
  await seek(page,'#voice',.92)
  await expect(page.locator('.echo-canvas')).toHaveAttribute('data-frame','1.0000')
  const landed=await page.locator('.echo-canvas').evaluate(c=>c.toDataURL())
  for(const raw of [.98,.92]){
    await seek(page,'#voice',raw)
    expect(await page.locator('.echo-canvas').evaluate(c=>c.toDataURL())).toBe(landed)
  }
  expect((await page.locator('#voice .chapter-stage').boundingBox()).y).toBeCloseTo(0,0)
})
