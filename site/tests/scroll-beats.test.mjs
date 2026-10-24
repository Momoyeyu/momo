import test from 'node:test'
import assert from 'node:assert/strict'
import { sceneProgress } from '../docs/.vitepress/theme/scroll-beats.mjs'

for (const scene of ['meeting','soul','memory','echo','paths']) {
  test(`${scene}: readable opening and landing holds surround reversible motion`, () => {
    for (const p of [0,.04,.1]) assert.equal(sceneProgress(p,scene),0)
    for (const p of [.92,.97,1]) assert.equal(sceneProgress(p,scene),1)
    const raw=[0,.1,.2,.35,.5,.7,.9,1]
    const forward=raw.map(p=>sceneProgress(p,scene))
    assert.ok(forward[4]>0 && forward[4]<1)
    assert.ok(forward.every((p,i)=>i===0 || p>=forward[i-1]))
    assert.deepEqual(raw.toReversed().map(p=>sceneProgress(p,scene)).reverse(),forward)
    assert.equal(sceneProgress(-1,scene),0)
    assert.equal(sceneProgress(2,scene),1)
  })
}
