import test from 'node:test'
import assert from 'node:assert/strict'
import { sampleEcho } from '../docs/.vitepress/theme/echo-motion.mjs'

test('human signal precedes the pause, gold return, and readable question', () => {
  const outgoing = sampleEcho(.22, 640, 400)
  const pause = sampleEcho(.40, 640, 400)
  const returning = sampleEcho(.60, 640, 400)
  assert.ok(outgoing.input > 0 && outgoing.reply === 0)
  assert.equal(pause.reply, 0)
  assert.ok(returning.reply > 0 && returning.question === 0)
  assert.equal(sampleEcho(.85,640,400).question,1)
})
test('echo is reversible, bounded, deterministic, and rests during the final fifth', () => {
  for (const [w,h] of [[640,400],[380,300]]) {
    const first=sampleEcho(.56,w,h)
    sampleEcho(.9,w,h)
    assert.deepEqual(sampleEcho(.56,w,h),first)
    for (const p of [0,.2,.4,.6,.8,1]) for (const glyph of sampleEcho(p,w,h).glyphs) {
      assert.ok(Number.isFinite(glyph.x) && Number.isFinite(glyph.y))
      assert.ok(glyph.opacity >= 0 && glyph.opacity <= 1)
    }
    assert.deepEqual(sampleEcho(.85,w,h),sampleEcho(1,w,h))
    assert.ok(sampleEcho(.6,w,h).glyphs.length <= 1200)
  }
})
test('the last impression keeps a faint incoming trace alongside the response', () => {
  const frame=sampleEcho(1,640,480)
  assert.ok(frame.glyphs.some(g=>!g.gold && g.opacity>.02))
  assert.ok(frame.glyphs.some(g=>g.gold && g.opacity>.15))
})
