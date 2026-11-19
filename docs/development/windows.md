# Windows 桌面验证

## v0.3.0

当前 Electron x64 包在 `ssh laptop` 的原生 Windows 环境构建；WSL 只用于传输与编排，不将 Linux Python/Node 混入发行包。发行形式是完整目录的 ZIP，解压后运行 `momo.exe`，没有安装器和自动卸载器。

- 使用 Windows Python 冻结后端，`prepare-windows.py` 检查本地依赖并补齐 VC runtime。离线复用旧构建时只允许 `--audited-runtime` 从带清单的旧产物中取 SHA-256 验证通过的 VC DLL 与许可证，不复用旧业务后端。
- `prepare-plugin-runtime.py` 根据平台准备固定校验值的 Node 24.11.0，支持 `--node-archive`；Windows 使用 `npm.cmd`，不编译 macOS Swift helper。
- 资源检查只在 macOS 要求原生 helper 与 QQ manifest。Windows 的原生 QQ、电脑观察/操作及依赖 Unix 会话的 Minecraft 适配明确不可用，不展示可启用的假入口。
- 退出先正常关闭后端输入；超时兜底只对本应用持有的 PID 执行 `taskkill /PID … /T /F`，不按端口或名称杀其他服务。

本轮通过：冻结后端实际 E5/sqlite-vec、SenseVoice/VAD、鉴权、中文数据路径和 EOF 退出；原生 Electron 包 title、intro、settings、Boot/Live2D、使用本地测试 provider 的对话、设置持久化、单实例和退出后端口关闭。没有用浏览器开发服务器替代原生产物测试。

边界：本版没有商业代码签名；真实远端模型/TTS 服务、原生 QQ、computer use、真实 Minecraft 和强制崩溃/系统关机等场景不在本轮 Windows 验证范围。普通退出已验证不代表所有异常终止都已覆盖。

## v0.3.2

原生 Windows 11 x64 从源码完成 Electron、前端和冻结后端构建。Forge 在 Windows 打包阶段将已审计的 VC++ 运行库复制到 `momo.exe` 同级目录；`audit-windows.py --installed` 从包内的 `resources` 检查该位置和后端完整依赖。Windows 测试使用系统原生路径，后端退出与 `taskkill` 交错时等待真实进程退出事件。

从发布 ZIP 解压后，通过 PE 审计和原生验收。覆盖首次启动、设置、Live2D、测试会话、配置持久化、单实例和后端退出。冻结后端实测了 SenseVoice/VAD、E5/sqlite-vec、Unicode 数据目录与请求鉴权。v0.3.2 的 Windows 包包含退出和 CRT 修复，平台精确源码提交见同一 Release 的 `build-info.json`。
