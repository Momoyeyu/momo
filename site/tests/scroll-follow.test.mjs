import test from 'node:test'
import assert from 'node:assert/strict'
import { createScrollFollow } from '../docs/.vitepress/theme/scroll-follow.mjs'

function fixture(initial = 0) {
  let time = 0, nextId = 0, target = initial
  const frames = new Map(), values = []
  const follow = createScrollFollow({
    read: () => target,
    write: value => values.push(value),
    now: () => time,
    raf: callback => { const id = ++nextId; frames.set(id, callback); return id },
    cancel: id => frames.delete(id),
  })
  const step = milliseconds => {
    time += milliseconds
    const pending = [...frames.values()]
    frames.clear()
    for (const callback of pending) callback(time)
  }
  return {
    follow, frames, values, step,
    set target(value) { target = value },
    get value() { return values.at(-1) },
    advance(milliseconds, rate = 60) {
      const count = Math.round(milliseconds / 1000 * rate)
      for (let i = 0; i < count; i++) step(milliseconds / count)
    },
  }
}

test('first observation paints the actual position without an offscreen entrance', () => {
  const scene = fixture(.63)
  scene.follow.request()
  assert.equal(scene.value, .63)
  assert.equal(scene.frames.size, 0)
})

test('displayed progress follows elapsed time equally at 60 and 120 Hz', () => {
  const scenes = [fixture(), fixture()]
  for (const scene of scenes) {
    scene.follow.request()
    scene.target = .4
    scene.follow.request()
  }
  scenes[0].advance(200, 60)
  scenes[1].advance(200, 120)
  assert.ok(scenes[0].value > .38 && scenes[0].value < .4)
  assert.ok(Math.abs(scenes[0].value - scenes[1].value) < 1e-12)
})

test('continuous native scroll has a short bounded lag and reverses without overshoot', () => {
  const scene = fixture()
  scene.follow.request()
  for (let i = 1; i <= 30; i++) {
    const target = i / 80
    scene.target = target
    scene.follow.request()
    scene.step(1000 / 60)
    assert.ok(scene.value <= target)
    assert.ok(target - scene.value < .045)
  }
  const beforeReverse = scene.value
  scene.target = .15
  scene.follow.request()
  scene.step(1000 / 60)
  assert.ok(scene.value < beforeReverse)
  assert.ok(scene.value > .15)
  scene.advance(400)
  assert.equal(scene.value, .15)
})

test('a stopped gesture reaches its exact target and releases the animation loop', () => {
  const scene = fixture()
  scene.follow.request()
  scene.target = .4
  scene.follow.request()
  scene.follow.request()
  assert.equal(scene.frames.size, 1)
  scene.advance(350)
  assert.equal(scene.value, .4)
  assert.equal(scene.frames.size, 0)
  const writes = scene.values.length
  scene.advance(500)
  assert.equal(scene.values.length, writes)
})

test('explicit navigation cancels pending motion and paints the exact requested phase', () => {
  const scene = fixture()
  scene.follow.request()
  scene.target = .35
  scene.follow.request()
  scene.step(16)
  scene.target = .48
  scene.follow.request({ immediate: true })
  assert.equal(scene.value, .48)
  assert.equal(scene.frames.size, 0)
  scene.advance(200)
  assert.equal(scene.value, .48)
})

test('a discontinuous scroll jump snaps rather than dragging an old scene across the page', () => {
  const scene = fixture()
  scene.follow.request()
  scene.target = .9
  scene.follow.request()
  assert.equal(scene.value, .9)
  assert.equal(scene.frames.size, 0)
})

test('pause releases work and resume does not count hidden time as animation time', () => {
  const scene = fixture()
  scene.follow.request()
  scene.target = .4
  scene.follow.request()
  scene.step(50)
  const paused = scene.value
  scene.follow.stop()
  assert.equal(scene.frames.size, 0)
  scene.step(10000)
  assert.equal(scene.value, paused)
  scene.target = .3
  scene.follow.request()
  scene.step(16)
  assert.ok(scene.value > paused && scene.value < .3)
})

test('dispose prevents pending and later requests from writing into a removed scene', () => {
  const scene = fixture()
  scene.follow.request()
  scene.target = .4
  scene.follow.request()
  scene.follow.dispose()
  assert.equal(scene.frames.size, 0)
  const writes = scene.values.length
  scene.target = 1
  scene.follow.request({ immediate: true })
  scene.advance(1000)
  assert.equal(scene.values.length, writes)
})
