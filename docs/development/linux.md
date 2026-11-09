# Linux 桌面验证

v0.3.0 提供 Electron x64 便携 tar.gz；原生构建环境为 Linux Mint 22.3（Ubuntu noble 系）、glibc 2.39、Node 24.11.0、Electron 45.2.3。解压整个包，在图形会话运行 `./momo`；依赖宿主图形和音频系统库，未验证更旧 glibc 或其他发行版。运行不依赖 Docker。

冻结后端实际通过 E5/sqlite-vec、SenseVoice/VAD、鉴权、中文数据路径、EOF 退出。打包 UI 在 Xvfb 验证标题、设置、Boot/Chat、Live2D、本地 mock provider 流式回复、取景偏好重载和正常关闭后后端退出。无头测试使用软件 WebGL 标志，未写入发行默认启动参数；没有关闭 Chromium sandbox。

原生 QQ 与电脑控制只在 macOS 实现，Linux 包禁用相关入口；Minecraft JAR 随包但本轮未验证真实游戏联调。无头 smoke 不等于真实桌面 GPU、系统音频或所有发行版兼容性验证。

构建与测试在借用主机的独立临时目录中运行，收回 tar.gz、校验值和必要证据后清理本轮源代码、依赖、产物、数据和专用网络隧道，不留下安装实例。
