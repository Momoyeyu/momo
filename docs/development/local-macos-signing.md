# 本机体验版签名与系统授权

旧包用 ad-hoc 签名，其 designated requirement 绑定每版 cdhash。升级会改变身份；macOS 中同名 momo 的旧录屏/辅助功能记录不代表新包获准。这不是通过重开应用或反复切开关就能稳定修复的问题。

## 当前构建规则

- 本机使用钥匙串里的固定 `momo Local Development` 代码签名身份。私钥不进仓库、安装包、数据备份或远程构建机；不在每次构建时重新生成证书。
- Forge 默认要求这份有效身份，找不到就失败。`CSC_NAME` 可指定已有证书（优先用唯一指纹）。`CSC_NAME=-` 只允许无私钥的远端生成 staging，不作为可安装版本。
- 收回 staging 后在本机运行 `node scripts/sign-local-macos.cjs /path/to/momo.app`。它签署 OS 交互 helper、Electron 嵌套代码及外层资源封印，执行 deep/strict 验证，并用已安装版本的 designated requirement 验证候选。资源中的第三方依赖保留其现有签名，受外层资源封印保护。
- 稳定身份必须同时绑定 `dev.momo.electron` 和证书；拒绝 cdhash-only 或 identifier-only requirement。证书丢失或更换不能静默退回 ad-hoc。
- 首次从旧 ad-hoc 身份迁移需要显式 `--migrate-identity`。这只允许记录一次身份迁移，不授予任何系统权限。之后升级不使用该参数。
- 不直接重签 `/Applications` 中运行的应用。先签 staging、跑隔离 smoke，再退出旧进程，备份当前 profile，集中替换唯一安装版。

## 首次迁移与验证边界

旧 ad-hoc 的录屏/辅助功能记录可能需要由用户在系统设置中移除，再添加固定签名的新包；系统要求时 Quit & Reopen。只处理 momo 自己的记录，不修改 TCC 数据库，不重置其他应用，不绕过确认。

两个不同内容版本具有相同 requirement、通过彼此 requirement 验证，只证明代码身份稳定。实际系统授权继承还需在固定签名版本首次授权后，再更新并执行录屏/辅助功能检查；未做这一步前不得报告 TCC 继承已通过。

观察桌面不需要 momo 内应用授权；只有 macOS 的录屏能力要求系统授权。点击/输入仍保留 momo 的应用级一次/始终允许机制。本次签名不会扩大任何观察或控制范围。

这是本机体验版身份，不是 Developer ID、notarization 或公开发行方案。证书的信任仅限当前用户的 code-signing policy；不是通用 TLS 根证书。

## 2026-11-07 验证

固定身份已导入本机 login 钥匙串；临时 PEM 私钥已清理，仅保留公开证书。两个内容不同的测试 Mach-O 使用同一证书签署后，requirement 都绑定相同证书与 bundle ID，并通过交叉验证。33 项 shell 测试、三语桌宠浏览器回归及两仓 CI 通过。此处尚不代表首次授权后跨安装包的 TCC 继承已实测通过。

首次签名若系统要求 codesign 使用私钥，由用户在钥匙串提示确认；不读取登录密码、不降低整个钥匙串的访问限制。不在开发机的签名脚本里设置全应用私钥访问。

固定签名安装记录：runtime `af5ee40` / shared `2ff32cc` 已通过安装产物 smoke（临时 profile、Boot/Chat/Settings、桌宠及侧栏、备份恢复、进程退出），随后替换 `/Applications/momo.app` 并实际验证观察开关和返回标题。签名工具的策略兼容修复 `d8c210d` 使用 `security find-identity -v -p codesigning` 精确匹配证书指纹，避免 osx-sign 的通用策略查询漏掉仅受 code-signing 信任的证书；34 项 shell 测试和 CI 通过。

新包与 helper 均绑定固定证书。安装后的 TCC 日志仍指向旧 ad-hoc requirement，确认首次迁移确有必要；已退出应用并通过 `tccutil reset ScreenCapture dev.momo.electron`、`tccutil reset Accessibility dev.momo.electron` 仅撤销 momo 的旧记录。新的系统授权正在等待用户确认，尚未宣称跨更新继承通过。旧安装包及远端构建目录已清理，只保留当前安装和最新 pre-install profile。

参考：[Apple TN3127](https://developer.apple.com/documentation/technotes/tn3127-inside-code-signing-requirements)、[Apple TN2206](https://developer.apple.com/library/archive/technotes/tn2206/)。
