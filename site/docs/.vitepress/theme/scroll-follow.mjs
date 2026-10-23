// Follow native scroll visually; the browser remains the only owner of scroll position.
// Progress is normalized (0–1). Explicit navigation and first observations paint exactly.
export function createScrollFollow({
  read,
  write,
  response = 18,
  raf = callback => requestAnimationFrame(callback),
  cancel = id => cancelAnimationFrame(id),
  now = () => performance.now(),
}) {
  const epsilon = .002
  let current, target, previousTime, frame
  let disposed = false

  function stop() {
    if (frame !== undefined) cancel(frame)
    frame = previousTime = undefined
  }

  function settle(value) {
    stop()
    current = target = value
    write(value)
  }

  function sample(immediate = false) {
    const value = read()
    // Large discontinuities are navigation/scrollbar jumps, not a gesture to replay.
    if (current === undefined || immediate || Math.abs(value - target) > .5) {
      settle(value)
      return false
    }
    target = value
    if (Math.abs(target - current) <= epsilon) {
      settle(target)
      return false
    }
    return true
  }

  function tick(time) {
    frame = undefined
    if (disposed || !sample()) return
    const seconds = Math.max(0, time - previousTime) / 1000
    previousTime = time
    current += (target - current) * -Math.expm1(-response * seconds)
    if (Math.abs(target - current) <= epsilon) {
      settle(target)
      return
    }
    write(current)
    if (!disposed && previousTime !== undefined) frame = raf(tick)
  }

  return {
    request({ immediate = false } = {}) {
      if (disposed || !sample(immediate) || frame !== undefined) return
      previousTime = now()
      frame = raf(tick)
    },
    stop,
    dispose() { stop(); disposed = true },
  }
}
