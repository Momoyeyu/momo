import { test, expect } from '@playwright/test'
import { mkdirSync } from 'node:fs'

const evidence = process.env.MOMO_SITE_EVIDENCE || '.playwright-evidence'
mkdirSync(evidence, { recursive: true })
async function atChapter(page, id, p) {
  const target = await page.evaluate(({ id, p }) => {
    const chapter = document.getElementById(id), stage = chapter.querySelector('.chapter-stage')
    scrollTo({ top: scrollY + chapter.getBoundingClientRect().top + Math.max(0, chapter.offsetHeight - stage.offsetHeight) * p, behavior: 'instant' })
    return (-chapter.getBoundingClientRect().top / Math.max(1, chapter.offsetHeight - stage.offsetHeight)).toFixed(4)
  }, { id, p })
  await expect.poll(() => page.locator(`#${id}`).getAttribute('data-progress')).toBe(target)
}

test('desktop: content-specific chapters reverse, settle, and keep text above moving fragments', async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 1000 })
  await page.goto('./')
  await expect(page.locator('.product-chapters')).toHaveClass(/chapters-enhanced/)
  for (const [id, effect] of [['together','.signature-echo.echo-a'], ['memory','.archive-fragments'], ['voice','.echo-canvas']]) {
    await atChapter(page, id, .2)
    const early = await page.locator(`#${id} ${effect}`).evaluate(el => el.outerHTML + getComputedStyle(el).transform + getComputedStyle(el).opacity)
    await atChapter(page, id, .65)
    const later = await page.locator(`#${id} ${effect}`).evaluate(el => el.outerHTML + getComputedStyle(el).transform + getComputedStyle(el).opacity)
    expect(later).not.toEqual(early)
    await expect(page.locator(`#${id} h2`)).toBeInViewport()
    await page.screenshot({ path: `${evidence}/chapter-${id}.png`, animations: 'disabled' })
    await atChapter(page, id, .2)
    await expect.poll(() => page.locator(`#${id} ${effect}`).evaluate(el => el.outerHTML + getComputedStyle(el).transform + getComputedStyle(el).opacity)).toBe(early)
  }
  await atChapter(page, 'memory', .5)
  expect(await page.locator('.archive-fragments').evaluate(el => Number(getComputedStyle(el).zIndex))).toBeLessThan(await page.locator('.archive-papers').evaluate(el => Number(getComputedStyle(el).zIndex)))
  await page.locator('#extensions').scrollIntoViewIfNeeded()
  await expect(page.locator('.map-branch')).toBeVisible()
  expect(await page.locator('html').evaluate(el => el.scrollWidth <= innerWidth)).toBe(true)
})

test('desktop: native scroll leads the displayed scene briefly, then converges without changing scroll position', async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 1000 })
  await page.goto('./')
  await expect(page.locator('.product-chapters')).toHaveClass(/chapters-enhanced/)
  await atChapter(page, 'voice', .2)
  const samples = await page.evaluate(async () => {
    const section = document.getElementById('voice'), stage = section.querySelector('.chapter-stage')
    const start = scrollY, range = section.offsetHeight - stage.offsetHeight
    scrollTo({ top: start + 140, behavior: 'instant' })
    const samples = []
    for (let i = 0; i < 40; i++) {
      await new Promise(requestAnimationFrame)
      samples.push({ raw: -section.getBoundingClientRect().top / range, shown: Number(section.dataset.progress), scroll: scrollY })
    }
    return samples
  })
  expect(samples.some(({ raw, shown }) => raw - shown > .008)).toBe(true)
  expect(samples.every(({ raw, shown }) => shown <= raw + .002)).toBe(true)
  expect(samples.at(-1).shown).toBeCloseTo(samples.at(-1).raw, 3)
  expect(new Set(samples.map(sample => sample.scroll)).size).toBe(1)
  await page.evaluate(() => { window.chapterWrites = 0; window.chapterObserver = new MutationObserver(() => window.chapterWrites++); document.querySelectorAll('.story-chapter').forEach(el => window.chapterObserver.observe(el,{ attributes: true, attributeFilter: ['data-progress'] })) })
  await page.waitForTimeout(150)
  expect(await page.evaluate(() => { window.chapterObserver.disconnect(); return window.chapterWrites })).toBe(0)
})

test('reduced motion: all chapter content remains complete without long sticky stages or moving fragments', async ({ page }) => {
  await page.emulateMedia({ reducedMotion: 'reduce' })
  await page.setViewportSize({ width: 1440, height: 1000 })
  await page.goto('./')
  await expect(page.locator('.product-chapters')).not.toHaveClass(/chapters-enhanced/)
  await expect(page.locator('.archive-fragments')).not.toBeVisible()
  for (const id of ['together', 'memory', 'voice', 'extensions']) {
    await page.locator(`#${id} h2`).scrollIntoViewIfNeeded()
    await expect(page.locator(`#${id} h2`)).toBeInViewport()
    expect(await page.locator(`#${id} .chapter-stage`).evaluate(el => getComputedStyle(el).position)).toBe('relative')
  }
  expect(await page.locator('.archive-entry').count()).toBe(3)
  await page.goto('./en/#voice')
  await expect(page.locator('#voice h2')).toHaveText('More than words.A face that responds.')
  await expect(page.locator('#voice h2')).toBeInViewport()
  await expect(page.locator('.echo-question')).toHaveText('Can you feel my heart?')
})
