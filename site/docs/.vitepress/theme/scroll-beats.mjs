// Keep the original scene choreography; only place it inside a longer scroll beat.
// The unused distance is deliberate reading time, in both scroll directions.
const beats = {
  meeting: [.12, .76],
  soul: [.18, .76],
  memory: [.16, .74],
  echo: [.16, .90],
  paths: [.18, .78],
}

export function sceneProgress(scroll, scene) {
  const [start, end] = beats[scene]
  return Math.max(0, Math.min(1, (scroll - start) / (end - start)))
}
