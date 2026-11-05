# Windows 桌面验证

017 改为 Electron，当前先验证 macOS arm64。旧壳 Windows 候选结果不适用于当前壳。

后续须验证：冻结 Python/native DLL、安装资源布局、Windows Job Object 或等价子进程树所有权、数据目录排他锁、中文路径、单实例、退出/异常退出、系统权限与安装卸载。未完成这些验证前不提供 Windows 发行声明。现有 prepare-windows / audit-windows 只处理 Python 依赖，不代表完整壳支持。
