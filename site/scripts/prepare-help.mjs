import { mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'

// Canonical documents live in the repository; generated pages ship with this site.
export function prepareHelp() {
  const destination = new URL('../docs/help/', import.meta.url)
  mkdirSync(destination, { recursive: true })
  for (const [source, target] of [['getting-started', 'getting-started'], ['tts', 'voice'], ['data', 'data']]) {
    const markdown = readFileSync(new URL(`../../docs/usage/${source}.md`, import.meta.url), 'utf8')
      .replaceAll('(tts.md)', '(./voice)')
      .replaceAll('(data.md)', '(./data)')
      .replace('[`deploy/tts/`](../../deploy/tts/)', '`deploy/tts/`')
    writeFileSync(new URL(`${target}.md`, destination), `<!-- Generated from docs/usage/${source}.md; edit the source. -->\n${markdown}`)
  }
  return fileURLToPath(destination)
}
