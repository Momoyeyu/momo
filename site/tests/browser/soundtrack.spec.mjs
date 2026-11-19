import { test, expect } from '@playwright/test'

for (const [name, width, height] of [['desktop', 1440, 900], ['mobile', 667, 807]]) {
  test(`${name}: soundtrack plays one full track at a time on demand`, async ({ page }) => {
    await page.setViewportSize({ width, height })
    const fetched = []
    page.on('request', request => {
      if (request.url().includes('/soundtrack/')) fetched.push(request.url())
    })
    await page.goto('./')
    const section = page.locator('#soundtrack')
    await section.scrollIntoViewIfNeeded()
    const tracks = section.getByRole('button')
    await expect(tracks).toHaveCount(6)
    expect(fetched).toEqual([])
    const audio = section.locator('audio')
    const successful = page.waitForResponse(response =>
      response.url().endsWith('/night-ost.mp3') && response.ok())
    await tracks.nth(3).click()
    await successful
    await expect(tracks.nth(3)).toHaveAttribute('aria-current', 'true')
    await expect.poll(() => audio.evaluate(player => player.paused ? 0 : player.duration)).toBeGreaterThan(180)
    await tracks.nth(1).click()
    await expect(tracks.nth(1)).toHaveAttribute('aria-current', 'true')
    await expect.poll(() => audio.evaluate(player => !player.paused && player.currentSrc.endsWith('connection-ost.mp3'))).toBe(true)
    await expect(audio).toHaveCount(1)
    expect(await section.evaluate(el => el.scrollWidth)).toBeLessThanOrEqual(width)
  })
}
