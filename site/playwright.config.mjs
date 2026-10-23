import { defineConfig } from '@playwright/test'
const port = Number(process.env.MOMO_SITE_PORT || 4176)
export default defineConfig({
  testDir: './tests/browser', workers: 1, timeout: 30000,
  use: { baseURL: `http://127.0.0.1:${port}/momo/`, locale: 'zh-CN', screenshot:'only-on-failure', channel: process.env.MOMO_SITE_BROWSER_CHANNEL || undefined },
  webServer: { command: `node node_modules/vitepress/bin/vitepress.js preview docs --host 127.0.0.1 --port ${port}`, url: `http://127.0.0.1:${port}/momo/`, reuseExistingServer: false },
  reporter: [['list']],
  projects: [
    { name: 'desktop', grep: /desktop:/ },
    { name: 'mobile', grep: /mobile:/ },
    { name: 'accessibility', grep: /reduced motion/ },
  ],
})
