import test from 'node:test'
import assert from 'node:assert/strict'
import { archiveGlyph, responseBar } from '../docs/.vitepress/theme/chapter-motion.mjs'

test('archive fragments retrace the same route and leave no debris over a finished page', () => {
  for (let i = 0; i < 26; i++) {
    const middle = archiveGlyph(i, 26, .45)
    archiveGlyph(i, 26, .9)
    assert.deepEqual(archiveGlyph(i, 26, .45), middle)
    assert.equal(archiveGlyph(i, 26, 0).opacity, 0)
    assert.ok(Math.abs(archiveGlyph(i, 26, 1).opacity) < 1e-10)
    assert.equal(Math.abs(archiveGlyph(i, 26, 1).rotation), 0)
    assert.ok(middle.x >= 0 && middle.x <= 100)
    assert.ok(Number.isFinite(middle.y))
  }
})

test('the reply score reveals from left to right and remains still when scrolling stops', () => {
  assert.equal(responseBar(10, 0), 2)
  assert.equal(responseBar(60, 0), 2)
  assert.ok(responseBar(10, .3) > 2)
  assert.equal(responseBar(60, .3), 2)
  const held = Array.from({ length: 72 }, (_, i) => responseBar(i, .65))
  assert.deepEqual(Array.from({ length: 72 }, (_, i) => responseBar(i, .65)), held)
  assert.ok(held.every(height => height >= 2 && height < 150))
})
