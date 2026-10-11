# `infra/`：带外运维与训练编排

> `momoctl` 承担基础运维，Web 控制台与训练可视化属于 P2；不读取对话内容，不转发 turn 音频/文本，不实现推理。

## 能力

- doctor/readiness、normal/degraded profile 与服务启停；
- manifest fetch/checksum/probe/activate/rollback；
- 配置 draft validate/test/atomic apply/rollback，secret 独立保存；
- 训练 job 本地/SSH 执行、日志、cancel、checkpoint 恢复、promotion；
- CLI 与 Web 共用核心和 schema，控制台默认仅监听 localhost。

## 控制面

```mermaid
flowchart LR
    CLI["momoctl CLI"] --> CORE["infra core"]
    WEB["localhost Web console"] --> GUARD["localhost + launch token<br/>state-changing request + CSRF"] --> CORE
    CORE --> CONFIG["config draft + secrets"]
    CORE --> MODELS["manifest / checksum / activate"]
    CORE --> SERVICES["profiles / readiness / logs"]
    CORE --> JOBS["job store + local / SSH runners"]
    SERVICES -.-> APP["app runtime"]
    SERVICES -.-> ADAPTERS["models services"]
    CORE -.-> NOTE["不进入 turn 数据通路"]
```

## service 操作

```mermaid
flowchart TD
    ENTRY["start / stop / restart / status / logs"] --> AUTH{"localhost + token + CSRF 通过？"}
    AUTH -->|"否"| DENY["拒绝，不改变服务"]
    AUTH -->|"是"| OP{"操作"}
    OP -->|"status"| STATUS["读取 process identity + readiness"]
    OP -->|"start"| RUNNING{"已按同一 identity 运行？"}
    RUNNING -->|"是"| NOOP["幂等 no-op；返回当前状态"]
    RUNNING -->|"否"| DEPS["按依赖顺序启动"] --> READY{"bounded readiness 通过？"}
    READY -->|"是"| ACTIVE["标记 active"]
    READY -->|"否 / 用户 cancel startup"| CLEAN["停止本次 owned children<br/>保留 attached providers"]
    OP -->|"stop"| STOP["只停止选定 owned service"]
    OP -->|"restart"| STOP --> DEPS
    OP -->|"logs"| CURSOR["以 bounded cursor 读取"] --> LOST{"连接中断？"}
    LOST -->|"是"| RESUME["用最后 cursor 重连<br/>不重复无限 backlog"] --> CURSOR
    LOST -->|"否"| TAIL["持续 tail"]
```

## 训练 lifecycle 与恢复

```mermaid
stateDiagram-v2
    [*] --> queued
    queued --> canceled: cancel before launch
    queued --> preparing: validate data and environment
    preparing --> failed: validation / preparation failed
    preparing --> running: local / SSH launch with remote job id
    running --> canceling: user cancel
    canceling --> canceled: process identity confirmed stopped
    running --> recovering: SSH connection lost
    recovering --> running: same remote id + process identity found
    recovering --> failed: remote process confirmed gone / artifact invalid
    running --> failed: process exit failure
    running --> succeeded: artifact checksum / manifest passed
    failed --> preparing: retry from checkpoint
    succeeded --> promoted: frozen eval gate passed + explicit promote
```

重连按 remote job id 与 process identity 查询；未确认前绝不重复 launch。`canceled` 是独立终态，不写成 failed。rollback 只切回 active artifact version，不修改原 job 的 `succeeded` / `promoted` 历史状态。恢复沿用已有 checkpoint 与 training manifest，不新增训练配置或评分门槛。

## 模型激活

fetch 写临时目录 → checksum/revision 校验 → provider warmup/probe → 原子 activate。任一步失败保留 active 版本；promotion 后至少保留可回滚版本。详见 [model-fetch.md](../05-ops/model-fetch.md)。

## 验收

- [ ] CLI/Web 对同一操作给出同一 schema 与状态。
- [ ] 配置或模型 probe 失败时 active 不变。
- [ ] secret 不出现在 GET、日志、导出和静态网站。
- [ ] queued/preparing/canceling/recovering 路径不会报告假成功或重复启动。
- [ ] runtime 音频与对话文本不经过 infra。
