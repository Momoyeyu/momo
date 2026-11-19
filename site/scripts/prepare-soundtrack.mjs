import { copyFileSync, mkdirSync } from 'node:fs'

import { soundtrackTracks } from './soundtrack-tracks.mjs'

export function prepareSoundtrack() {
  const output = new URL('../docs/public/soundtrack/', import.meta.url)
  const source = new URL('../../app/frontend/public/audio/', import.meta.url)
  mkdirSync(output, { recursive: true })
  for (const [id] of soundtrackTracks) {
    const file = `${id}-ost.mp3`
    copyFileSync(new URL(file, source), new URL(file, output))
  }
}
