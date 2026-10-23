// Scroll is the only scene clock. Cell identities never depend on elapsed time.
export const clamp = value => Math.min(1, Math.max(0, value))
export function easeBetween(start, end, value) {
  const t = clamp((value - start) / (end - start))
  return t * t * (3 - 2 * t)
}
const mix = (a, b, t) => a + (b - a) * t
const hash = id => ((Math.imul(id + 17, 0x45d9f3b) ^ Math.imul(id + 113, 0x27d4eb2d)) >>> 0) / 4294967296
export function meetingFrame(progress, target, viewport) {
  const p = clamp(progress)
  const landing = easeBetween(.6, .98, p)
  const scale = Math.max(1, Math.min(viewport.width * .85 / target.width, viewport.height * .68 / target.height))
  return {
    x: (viewport.width / 2 - (target.left + target.width / 2)) * (1 - landing) || 0,
    y: (viewport.height * .49 - (target.top + target.height / 2)) * (1 - landing) || 0,
    scale: 1 + (scale - 1) * (1 - landing),
    rotation: -1.6 * easeBetween(.78, .98, p),
    reveal: easeBetween(.64, .7, p),
    presence: 1 - easeBetween(.08, .26, p),
    intro: 1 - easeBetween(.04, .16, p),
    chapter: easeBetween(.75, .94, p),
    landing,
  }
}

// Only called when the image loads: alpha decides the silhouette; three inks decide glyph weight.
export function presenceCells({ data, width, height }) {
  const cells = []
  const inks = [[243, 236, 223], [231, 190, 119], [24, 21, 31]]
  for (let id = 0; id < width * height; id++) {
    if (data[id * 4 + 3] < 96) continue
    let ink = 0, distance = Infinity
    inks.forEach((rgb, index) => {
      const d = rgb.reduce((sum, value, c) => sum + (value - data[id * 4 + c]) ** 2, 0)
      if (d < distance) { distance = d; ink = index }
    })
    cells.push({ id, u: (id % width + .5) / width, v: (Math.floor(id / width) + .5) / height, ink, seed: hash(id) })
  }
  const rows = Math.ceil(cells.length / 96)
  const destinations = Array.from({ length: cells.length }, (_, index) => index)
  for (let i = destinations.length - 1; i > 0; i--) {
    const j = Math.floor(hash(i) * (i + 1))
    ;[destinations[i], destinations[j]] = [destinations[j], destinations[i]]
  }
  cells.forEach((cell, index) => {
    // A stable permutation spreads the retained silhouette cells across the incoming room.
    const destination = destinations[index]
    cell.roomU = (destination % 96 + .5) / 96
    cell.roomV = (Math.floor(destination / 96) + .5) / rows
  })
  return cells
}
export function presenceCellFrame(cell, progress, source, room) {
  const p = clamp(progress), seed = cell.seed
  const travel = easeBetween(.2 + seed * .07, .52 + seed * .06, p)
  const x = source.left + cell.u * source.width, y = source.top + cell.v * source.height
  const tx = room.left + cell.roomU * room.width, ty = room.top + cell.roomV * room.height
  const arc = Math.sin(travel * Math.PI)
  const printOrder = cell.v * .65 + cell.u * .35
  return {
    x: mix(x, tx, travel),
    y: mix(y, ty, travel) - arc * (70 + seed * 100),
    alpha: easeBetween(.015 + printOrder * .1, .065 + printOrder * .1, p) * (1 - easeBetween(.56, .69, p)),
    travel,
  }
}
export function roomCellFrame(cell, progress) {
  const order = cell.u * .57 + cell.v * .36 + cell.seed * .07
  const reveal = easeBetween(.32 + order * .22, .39 + order * .22, progress)
  return { reveal, edge: Math.sin(reveal * Math.PI), alpha: progress < .7 ? 1 : 0 }
}

// CSS object-fit: cover, expressed in normalized source-image coordinates.
export function coverCrop(sourceAspect, targetAspect) {
  const width = Math.min(1, targetAspect / sourceAspect)
  const height = Math.min(1, sourceAspect / targetAspect)
  return { left: (1 - width) / 2, top: (1 - height) / 2, width, height }
}
