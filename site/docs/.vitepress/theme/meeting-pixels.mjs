import { clamp, coverCrop, presenceCells, presenceCellFrame, roomCellFrame } from './meeting-motion.mjs'

const INKS = ['#b6a28a', '#e7be77', '#615569']
const GLYPHS = ['╱', '─', '·', '╲', '┐', '└', '+', '░']
const canvasOwners = new WeakMap()
function sample(image, width, height) {
  const canvas = document.createElement('canvas')
  canvas.width = width; canvas.height = height
  const context = canvas.getContext('2d', { willReadFrequently: true })
  if (!context) throw new Error('Canvas is unavailable')
  context.drawImage(image, 0, 0, width, height)
  return context.getImageData(0, 0, width, height)
}

// A single scroll-rendered plate, with two readbacks at load and no perpetual animation loop.
export async function createMeetingPixels(canvas, figure, posterUrl) {
  const context = canvas.getContext('2d', { alpha: true })
  if (!context) throw new Error('Canvas is unavailable')
  const owner = Symbol('meeting renderer')
  canvasOwners.set(canvas, owner)
  const ownsCanvas = () => canvasOwners.get(canvas) === owner
  const poster = new Image()
  poster.src = posterUrl
  await Promise.all([figure.decode(), poster.decode()])
  if (!ownsCanvas()) throw new DOMException('A newer scene owns this canvas', 'AbortError')
  const sampleWidth = 96, sampleHeight = Math.round(96 * figure.naturalHeight / figure.naturalWidth)
  const cells = presenceCells(sample(figure, sampleWidth, sampleHeight))
  const roomColumns = 112, roomRows = 72
  const posterAspect = poster.naturalWidth / poster.naturalHeight
  const room = sample(poster, 224, Math.round(224 / posterAspect))
  const blocks = []
  for (let i = 0; i < roomColumns * roomRows; i++) {
    blocks.push({ u: (i % roomColumns + .5) / roomColumns, v: (Math.floor(i / roomColumns) + .5) / roomRows, seed: (i * 31 % 101) / 101, color: '' })
  }
  let colorAspect = 0
  function fitRoomColors(aspect) {
    const key = Math.round(aspect * 100000) / 100000
    if (key === colorAspect) return
    colorAspect = key
    const crop = coverCrop(posterAspect, aspect)
    // Refit cached source pixels on layout changes; scrolling never reads the image back.
    for (const cell of blocks) {
      const x = Math.min(room.width - 1, Math.floor((crop.left + cell.u * crop.width) * room.width))
      const y = Math.min(room.height - 1, Math.floor((crop.top + cell.v * crop.height) * room.height))
      const k = (y * room.width + x) * 4
      cell.color = `rgb(${room.data[k]},${room.data[k + 1]},${room.data[k + 2]})`
    }
  }
  let disposed = false
  return {
    draw(progress, viewport, sourceRect, roomRect) {
      if (disposed || !ownsCanvas()) return
      const ratio = Math.min(window.devicePixelRatio || 1, 1.5)
      const width = Math.round(viewport.width * ratio), height = Math.round(viewport.height * ratio)
      if (canvas.width !== width || canvas.height !== height) { canvas.width = width; canvas.height = height }
      context.setTransform(ratio, 0, 0, ratio, 0, 0)
      context.clearRect(0, 0, viewport.width, viewport.height)
      const p = clamp(progress)
      if (p <= 0 || p >= .7) return
      // The incoming room is printed cell by cell by one diagonal scanfront.
      const bw = roomRect.width / roomColumns, bh = roomRect.height / roomRows
      if (p > .32) fitRoomColors(roomRect.width / roomRect.height)
      if (p > .32) for (const cell of blocks) {
        const state = roomCellFrame(cell, p)
        if (!state.reveal || !state.alpha) continue
        const x = roomRect.left + (cell.u - .5 / roomColumns) * roomRect.width
        const y = roomRect.top + (cell.v - .5 / roomRows) * roomRect.height
        context.globalAlpha = state.alpha
        context.fillStyle = state.edge > .3 ? INKS[1] : cell.color
        const size = state.reveal
        // Integer device-pixel edges keep decoded cells opaque at shared seams.
        const left = Math.floor((x + bw * (1 - size) / 2) * ratio) / ratio
        const top = Math.floor((y + bh * (1 - size) / 2) * ratio) / ratio
        const right = Math.ceil((x + bw * (1 + size) / 2) * ratio) / ratio
        const bottom = Math.ceil((y + bh * (1 + size) / 2) * ratio) / ratio
        context.fillRect(left, top, right - left, bottom - top)
        // Small held scan traces follow the hot edge; their identity is tied to the cell.
        if (state.edge > .65 && cell.seed > .82) {
          context.globalAlpha = state.edge * state.alpha * .32
          context.fillRect(x - bw * 2, y + bh / 2, bw * 4, 1)
        }
      }
      // The outgoing silhouette supplies the glyphs; those same cells arc into the room.
      const glyphSize = Math.max(5, Math.min(11, sourceRect.width / sampleWidth * .92))
      context.font = `${glyphSize}px monospace`
      context.textBaseline = 'middle'; context.textAlign = 'center'
      for (const cell of cells) {
        const state = presenceCellFrame(cell, p, sourceRect, roomRect)
        if (state.alpha < .002 || state.x < -20 || state.x > viewport.width + 20 || state.y < -20 || state.y > viewport.height + 20) continue
        context.fillStyle = INKS[cell.ink]
        context.globalAlpha = state.alpha * (cell.ink === 2 ? .3 : .52)
        if (state.travel > .05 && state.travel < .94 && cell.seed > .88) {
          context.globalAlpha *= .35
          context.fillRect(state.x - 12 * state.travel, state.y, 20 * state.travel, 1)
          context.globalAlpha = state.alpha * .6
        }
        context.fillText(GLYPHS[(cell.id + cell.ink * 3) % GLYPHS.length], state.x, state.y)
      }
      context.globalAlpha = 1
      canvas.dataset.cells = String(cells.length)
      canvas.dataset.frame = p.toFixed(4)
    },
    dispose() {
      disposed = true
      // A delayed initializer may share this DOM canvas with its replacement.
      if (!ownsCanvas()) return
      canvasOwners.delete(canvas)
      context.clearRect(0, 0, canvas.width, canvas.height)
      canvas.width = 0; canvas.height = 0
    },
  }
}
