# momo 产品官网

此目录只介绍实际可用的产品体验。VitePress 提供静态构建，自定义 Vue 主题提供首页；使用说明在仓库 `docs/usage/`，开发说明在 `docs/development/`，构建时从 `docs/usage/` 生成站内帮助页，保持单一文档来源，不链接尚未发布的 GitHub 文件。

```sh
pnpm install --frozen-lockfile
pnpm test
pnpm build
pnpm exec playwright install chromium
pnpm test:browser
```

构建输出为 `docs/.vitepress/dist/`，保留 GitHub Pages `/momo/` 子路径。浏览器测试启动独立的生产预览服务，覆盖桌面、窄屏、键盘、字体失败、reduced-motion、素材及子路径。已有 Chrome 时可使用 `MOMO_SITE_BROWSER_CHANNEL=chrome pnpm test:browser`。

主题颜色和字体沿用 `app/frontend/`。`docs/public/media/first-meeting.png` 来自默认形象、默认房间与固定开局对白的隔离演示截图，没有私人数据。素材及字体许可随网站提供。`tests/product.test.mjs` 检查本地字体、素材入口及文案边界。

只有验证过且已发布的安装产物才能作为下载链接。当前没有公开发行包，因此首页如实说明状态，只链接真实的 GitHub 版本列表。发布后修改 `product-content.mjs` 及对应文案和测试。

首页使用可触摸 / 键盘切换的重叠卡片与 14 秒无声实录（`momo-moments.webm`）。素材来自隔离演示环境，字幕是合成测试对白，使用真实 Live2D / 字幕 / 表情控制，不含个人记录或角色语音。仅视频播放需要时加载，离屏、切卡、页面隐藏时暂停；reduced-motion / Save-Data 默认展示静态封面，仍可主动播放。

`prepare-help.mjs` 在 VitePress 配置加载时将仓库使用文档生成到 ignored 的 `docs/help/`；请编辑源文档而非生成文件。浏览器回归逐一请求首页、三份帮助和素材许可的站内链接，并检查锚点、`/momo/` 子路径、触摸与键盘切卡及真实视频解码。外部仓库 / 版本链接在交付前另外进行 HTTP 验证；网络不可用时不把未验证状态记作通过。
