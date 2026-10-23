import { test, expect } from '@playwright/test'

async function trackTransitions(page) {
  await page.addInitScript(() => {
    window.tabSlides = []
    const original = document.startViewTransition?.bind(document)
    if (!original) return
    document.startViewTransition = update => {
      const transition = original(update)
      transition.ready.then(() => {
        const cursor = document.querySelector('.nav-indicator')
        window.tabSlides.push({
          direction: document.documentElement.dataset.pageSwitch,
          old: getComputedStyle(document.documentElement, '::view-transition-old(site-page)').animationName,
          next: getComputedStyle(document.documentElement, '::view-transition-new(site-page)').animationName,
          duration: getComputedStyle(cursor).transitionDuration,
        })
      }).catch(() => {})
      return transition
    }
  })
}
const nav = page => page.getByTestId('global-navigation')
const settled = page => expect(page.locator('html')).not.toHaveAttribute('data-page-switch')

test('desktop: one persistent header slides its cursor with the two opposing page views', async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 1000 })
  await trackTransitions(page)
  await page.goto('./')
  await expect(page.getByTestId('nav-indicator')).toHaveAttribute('data-ready','true')
  await page.evaluate(() => { window.initialHeader = document.querySelector('.site-header') })
  await nav(page).getByRole('link', { name: '使用帮助', exact: true }).click()
  await expect(page.locator('.help-page article h1')).toHaveText('初次见面')
  await settled(page)
  expect(await page.evaluate(() => window.initialHeader === document.querySelector('.site-header'))).toBe(true)
  expect(await page.evaluate(() => window.tabSlides[0])).toEqual({ direction: 'forward', old: 'page-leave-left', next: 'page-enter-right', duration: '0.35s, 0.35s' })
  const cursor = await page.getByTestId('nav-indicator').boundingBox(), selected = await nav(page).locator('a[aria-current]').boundingBox()
  expect(cursor.x).toBeCloseTo(selected.x, 0)
  expect(cursor.width).toBeCloseTo(selected.width, 0)
  await nav(page).getByRole('link', { name: '首页', exact: true }).click()
  await expect(page.locator('#intro-title')).toBeVisible()
  await settled(page)
  expect(await page.evaluate(() => window.tabSlides[1])).toMatchObject({ direction: 'backward', old: 'page-leave-right', next: 'page-enter-left' })
  expect(await page.locator('.site-header').count()).toBe(1)
  expect(await page.locator('html').evaluate(el => el.scrollWidth <= innerWidth)).toBe(true)
})

test('desktop: browser back and forward slide in the right direction and restore reading position', async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 1000 })
  await trackTransitions(page)
  await page.goto('./#memory')
  await page.evaluate(() => document.fonts.ready)
  const link = page.locator('#memory .text-link')
  await link.scrollIntoViewIfNeeded()
  await link.click()
  await expect(page).toHaveURL(/help\/data$/)
  await settled(page)
  await page.goBack()
  await expect(page).toHaveURL(/#memory$/)
  await settled(page)
  const saved = await page.evaluate(() => history.state.scrollPosition)
  expect(saved).toBeGreaterThan(1000)
  await expect.poll(() => page.evaluate(() => scrollY)).toBe(saved)
  await page.goForward()
  await expect(page).toHaveURL(/help\/data$/)
  await settled(page)
  expect(await page.evaluate(() => window.tabSlides.map(slide => slide.direction))).toEqual(['forward', 'backward', 'forward'])
})

test('desktop: unsupported transitions leave one usable page with a keyboard-accessible cursor', async ({ page }) => {
  await page.addInitScript(() => { document.startViewTransition = undefined })
  await page.goto('./')
  await expect(page.getByTestId('nav-indicator')).toHaveAttribute('data-ready','true')
  await nav(page).getByRole('link', { name: '使用帮助', exact: true }).press('Enter')
  await expect(page.locator('.help-page article h1')).toHaveText('初次见面')
  await settled(page)
  await nav(page).getByRole('link', { name: '首页', exact: true }).press('Enter')
  await expect(page.locator('#intro-title')).toBeVisible()
  await settled(page)
  expect(await page.locator('main').count()).toBe(1)
})

test('desktop: the last rapid tab selection wins before capture and during an active slide', async ({ page }) => {
  const errors=[]
  page.on('pageerror', error => errors.push(error.message))
  await trackTransitions(page)
  await page.goto('./')
  await expect(page.getByTestId('nav-indicator')).toHaveAttribute('data-ready','true')
  await page.evaluate(() => {
    const links=document.querySelectorAll('.primary-nav a')
    links[1].click();links[0].click()
  })
  await settled(page)
  await expect(page).toHaveURL(/\/momo\/$/)
  await expect(page.locator('#intro-title')).toBeVisible()
  await nav(page).getByRole('link',{name:'使用帮助',exact:true}).click()
  await expect.poll(() => page.evaluate(() => window.tabSlides.length)).toBeGreaterThan(0)
  await nav(page).getByRole('link',{name:'首页',exact:true}).click()
  await expect(page.locator('#intro-title')).toBeVisible()
  await settled(page)
  expect(await page.locator('main').count()).toBe(1)
  await expect(page.getByTestId('site-route-view')).not.toHaveAttribute('inert')
  expect(errors).toEqual([])
})

test('reduced motion: initial routes, locale changes and the cursor avoid unsolicited page motion', async ({ page }) => {
  await page.emulateMedia({ reducedMotion: 'reduce' })
  await trackTransitions(page)
  await page.goto('./help/voice?lang=en')
  await expect(nav(page).getByRole('link')).toHaveText(['Home','Guides'])
  await page.getByTestId('language-menu').locator('summary').click()
  await page.getByTestId('locale-ja').click()
  await expect(page).toHaveURL(/help\/voice\?lang=ja$/)
  await expect(nav(page).getByRole('link')).toHaveText(['ホーム','使い方'])
  await nav(page).getByRole('link', { name: 'ホーム', exact: true }).click()
  await expect(page).toHaveURL(/\/ja\/$/)
  await settled(page)
  expect(await page.evaluate(() => window.tabSlides)).toEqual([])
  expect(await page.getByTestId('nav-indicator').evaluate(el => getComputedStyle(el).transitionDuration)).toBe('0s')
})
