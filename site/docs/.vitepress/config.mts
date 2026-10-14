import { defineConfig } from 'vitepress'
import { installMermaidFence } from './markdown-fence.mjs'
import { remapCanonicalLink } from './markdown-links.mjs'

export default defineConfig({
  title: 'momo',
  description: 'momo 是一个有形象、有性格、有自己记忆的伙伴：她能听你说话，也懂得怎么陪你。',
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
      { text: '路线图', link: '/guide/roadmap' },
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
        text: '功能',
        items: [
          { text: 'Web 交互闭环', link: '/guide/pipeline' },
          { text: '像人深化', link: '/guide/stage' },
          { text: '能力扩展', link: '/guide/dialogue' },
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
        text: '模块',
        collapsed: true,
        items: [
          { text: '模块边界', link: '/guide/repos' },
          { text: 'app 模块', link: '/guide/momo-app' },
        ],
      },
      {
        text: '参考与工程',
        collapsed: true,
        items: [
          { text: '文档索引', link: '/guide/docs-index' },
          { text: '架构决策', link: '/guide/decisions' },
          { text: '记忆与人格', link: '/guide/soul' },
          { text: '长期记忆', link: '/guide/memory' },
          { text: '记忆调研', link: '/guide/soul-memory-notes' },
          { text: '工具与感知', link: '/guide/capabilities' },
          { text: '直播扩展', link: '/guide/streaming' },
          { text: 'GitHub Project 策略', link: '/guide/github-project' },
        ],
      },
    ],
    outline: { level: [2, 3], label: '本页内容' },
    socialLinks: [{ icon: 'github', link: 'https://github.com/Momoyeyu/momo' }],
    footer: {
      message: '让屏幕里多一个，真正像人的存在。',
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
