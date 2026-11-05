# Electron 桌面迁移（017）

共享业务唯一源为 momo-web-v3 的 frontend、backend、plugin-host。`app/migration/web-source.json` 记录晋级 revision 与逐文件 SHA-256；业务修复回源，再运行 `scripts/promote.py`。用户保全的旧检出不修改。

## 边界

Electron main 只拥有窗口、单实例、资源协议和 Python 子进程；preload 只暴露 `momoDesktop.getRuntime()`。业务沿用 REST / SSE / WS，没有另造 IPC 业务 API。contextIsolation、sandbox 和 webSecurity 开启，Node 不进入 renderer。只有本窗口主 frame 的 `momo://app` 来源能取令牌；子 frame、外链和非法参数拒绝。令牌不写入文件或 URL。

后端绑定随机 loopback 端口，以 nonce、PID、端口握手。55 秒未就绪即失败；退出先关闭 stdin，让 Python 收束活动，6.5 秒后仅回收拥有的 Unix 进程组。父进程意外死亡由 stdin EOF watchdog 收束。重复启动只聚焦现有窗口；Python 还持有数据目录排他锁，阻止 Web / 桌面双写。关闭主窗口退出应用。

## 开发与安装资源

macOS arm64 先交付候选。Electron 45.2.3 / Forge 8.0.1；shell strict TypeScript 编译，现有 Vite 构建 renderer，不增加第二套 Vite 插件管线。Forge 使用 hoisted pnpm 布局，按白名单打入 `.desktop` 和 `frontend/dist`；Python / Node / 模型 / 插件 / helper 放在 asar 外。普通玩家不用安装 Python、Node 或 Docker。

```sh
cd app
pnpm install --frozen-lockfile
(cd backend && uv sync --frozen && uv pip install pyinstaller==6.20.0)
python3 scripts/prepare-core.py
# 准备锁定的离线模型到 resources/models；见 scripts/prepare-models.py。
python3 scripts/freeze-backend.py
# Java 21 / Gradle 8.10.2 are build dependencies only.
(cd plugin-host/minecraft-fabric && gradle --no-daemon build)
python3 scripts/prepare-plugin-runtime.py
# Reviewed QQ resources only, never a live data/ or runtime-config.json.
python3 scripts/prepare-qq-bundle.py --resources /prepared/resources --manifest /prepared/manifest.json
(cd frontend && pnpm install --frozen-lockfile --ignore-workspace)
node scripts/frontend.mjs build
pnpm test:shell
pnpm build:macos
```

`pnpm dev` 使用同一打包资源契约，仅数据目录不同。重型构建在任务专属远端目录执行，收回产物/证据后清理。

## 数据与测试

正式 profile 为系统 appData/momo；开发为 appData/momo-development。`MOMO_DESKTOP_DATA_DIR` 可选择完全隔离的 profile；实际业务数据在其 data/。不自动覆盖旧应用数据，不直接移植旧 WebView 的 IndexedDB。旧版从设置导出带版本/校验的完整备份，再在新壳显式导入；恢复前备份且撤销活动，由共享 archive 模块处理。

原生 smoke：`MOMO_E2E_APP=/absolute/path/momo.app/Contents/MacOS/momo node desktop/test/native-smoke.mjs`。脚本创建并删除自己的 profile，仅测试进程临时开放 loopback CDP；正常启动不开放调试端口。浏览器测试通过不等于此项通过。录屏、麦克风、辅助功能的系统授权仍需真实系统验证。

QQ 资源闭包固定为 QQ 6.9.95/48517 与其独立 Electron 37.1.0 ABI，不装入主壳。首次启动校验并复制纯资源到当前 profile 的版本目录；账号、控制管道与 canary 从空目录创建，后续启动保留该 profile 自身会话。约增加 841 MiB 私有资源空间；旧资源版本保留供故障恢复。构建清单 SHA-256 在 `prepare-qq-bundle.py` 固定，原生发布 workflow 要求提供经过审阅的纯资源归档及校验值，不能从个人运行目录打包。

Forge 复制依赖时会把符号链接改为构建机绝对路径，统一资源列表的 afterCopyExtraResources 步骤将内部链接恢复为安装内相对链接，全部改写后再验证完整链，外逃链接直接失败。测试覆盖移除构建目录后的 Python dylib 与 QQ framework 链。

系统录屏使用 macOS 原生选择器（可用时），旧系统只允许明确选择来源；摄像头、外部 frame 和任意其他权限不放行。当前产物使用 ad-hoc 签名并执行 codesign 深度完整性校验，是未公证开发候选（无 Developer ID 身份）；系统授权体验、QQ 真实账号收发与第三方资源再分发资格仍需独立验证，未验证不宣称可发布。macOS 用户验收后才进入 Windows；Linux 未验证。不存在旧壳运行时兼容分支。
