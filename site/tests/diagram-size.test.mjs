import assert from 'node:assert/strict'
import test from 'node:test'
import { diagramWidth } from '../docs/.vitepress/diagram-size.mjs'

test('keeps wide diagrams near their intrinsic text size', () => {
  assert.equal(diagramWidth(1800, 700), 1680)
})

test('does not stretch a diagram narrower than the viewport', () => {
  assert.equal(diagramWidth(600, 700), 600)
})

test('fit mode uses the available viewport width', () => {
  assert.equal(diagramWidth(1800, 700, 'fit'), 700)
})

test('zoom applies after readable sizing', () => {
  assert.equal(diagramWidth(1800, 700, 'readable', 2), 3360)
})
