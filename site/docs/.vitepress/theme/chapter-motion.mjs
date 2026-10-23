import { clamp } from './meeting-motion.mjs'

export const phase = (p, start, end) => {
  const t = clamp((p - start) / (end - start))
  return t * t * (3 - 2 * t)
}

// Stable positions: reversing a chapter retraces the same pieces, rather than
// generating another random cloud on each scroll event.
export function archiveGlyph(index, count, progress) {
  const t = phase(progress, .06 + (index % 5) * .025, .77 + (index % 3) * .04)
  const row = index % 3
  const startX = 5 + ((index * 37) % 48)
  const startY = 8 + ((index * 23) % 83)
  const endX = 62 + Math.floor(index / 3) / Math.max(1, Math.ceil(count / 3) - 1) * 24
  const endY = 17 + row * 27
  return {
    x: startX + (endX - startX) * t,
    y: startY + (endY - startY) * t - Math.sin(t * Math.PI) * (12 + row * 4),
    rotation: (1 - t) * ((index % 7) - 3) * 17,
    opacity: Math.sin(Math.PI * t) * .65,
  }
}

export function responseBar(index, progress) {
  const position = index / 71
  const wave = Math.exp(-Math.pow((position - .28) / .17, 2)) + .8 * Math.exp(-Math.pow((position - .72) / .13, 2))
  const tone = .18 + Math.abs(Math.sin(index * 2.17)) * .82
  const release = phase(progress, .09 + position * .35, .36 + position * .42)
  return 2 + wave * tone * 112 * release
}
