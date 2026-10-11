import { defineConfig } from "vite";

// momod 默认监听地址见 momo-docs 01-architecture/config.md（MOMO_ORCH_LISTEN）。
const MOMOD = "127.0.0.1:7800";

export default defineConfig({
  server: {
    proxy: {
      // dev server 代理 /ws → momod（momo-app.md §前端）。
      "/ws": { target: `ws://${MOMOD}`, ws: true },
      "/healthz": { target: `http://${MOMOD}` },
    },
  },
});
