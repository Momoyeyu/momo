# `docs/`：产品与系统设计

> `docs/` 汇集产品体验、系统协作、操作流程与工程约束；`site/` 通过 include 呈现同一份内容。

## 信息架构

```mermaid
flowchart TD
    PRODUCT["00-context<br/>产品 / 路线 / ADR / 研究"] --> ARCH["01-architecture<br/>系统 / 桌面 / 运行时 / 协议 / 流程 / 配置"]
    ARCH --> REPOS["02-repos<br/>模块实现边界"]
    ARCH --> FEATURES["03-features<br/>体验主题与扩展方向入口"]
    FEATURES --> TRACKS["04-tracks<br/>persona / voice"]
    REPOS --> OPS["05-ops<br/>部署 / 降级 / 模型 / 借用"]
    PRODUCT --> MGMT["06-management<br/>工程状态与项目协作"]
    ARCH -.-> SITE["site/docs/guide<br/>VitePress include wrappers"]
```

## 写作约束

- Mermaid 使用标准 fenced `flowchart`、`sequenceDiagram`、`stateDiagram-v2`；小图优先，每个用户操作包含入口、决策、效果、terminal/retry/cancel。
- 不杜撰性能；历史实测带日期，未验证数据写“待实测”。
- 协议/状态机先改 `01-architecture/`，再同步引用。
- env 默认值与模板只有 [config.md](../01-architecture/config.md) 是单一来源；UI settings 不伪装成 env。
- public guide 不复制图或协议，通过 include 消费 canonical 文件。

## 阅读入口

1. [产品](../00-context/product.md)
2. [路线图](../00-context/roadmap.md)
3. [系统](../01-architecture/system.md)
4. [桌面](../01-architecture/desktop.md)
5. [运行时](../01-architecture/runtime.md)
6. [通信协议](../01-architecture/protocol.md)
7. [操作路径](../01-architecture/workflows.md)
