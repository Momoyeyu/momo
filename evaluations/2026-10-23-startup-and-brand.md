# 010 / 013：macOS 联合验收候选

状态：已实现、自测并打包，等待夜羽验收；不提前归入 done。本轮不改官网、不构建其他平台。

## 关键实现

- **首次连接**：默认只保留三个字段；说明和 DeepSeek 文字链接收进问号。提交前用实际模型完成最小生成探测，失败保留草稿、不保存、不开始开场。`Deepseek-chat` 与有效的 `deepseek-chat` 大小写不同是原 400 的直接原因；地址可达或模型列表可读不再足以通过。
- **异步边界**：离开配置页立即取消健康检查，逐次确认请求仍有效，防止旧结果在新页面保存或启动开场。探测不额外要求供应商支持 `max_tokens`，不回显原始错误或密钥。
- **人物与构图**：三语 README、启动和标题统一官网 C 三色剪影 `momo-presence-v2.webp`；应用与官网文件 SHA256 一致。A / B 仅留历史来源，不进入运行资源。标题略左移、标语右缩进，左侧菜单位于标语下方，剪影独立放右侧；三语、960 / 1280 桌面舞台无重叠。
- **进入对话**：仅 title → chat 播放 1.3 秒剪影 / ASCII / 实况房间交接；其他路径（包括设置、存读档、记录返回 chat）保留 RoPE。标题图像与粒子起点共用坐标。固定粒子、字形缓存与像素比上限，失败、后台、减少动态和快速导航均能释放输入。
- **标题按钮微调**：最大宽度从 320px 收至 272px，墨紫底色 38% alpha、局部模糊与细高光，文字保持不透明；悬停仍能辨认焦点。玩家要求重测 010 后，先备份正式配置与 SQLite，仅清空 LLM 连接并重置首次引导标记，核对记忆 / 会话 / 存档表内容未变；界面语言与其他配置保留。
- **备份布局**：放到通用设置两栏下方，缩小为横排两枚次级按钮，留出间距；导出与恢复逻辑保留。

## 验证

| 范围 | 实际结果 |
| --- | --- |
| 后端 | 283 通过，3 项 opt-in 跳过；包含真实生成探测、模型大小写、响应错误与参数兼容回归 |
| 前端单元 | 最终布局后 143 通过，34 个文件；原生构建含 TypeScript 检查 |
| 完整浏览器 E2E | 最终布局后 76 通过、5 项可选真实服务测试跳过；包含实际 ZIP / YAML / SQLite 备份恢复 |
| 原生 Tauri | 最终布局后 5 通过：启动 / 重载、打包资源与认证、三语资源来源、可见 ASCII、标题布局、三条工具页返回及备份区排版 |
| 真实模型 | 本轮独立原生配置流程 2 通过：错误模型被拒、使用与网页相同配置完成健康检查、首次问候、记名和后续真实对话 |
| 审查 | 独立 code-review-excellence 发现等待健康检查后导航的迟到写入问题，已修复并验证 RED → GREEN；返回转场问题也先复现失败再修复 |

macOS 生产构建与本地签封检查通过。唯一安装入口为 `/Applications/momo.app`，安装包为 `app/dist/momo-latest-macos-arm64.dmg`；清退旧构建 / E2E 应用和本轮测试配置，玩家配置、记忆、存档保留。此为本地 ad-hoc 候选，不宣称公证发行。

忽略的本地证据：`.workbuddy/010-connect-gate/` 中的 `final-backend.log`、`final-unit-followup.log`、`full-route-layout.log`、`native-route-layout.log`、`final-native-real.log`、`route-red/`、`title-layout-red/`、`route-layout-confirmed/`。截图在 `app/dist/native-e2e/`。测试使用独立数据目录，真实服务凭据仅作本轮私有 fixture，完成后删除；本轮未使用实验室机器。

## 对话框时序的全面修复

复现：返回设置、存档、读档或记录时，450ms 对话框动画早于 560ms RoPE 完成；直接启动时也在启动遮幕后提前播放。只限制 ASCII 入口不能解决这一层竞态。

- App 统一发出场景就绪，启动层实际移除后才允许 chat 入场；对话框和工具栏随后浮现。
- Chat 等待实际 CSS Animation.finished，完成后才开放输入、恢复麦克风和挂载 FirstMeeting。取消 / 减少动态也能结束等待；离页释放旧等待，已进行的对话回合不被中断。
- 逐帧检查原版 8 项均失败，修复后补足暂停 / 取消和快速跳转共 10 项。原生四条入口也逐帧记录：遮幕期间 opacity=0、输入未开放，随后记录到实际上浮帧。
- 最终浏览器全套 **86 通过、5 个 opt-in 跳过**；单元 **143 通过**、TypeScript / 生产构建通过；原生 **8 通过**（时序 1、资源 / 持久化 3、启动 1、完整玩家流程 3）。完整玩家流程使用隔离的确定性模型服务，实际经过原生 IPC / 后端 / SQLite / stdio MCP；本轮不重复外部真实 LLM / TTS。
- 旧功能测试发现 mount / displayed 不代表可输入，测试公共等待与直接输入的旧用例改为等待实际 inert 门槛释放；原生测试窗口需可见，不能用后台 WebKit 暂停时的 DOM 代替视觉成功。一次 WebDriver 脚本超时重试后，最终原生玩家流程全部通过。

本轮日志：`.workbuddy/010-connect-gate/dialog-red.log`、`dialog-final-browser.log`、`dialog-unit.log`、`dialog-typecheck.log`、`dialog-native.log`、`dialog-native-game-final.log`、`dialog-release-build.log`、`dialog-package.log`。逐帧证据在浏览器测试目录与 `app/dist/native-e2e/dialog-from-*.json`。最终候选替换 `/Applications/momo.app`；正式玩家配置 / 记忆 / 存档未重置。

## 010 / 013 独立代码审查（2026-10-23）

按 code-review-excellence 检查当前完整 diff 和新增文件：配置探测 / 持久化前置条件、离页取消、敏感信息、启动 / RoPE / ASCII / CSS 入场之间的状态交接、浮层与输入解锁、麦克风恢复、粒子生命周期。未发现可确认的 010 / 013 新增缺陷。审查者重跑探测 4 项、剪影取样 2 项全部通过；未将静态审查称为重跑 Mac 安装包。原生与浏览器既有验证边界沿用本文前述记录。

Windows 本轮额外预检：原生 WebView 使用最新冻结后端，错误模型保留引导且不保存，真实 Web 配置通过并完成首次对话，2 项通过；Windows 最终安装验证另见 `2026-10-23-windows-candidate.md`。
