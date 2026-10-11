import { defineConfig } from 'vitepress'
import { installMermaidFence } from './markdown-fence.mjs'
import { remapCanonicalLink } from './markdown-links.mjs'

export default defineConfig({
  title: 'momo',
  base: '/momo/',
  lang: 'zh-CN',
  cleanUrls: true,
  appearance: true,
  head: [
    ['link', { rel: 'icon', href: '/momo/favicon.svg', type: 'image/svg+xml' }],
    ['script', {}, "try{const k='vitepress-theme-appearance';if(!localStorage.getItem(k))localStorage.setItem(k,'light')}catch(e){}"],
  ],
  themeConfig: {
    nav: [
      { text: '介绍', link: '/' },
      { text: '桌面', link: '/guide/desktop' },
      { text: '设计图谱', link: '/guide/architecture' },
      { text: '素材与许可', link: '/guide/credits' },
      { text: 'GitHub', link: 'https://github.com/Momoyeyu/momo' },
    ],
    sidebar: [
      {
        text: 'momo',
        items: [
          { text: '项目介绍', link: '/guide/' },
          { text: '总体架构', link: '/guide/architecture' },
          { text: '桌面宿主', link: '/guide/desktop' },
          { text: '操作路径图谱', link: '/guide/workflows' },
          { text: 'momo-ws 协议', link: '/guide/protocol' },
        ],
      },
      {
        items: [
          { text: '路线图', link: '/guide/roadmap' },
        ],
      },
      {
        text: '体验与运行',
        items: [
          { text: '运行时与打断', link: '/guide/runtime' },
          { text: '人格与上下文', link: '/guide/persona' },
          { text: '素材与许可', link: '/guide/credits' },
        ],
      },
      {
        collapsed: true,
        items: [
          { text: 'app 模块', link: '/guide/momo-app' },
        ],
      },
      {
        text: '参考与工程',
        collapsed: true,
        items: [
          { text: '文档索引', link: '/guide/docs-index' },
          { text: '架构决策', link: '/guide/decisions' },
          { text: 'GitHub Project 策略', link: '/guide/github-project' },
        ],
      },
    ],
    outline: { level: [2, 3], label: '本页内容' },
    socialLinks: [{ icon: 'github', link: 'https://github.com/Momoyeyu/momo' }],
    footer: {
      copyright: 'momo · 设计文档',
    },
    search: { provider: 'local' },
  },
  markdown: {
    math: false,
    config(md) {
      installMermaidFence(md)
      const originalNormalizeLink = md.normalizeLink.bind(md)
      md.normalizeLink = (url) => originalNormalizeLink(remapCanonicalLink(url))
    },
  },
})
