import { defineConfig } from 'vitepress'

export default defineConfig({
  title: 'momo',
  base: '/momo/',
  lang: 'zh-CN',
  cleanUrls: true,
  themeConfig: {
    nav: [
      { text: '介绍', link: '/' },
    ],
    sidebar: [
      {
        items: [
          { text: '项目介绍', link: '/guide/' },
          { text: '总体架构', link: '/guide/architecture' },
      },
      {
        items: [
      },
      {
        items: [
      },
      {
        items: [
    ],
    outline: { level: [2, 3], label: '本页内容' },
    footer: {
    },
  },
  markdown: {
})
