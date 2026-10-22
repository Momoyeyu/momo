import { defineConfig } from 'vitepress'
import { prepareHelp } from '../../scripts/prepare-help.mjs'
import { copy } from './theme/product-copy.mjs'
prepareHelp()
const origin = 'https://momoyeyu.github.io/momo'
export default defineConfig({
  title: copy.zh.title,
  description: copy.zh.description,
  base: '/momo/', lang: 'zh-CN', cleanUrls: true, appearance: false,
  locales: {
    root: { label: '简体中文', lang: 'zh-CN', title: copy.zh.title, description: copy.zh.description },
    en: { label: 'English', lang: 'en', title: copy.en.title, description: copy.en.description },
    ja: { label: '日本語', lang: 'ja', title: copy.ja.title, description: copy.ja.description },
  },
  head: [
    ['meta', { name: 'theme-color', content: '#18151F' }],
    ['link', { rel: 'icon', href: '/momo/favicon.svg', type: 'image/svg+xml' }],
    ['link', { rel: 'stylesheet', href: '/momo/fonts/fonts.css' }],
  ],
  transformHead({ pageData }) {
    if (!/^(?:(?:en|ja)\/)?index\.md$/.test(pageData.relativePath)) return []
    const suffix = pageData.relativePath.startsWith('en/') ? '/en/' : pageData.relativePath.startsWith('ja/') ? '/ja/' : '/'
    return [
      ['link', { rel: 'canonical', href: origin + suffix }],
      ...[['zh-CN', '/'], ['en', '/en/'], ['ja', '/ja/'], ['x-default', '/']].map(([hreflang, path]) => ['link', { rel: 'alternate', hreflang, href: origin + path }]),
    ]
  },
})
