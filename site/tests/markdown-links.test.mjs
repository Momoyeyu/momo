import assert from 'node:assert/strict'
import test from 'node:test'
import { remapCanonicalLink } from '../docs/.vitepress/markdown-links.mjs'

test('remaps canonical relative markdown links', () => {
  assert.equal(remapCanonicalLink('../01-architecture/system.md'), '/guide/architecture')
  assert.equal(remapCanonicalLink('./desktop.md'), '/guide/desktop')
  assert.equal(remapCanonicalLink('../02-repos/momo-app.md'), '/guide/momo-app')
})

test('preserves hash and query suffixes', () => {
  assert.equal(remapCanonicalLink('./protocol.md#32-byte-二进制帧'), '/guide/protocol#32-byte-二进制帧')
  assert.equal(remapCanonicalLink('./roadmap.md?view=current#p0'), '/guide/roadmap?view=current#p0')
})

test('leaves external, hash-only, assets, and rooted routes unchanged', () => {
  assert.equal(remapCanonicalLink('https://example.com/file.md'), 'https://example.com/file.md')
  assert.equal(remapCanonicalLink('#核心类型'), '#核心类型')
  assert.equal(remapCanonicalLink('../assets/diagram.svg'), '../assets/diagram.svg')
  assert.equal(remapCanonicalLink('/momo/guide/architecture'), '/momo/guide/architecture')
  assert.equal(remapCanonicalLink('/guide/architecture'), '/guide/architecture')
})
