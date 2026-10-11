import assert from 'node:assert/strict'
import { existsSync, readFileSync } from 'node:fs'
import path from 'node:path'
import test from 'node:test'
import { fileURLToPath } from 'node:url'

const siteRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const modelRoot = path.join(siteRoot, 'docs/public/live2d/hiyori')
const manifestPath = path.join(modelRoot, 'Hiyori.model3.json')
const manifestSource = readFileSync(manifestPath, 'utf8')
const manifest = JSON.parse(manifestSource)
const files = manifest.FileReferences

const expectedIdle = [
  'motions/Hiyori_m01.motion3.json',
  'motions/Hiyori_m02.motion3.json',
  'motions/Hiyori_m03.motion3.json',
  'motions/Hiyori_m05.motion3.json',
  'motions/Hiyori_m06.motion3.json',
  'motions/Hiyori_m07.motion3.json',
  'motions/Hiyori_m08.motion3.json',
  'motions/Hiyori_m09.motion3.json',
  'motions/Hiyori_m10.motion3.json',
]
const expectedTapBody = ['motions/Hiyori_m04.motion3.json']
const expectedReferences = [
  'Hiyori.moc3',
  'Hiyori.2048/texture_00.png',
  'Hiyori.2048/texture_01.png',
  'Hiyori.physics3.json',
  'Hiyori.pose3.json',
  'Hiyori.userdata3.json',
  'Hiyori.cdi3.json',
  ...expectedIdle,
  ...expectedTapBody,
]

test('Hiyori manifest has the pinned resource shape', () => {
  assert.deepEqual(files.Textures, ['Hiyori.2048/texture_00.png', 'Hiyori.2048/texture_01.png'])
  assert.deepEqual(files.Motions.Idle.map((motion) => motion.File), expectedIdle)
  assert.deepEqual(files.Motions.TapBody.map((motion) => motion.File), expectedTapBody)
  assert.equal(files.Motions.Idle.length, 9)
  assert.equal(files.Motions.TapBody.length, 1)
  assert.equal('Expressions' in files, false)
  assert.equal(manifestSource.includes('"Sound"'), false)
})

test('every Hiyori manifest reference stays inside the resource folder and exists', () => {
  const actualReferences = [
    files.Moc,
    ...files.Textures,
    files.Physics,
    files.Pose,
    files.UserData,
    files.DisplayInfo,
    ...Object.values(files.Motions).flat().map((motion) => motion.File),
  ]
  assert.deepEqual(actualReferences.sort(), expectedReferences.sort())
  for (const reference of actualReferences) {
    const resolved = path.resolve(modelRoot, reference)
    assert.equal(resolved.startsWith(`${modelRoot}${path.sep}`), true, reference)
    assert.equal(existsSync(resolved), true, reference)
  }
})

test('Live2D stage uses Hiyori motions without expression calls', () => {
  const stage = readFileSync(path.join(siteRoot, 'docs/.vitepress/theme/Live2DStage.vue'), 'utf8')
  assert.match(stage, /const MODEL_URL = '\/live2d\/hiyori\/Hiyori\.model3\.json'/)
  assert.equal(stage.includes('.expression('), false)
  assert.match(stage, /model\.motion\('TapBody', 0, forceMotionPriority\)/)
  assert.match(stage, /model\.motion\('Idle', selectedMotion, forceMotionPriority\)/)
})
