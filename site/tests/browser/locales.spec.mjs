import { test, expect } from '@playwright/test'
import { mkdirSync } from 'node:fs'
import { copy, localePath } from '../../docs/.vitepress/theme/product-copy.mjs'
const evidence=process.env.MOMO_SITE_EVIDENCE||'.playwright-evidence'
mkdirSync(evidence,{recursive:true})
for (const code of ['zh','en','ja']) {
  test(`desktop: ${code} SSR language, full product content, menu and local guide`,async({page,request})=>{
    const path=`/momo${localePath(code)}`
    const t=copy[code]
    const response=await request.get(path)
    const html=await response.text()
    expect(response.status()).toBe(200)
    expect(html).toContain(`<html lang="${t.lang}"`)
    expect(html).toContain(t.hero[0])
    expect(html).toContain(t.horizon.nowItems[0])
    expect(html).toContain('hreflang="en"')
    expect(html).toContain('hreflang="ja"')
    expect(html).toContain(`rel="canonical" href="https://momoyeyu.github.io${path}"`)
    await page.setViewportSize({width:1440,height:1000})
    const errors=[];page.on('pageerror',e=>errors.push(e.message))
    await page.goto(path)
    await page.evaluate(()=>document.fonts.ready)
    await expect(page.locator('h1')).toHaveText(t.hero.join(''))
    await expect(page.locator('html')).toHaveAttribute('lang',t.lang)
    const menu=page.getByTestId('language-menu')
    await menu.locator('summary').click()
    await expect(menu).toHaveAttribute('open','')
    await page.keyboard.press('Escape')
    await expect(menu).not.toHaveAttribute('open','')
    await expect(menu.locator('summary')).toBeFocused()
    await menu.locator('summary').click()
    await page.locator('h1').click()
    await expect(menu).not.toHaveAttribute('open','')
    const github=page.getByRole('link',{name:t.github})
    await expect(github.locator('svg')).toHaveCount(1)
    await expect(github).toHaveAttribute('href','https://github.com/Momoyeyu/momo')
    expect(await page.locator('html').evaluate(el=>el.scrollWidth<=innerWidth)).toBe(true)
    await page.getByTestId('deck-next').click()
    await expect(page.getByTestId('deck-status')).toHaveText(t.themes[1])
    expect(await page.locator('[data-position="active"]').evaluate(el=>el.scrollHeight<=el.clientHeight+2)).toBe(true)
    await page.getByTestId('deck-next').click()
    expect(await page.locator('[data-position="active"]').evaluate(el=>el.scrollHeight<=el.clientHeight+2)).toBe(true)
    await page.getByTestId('deck-next').click()
    await page.getByTestId('video-toggle').click()
    await page.screenshot({path:`${evidence}/desktop-${code}.png`,fullPage:true,animations:'disabled'})
    await page.locator('#faq summary').first().click()
    await expect(page.locator('#faq details').first()).toHaveAttribute('open','')
    const links=[...new Set(await page.locator('a[href]').evaluateAll(nodes=>nodes.map(n=>n.href)))]
    for(const href of links){
      const url=new URL(href)
      if(url.origin!==new URL(page.url()).origin)continue
      expect(url.pathname.startsWith('/momo/')).toBe(true)
      const result=await request.get(url.href)
      expect(result.status(),url.href).toBe(200)
      if(url.hash)expect(await result.text()).toContain(`id="${decodeURIComponent(url.hash.slice(1))}"`)
    }
    await page.goto(path)
    // A native details element works before hydration; prove Vue handled a card first.
    await page.getByTestId('deck-next').click()
    await expect(page.getByTestId('deck-status')).toHaveText(t.themes[1])
    await page.locator('.chapter-index a[href="#memory"]').click()
    await expect(page).toHaveURL(new RegExp(`${path}#memory$`))
    await menu.locator('summary').click()
    const next=code==='zh'?'en':code==='en'?'ja':'zh'
    await expect(page.getByTestId(`locale-${next}`)).toHaveAttribute('href',`/momo${localePath(next)}#memory`)
    await page.getByTestId(`locale-${next}`).click()
    await expect(page).toHaveURL(new RegExp(`/momo${localePath(next)}#memory$`))
    await expect(page.locator('html')).toHaveAttribute('lang',copy[next].lang)
    await page.goto(path)
    await page.getByRole('link',{name:t.guide,exact:true}).click()
    await expect(page.locator('.help-page article h1')).toBeVisible()
    if(code!=='zh') {
      await expect(page.locator('.translation-note')).toHaveText(t.helpNote)
      const voiceLink=page.locator('.help-page article a').filter({hasText:'TTS'})
      await expect(voiceLink).toHaveAttribute('href',new URL(`/momo/help/voice?lang=${code}`,page.url()).href)
      await voiceLink.click()
      await expect(page).toHaveURL(new URL(`/momo/help/voice?lang=${code}`,page.url()).href)
      await page.reload()
      await expect(page.locator('.translation-note')).toHaveText(t.helpNote)
      await page.goBack()
      await expect(page.locator('.translation-note')).toHaveText(t.helpNote)
    }
    await page.getByRole('link',{name:t.back,exact:true}).click()
    await expect(page.locator('h1')).toHaveText(t.hero.join(''))
    expect(errors).toEqual([])
  })
}
