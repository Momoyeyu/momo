import path from 'node:path/posix'

export const canonicalRoutes = Object.freeze({
  'README.md': '/guide/docs-index',
  'product.md': '/guide/',
  'roadmap.md': '/guide/roadmap',
  'decisions.md': '/guide/decisions',
  'system.md': '/guide/architecture',
  'runtime.md': '/guide/runtime',
  'protocol.md': '/guide/protocol',
  'desktop.md': '/guide/desktop',
  'workflows.md': '/guide/workflows',
  'boundaries.md': '/guide/repos',
  'momo-app.md': '/guide/momo-app',
  'persona.md': '/guide/persona',
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
