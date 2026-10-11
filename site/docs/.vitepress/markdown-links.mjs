import path from 'node:path/posix'

export const canonicalRoutes = Object.freeze({
  'README.md': '/guide/docs-index',
  'product.md': '/guide/',
  'research.md': '/guide/research',
  'roadmap.md': '/guide/roadmap',
  'decisions.md': '/guide/decisions',
  'system.md': '/guide/architecture',
  'runtime.md': '/guide/runtime',
  'protocol.md': '/guide/protocol',
  'desktop.md': '/guide/desktop',
  'workflows.md': '/guide/workflows',
  'boundaries.md': '/guide/repos',
  'config.md': '/guide/config',
  'momo-app.md': '/guide/momo-app',
  'momo-models.md': '/guide/momo-models',
  'momo-infra.md': '/guide/infra-console',
  'momo-docs.md': '/guide/momo-docs',
  'p0-pipeline.md': '/guide/p0-pipeline',
  'p1-stage.md': '/guide/p1-stage',
  'p2-dialogue.md': '/guide/p2-dialogue',
  'p3-soul.md': '/guide/p3-soul',
  'p4-capabilities.md': '/guide/p4-capabilities',
  'p5-streaming.md': '/guide/p5-streaming',
  'persona.md': '/guide/persona',
  'voice-clone.md': '/guide/voice-clone',
  'deployment.md': '/guide/deployment',
  'degradation.md': '/guide/degradation',
  'model-fetch.md': '/guide/model-fetch',
  'borrowed-etiquette.md': '/guide/borrowed-etiquette',
  'github-project.md': '/guide/github-project',
  'resources.md': '/guide/credits',
})

const externalPattern = /^(?:[a-z][a-z\d+.-]*:|\/\/)/i

export function remapCanonicalLink(url) {
  if (!url || url.startsWith('#') || externalPattern.test(url)) return url
  if (url.startsWith('/')) return url

  const hashIndex = url.indexOf('#')
  const queryIndex = url.indexOf('?')
  const splitIndex = [hashIndex, queryIndex].filter((index) => index >= 0).sort((a, b) => a - b)[0] ?? url.length
  const pathname = url.slice(0, splitIndex)
  const suffix = url.slice(splitIndex)
  if (!pathname.toLowerCase().endsWith('.md')) return url

  const basename = path.basename(pathname)
  const route = canonicalRoutes[basename]
  return route ? `${route}${suffix}` : url
}
