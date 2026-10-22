import { defineConfig } from '@playwright/test'
export default defineConfig({
  testDir: './tests/browser', workers: 1, timeout: 30000,
  use: { baseURL: 'http://127.0.0.1:4176/momo/', locale: 'zh-CN', screenshot:'only-on-failure', channel: process.env.MOMO_SITE_BROWSER_CHANNEL || undefined },
  webServer: { command: 'pnpm preview --host 127.0.0.1 --port 4176', url: 'http://127.0.0.1:4176/momo/', reuseExistingServer: false },
  reporter: [['list']],
  projects: [
    { name: 'desktop', grep: /desktop:/ },
    { name: 'mobile', grep: /mobile:/ },
    { name: 'accessibility', grep: /reduced motion/ },
  ],
})
