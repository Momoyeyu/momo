import { defineConfig } from 'vitepress'

export default defineConfig({
  title: 'momo',
  description: '本地可跑的 AI 伙伴 · 设计文档与开发指南',
  base: '/momo/',
  lang: 'zh-CN',
  cleanUrls: true,
  themeConfig: {
    nav: [
      { text: '介绍', link: '/' },
      { text: '架构', link: '/guide/architecture' },
      { text: '协议', link: '/guide/protocol' },
      { text: '流水线', link: '/guide/voice-clone' },
      { text: '路线图', link: '/guide/roadmap' },
      { text: 'GitHub', link: 'https://github.com/Momoyeyu/momo' }
    ],
    sidebar: [
      {
        text: '总览',
        items: [
          { text: '项目介绍', link: '/guide/' },
          { text: '总体架构', link: '/guide/architecture' },
          { text: '部署拓扑', link: '/guide/deployment' }
        ]
      },
      {
        text: '运行时',
        items: [
          { text: 'momo-ws 协议', link: '/guide/protocol' },
          { text: '回合状态机', link: '/guide/runtime' },
          { text: '打断 (barge-in)', link: '/guide/barge-in' }
        ]
      },
      {
        text: '训练流水线',
        items: [
          { text: '声线克隆', link: '/guide/voice-clone' },
          { text: '人格微调', link: '/guide/persona' },
          { text: '训练 job 与控制台', link: '/guide/infra-console' }
        ]
      },
      {
        text: '工程',
        items: [
          { text: '配置 (MOMO_*)', link: '/guide/config' },
          { text: '路线图与里程碑', link: '/guide/roadmap' },
          { text: '仓库与协作', link: '/guide/repos' }
        ]
      }
    ],
    outline: { level: [2, 3], label: '本页内容' },
    socialLinks: [
      { icon: 'github', link: 'https://github.com/Momoyeyu/momo' }
    ],
    footer: {
      message: 'Docs authored ahead of code — 文档先行',
      copyright: 'momo · MIT License · by Momoyeyu'
    },
    search: { provider: 'local' }
  },
  markdown: {
    math: false
  }
})
