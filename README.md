# momo-infra

运维工具集 momoctl：doctor / pull / serve / switch / bench / gc。不承载业务，不在数据通路上。

设计文档：<https://github.com/Momoyeyu/momo-docs>（`02-repos/momo-infra.md`）

## 用法

```bash
cargo build --release          # 产物 target/release/momoctl

momoctl doctor                 # 环境检查矩阵（system/toolchain/python/network/env/models/endpoints）
momoctl doctor --json          # 脚本消费：{schema:1, profile, groups:[{name,checks:[{id,status,detail,hint}]}]}
momoctl pull [--only NAME]     # 按 momo-models 的 MANIFEST.toml 下载 + sha256 校验（stub）
momoctl serve up|down|status|logs|restart [--profile normal|degraded] [--only COMPONENT]
momoctl switch --to degraded [--restart]
momoctl bench [--turns N] [--strict]
momoctl gc [--dry-run] [--keep N] [--yes]
```

全局 flag：`--json`（单一 JSON 对象，schema 版本化）、`--env-file PATH`、`-v/-vv`。

## 退出码

| code | 含义 |
|---|---|
| 0 | 成功（doctor 全 ok / bench 达标） |
| 1 | 运行时错误（含未实现 stub） |
| 2 | 用法/配置错误（缺 env 等） |
| 3 | 检查未通过（doctor 有 fail / bench --strict 超标） |
| 4 | 网络不可达 |
| 5 | 校验失败（sha256 不匹配、manifest 损坏） |

## 布局

- `profiles/normal.toml` / `degraded.toml`：serve 组件表（组件、启动顺序、env 覆盖）
- `src/cmds/`：子命令；`src/serve/`：进程管理与 profile 解析
- 状态目录固定 `${XDG_STATE_HOME:-~/.local/state}/momo/`（pid/、logs/、active_profile）

## 开发

```bash
cargo fmt && cargo clippy && cargo test
```
