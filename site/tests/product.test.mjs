import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync, existsSync, readdirSync, statSync } from 'node:fs'
import { helpLinks, releaseUrl } from '../docs/.vitepress/theme/product-content.mjs'
import { prepareHelp } from '../scripts/prepare-help.mjs'

test('help is generated from actual usage documents and shipped with the site', () => {
  prepareHelp()
  for (const value of Object.values(helpLinks)) {
    assert.ok(value.startsWith('/help/'))
    const text = readFileSync(new URL(`../docs${value}.md`, import.meta.url), 'utf8')
    assert.ok(text.length > 300)
    assert.doesNotMatch(text, /github\.com\/Momoyeyu\/momo\/blob\/master|\]\(\.\.\//)
  }
  assert.equal(releaseUrl, 'https://github.com/Momoyeyu/momo/releases')
})
test('showcase media is local and lightweight, with preserved provenance', () => {
  const root = new URL('../docs/public/media/', import.meta.url)
  assert.ok(statSync(new URL('momo-moments.webm', root)).size < 5 * 1024 * 1024)
  for (const name of ['first-meeting.png', 'hiyori-license.md', 'hiyori-notice.txt', 'asset-notice.txt']) assert.ok(existsSync(new URL(name, root)))
})
test('all font references resolve locally with preserved licenses', () => {
  const root = new URL('../docs/public/fonts/', import.meta.url)
  const css = readFileSync(new URL('fonts.css', root), 'utf8')
  for (const [,name] of css.matchAll(/url\(\.\/([^)]+)\)/g)) assert.ok(existsSync(new URL(name,root)),name)
  for (const name of ['notosanssc-OFL.txt','notosansjp-OFL.txt','nunito-OFL.txt']) assert.ok(existsSync(new URL(name, root)))
  assert.ok(readdirSync(root).length > 100)
})

test('three complete locale dictionaries keep the same product structure', async () => {
  const { copy, languages, localePath } = await import('../docs/.vitepress/theme/product-copy.mjs')
  assert.deepEqual(Object.keys(languages), ['zh', 'en', 'ja'])
  const shape = value => Array.isArray(value) ? value.map(shape) : value && typeof value === 'object' ? Object.fromEntries(Object.entries(value).map(([k,v]) => [k,shape(v)])) : 'text'
  for (const code of Object.keys(languages)) {
    assert.deepEqual(shape(copy[code]), shape(copy.zh), code)
    assert.equal(copy[code].faqs.length, 6)
    assert.equal(copy[code].horizon.nowItems.length, 4)
    assert.ok(copy[code].relationship.body.length > 40)
    assert.ok(existsSync(new URL(`../docs${localePath(code)}index.md`, import.meta.url)))
  }
})
