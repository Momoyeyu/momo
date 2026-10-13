import { defineConfig } from 'vitepress'
import { installMermaidFence } from './markdown-fence.mjs'
import { remapCanonicalLink } from './markdown-links.mjs'

export default defineConfig({
  title: 'momo',
  description: 'momo 是本地优先的 AI 桌宠：有形象，能听你说话，也能安静陪着你。',
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
          { text: '桌宠 MVP', link: '/guide/pipeline' },
          { text: '熟悉陪伴', link: '/guide/stage' },
          { text: '能力扩展', link: '/guide/dialogue' },
          { text: '路线图', link: '/guide/roadmap' },
        ],
      },
      {
        text: '体验与运行',
        items: [
          { text: '运行时与打断', link: '/guide/runtime' },
          { text: '部署与降级', link: '/guide/deployment' },
          { text: '配置', link: '/guide/config' },
          { text: '人格与上下文', link: '/guide/persona' },
          { text: '声线个性化', link: '/guide/voice-clone' },
          { text: '素材与许可', link: '/guide/credits' },
        ],
      },
      {
        text: '模块与运维',
        collapsed: true,
        items: [
          { text: 'Monorepo 边界', link: '/guide/repos' },
          { text: 'app 模块', link: '/guide/momo-app' },
          { text: 'models 模块', link: '/guide/momo-models' },
          { text: 'infra 控制面', link: '/guide/infra-console' },
          { text: '模型获取', link: '/guide/model-fetch' },
          { text: '降级手册', link: '/guide/degradation' },
          { text: '借用机器规则', link: '/guide/borrowed-etiquette' },
        ],
      },
      {
        text: '参考与工程',
        collapsed: true,
        items: [
          { text: '文档索引', link: '/guide/docs-index' },
          { text: '技术研究', link: '/guide/research' },
          { text: '架构决策', link: '/guide/decisions' },
          { text: '记忆与人格', link: '/guide/soul' },
          { text: '工具与感知', link: '/guide/capabilities' },
          { text: '直播扩展', link: '/guide/streaming' },
          { text: 'GitHub Project 策略', link: '/guide/github-project' },
        ],
      },
    ],
    outline: { level: [2, 3], label: '本页内容' },
    socialLinks: [{ icon: 'github', link: 'https://github.com/Momoyeyu/momo' }],
    footer: {
      message: '让桌面多一个，懂得回应的存在。',
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
