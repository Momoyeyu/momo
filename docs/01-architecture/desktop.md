# 桌面宿主与形象生命周期

> 桌面宿主让 momo 成为日常桌面的一部分：可拖动、可交互、可收起，也能在设备切换和休眠后自然回到你身边。

## 启动、单实例与 readiness

```mermaid
flowchart TD
    LAUNCH["启动 momo"] --> SINGLE{"已有实例？"}
    SINGLE -->|"是"| FOCUS["唤起现有可见窗口 / menu"] --> END["结束新进程"]
    SINGLE -->|"否"| SHELL["创建 CompanionShell<br/>加载本地 settings"]
    SHELL --> CAP{"透明窗口与系统 menu 可用？"}
    CAP -->|"透明 + menu"| PET["创建透明 pet 窗口"]
    CAP -->|"透明不可用"| NORMAL["创建普通可见窗口 + menu"]
    CAP -->|"menu 不可用"| VISIBLE["普通可见窗口<br/>禁用 click-through"]
    PET --> PAR["并行加载 avatar 与 backend"]
    NORMAL --> PAR
    VISIBLE --> PAR
    PAR --> AVATAR{"avatar ready？"}
    AVATAR -->|"是"| IDLE["显示 idle companion"]
    AVATAR -->|"否"| AVATARFAIL["形象错误 / 重试<br/>设置与聊天仍可见"]
    PAR --> LLM{"LLM ready？"}
    LLM -->|"是"| TEXT["文字 reply 可用"]
    LLM -->|"否"| DRAFT["draft / 设置 / idle avatar 可用<br/>等待重试"]
    PAR --> VOICE{"ASR / TTS ready？"}
    VOICE -->|"是"| FULL["语音输入与有声回复可用"]
    VOICE -->|"否"| PARTIAL["按缺失能力禁用<br/>不阻止 LLM 文字 reply"]
```

不支持 Wayland/global shortcut/transparency 时使用普通可见窗口。系统 menu/tray 不可用时禁止 click-through，恢复入口必须留在可见窗口内。

## 首次运行、权限与文字保底

```mermaid
flowchart TD
    FIRST["首次打开聊天 / 语音"] --> EXPLAIN["先解释本地处理与模式"]
    EXPLAIN --> CHOICE{"用户选择输入"}
    CHOICE -->|"文字"| TEXT["LLM ready 时可用<br/>不请求麦克风"]
    CHOICE -->|"PTT / VAD"| ASK["系统麦克风权限请求"]
    ASK --> ALLOW{"允许？"}
    ALLOW -->|"是"| DEVICE{"设备与 ASR 可用？"}
    DEVICE -->|"是"| VOICE["保存模式偏好并开始 capture"]
    DEVICE -->|"否"| PICK["释放 mic；选择设备 / 重试 / 改文字"]
    ALLOW -->|"否"| FALLBACK["释放 mic；保留文字、draft 与设置"]
    PICK --> CHOICE
    FALLBACK --> TEXT
```

连续麦克风始终 opt-in；静态网站不请求该权限。LLM 不可用时可编辑和保留 draft，但不能生成 AI reply。

## avatar 加载与释放

```mermaid
flowchart TD
    SELECT["选择 avatar manifest"] --> CORE["加载 Cubism Runtime Core"]
    CORE --> MODEL3["读取 model3.json"]
    MODEL3 --> TEX["加载 textures"]
    TEX --> BASE{"必需资源完整？"}
    BASE -->|"否"| FAIL["错误卡片 + retry / 换模型"]
    BASE -->|"是"| OPTIONAL["加载 motions / expressions / physics"]
    OPTIONAL --> MISS{"可选资源缺失？"}
    MISS -->|"是"| PARTIAL["记录缺失能力<br/>仅启用实际存在的动作"]
    MISS -->|"否"| READY["绑定 AvatarAdapter + MotionDirector"]
    PARTIAL --> READY
    READY --> RUN["render；无 idle motion 时不承诺 idle 动作"]
    RUN --> SWITCH{"换模型还是退出？"}
    SWITCH -->|"换模型"| RELOAD["dispose model / textures / ticker"] --> SELECT
    SWITCH -->|"退出"| DISPOSE["dispose model / textures / ticker"] --> TERMINAL["结束"]
```

桃濑日和 Hiyori 是网站固定互动 demo，保持官方模型与角色设计原样。网站使用的 sample data、Cubism Core 与应用代码适用不同许可。未来桌面版若开放任意 avatar 导入与公开发布，必须按目标 SDK 与素材用途重新核对 publication license。

## 拖拽、点击、缩放与显示器恢复

```mermaid
flowchart TD
    DOWN["pointer down"] --> MOVE{"超过拖拽阈值？"}
    MOVE -->|"否，pointer up"| CLICK["触发 pet / tap reaction"]
    MOVE -->|"是"| DRAG["窗口拖拽；抑制 click"]
    DRAG --> RELEASE["保存逻辑坐标 + monitor id + DPI"]
    WHEEL["滚轮 / 设置缩放"] --> SCALE["限制 scale 范围并保存"]
    RELEASE --> CHANGE{"DPI / 显示器变化或重启"}
    SCALE --> CHANGE
    CHANGE --> RESTORE["按新 DPI 恢复逻辑坐标"]
    RESTORE --> EXISTS{"原 monitor 仍在？"}
    EXISTS -->|"是"| CLAMP["clamp 到可见工作区"]
    EXISTS -->|"否 / 拔出"| PRIMARY["迁移到主屏并 clamp"]
    CLAMP --> DONE["可见且可拖动"]
    PRIMARY --> DONE
```

## menu、穿透与退出

```mermaid
flowchart TD
    ENTRY["system menu / 可见窗口菜单"] --> ACTION{"操作"}
    ACTION -->|"隐藏"| HIDE["隐藏 pet；保留可恢复入口"]
    ACTION -->|"显示"| SHOW["恢复 pet 与上次位置"]
    ACTION -->|"置顶"| PIN["切换 always-on-top"]
    ACTION -->|"点击穿透"| SUPPORT{"可靠 menu 可恢复？"}
    SUPPORT -->|"否"| DISABLE["禁用 click-through"]
    SUPPORT -->|"是"| PASS["启用 click-through"] --> ESCAPE["menu 关闭穿透 / 恢复交互"]
    ACTION -->|"退出"| QUIT["Stop + 释放 mic / audio / avatar<br/>保存 transcript / settings"]
    QUIT --> OWNED["只终止 owned child process"]
    OWNED --> ATTACHED["断开 attached provider<br/>不关闭远端服务"]
    HIDE --> ENTRY
    SHOW --> ENTRY
    PIN --> ENTRY
    ESCAPE --> ENTRY
```

## sleep / wake

```mermaid
flowchart TD
    SLEEP["系统 sleep / 锁屏"] --> STOP["同步停 playback + 嘴型归零"]
    STOP --> RELEASE["释放 mic、AudioContext 与动画预算"]
    RELEASE --> CANCEL["取消活动 reply；保存 transcript"]
    CANCEL --> WAKE["wake / unlock"]
    WAKE --> DEVICES["重新枚举设备与权限"]
    DEVICES --> RECONNECT["重连 runtime / providers"]
    RECONNECT --> SNAP["session.snapshot<br/>不重放旧音频"]
    SNAP --> READY{"能力 ready？"}
    READY -->|"LLM ready"| IDLE["回 idle；文字 reply 可用"]
    READY -->|"部分缺失"| RETRY["保留 avatar / draft / 设置<br/>按能力重试"]
```

## 本地 UI settings

窗口位置、scale、置顶、穿透、输入模式、设备选择和 companion mode 是本地 UI settings，不是部署环境变量；密钥和服务 endpoint 仍由服务端配置管理。
