# 014：数字肌理与人机交流的演出研究

2026-10-23。夜羽确认官网恢复，要求保留已认可首屏，提高后续章节的艺术与叙事质量，并补充 DeepSeek、GPT6-Astra 及关系寓言参考。本轮为研究 / 分镜候选，官网代码和线上版本未修改。

## 实际查看与来源

| 参考 | 查看范围 | 主要实现依据 |
| --- | --- | --- |
| [Claude / world.execute(me)](https://www.bilibili.com/video/BV186pc65E68/) | 延用前轮已核对的完整时间轴，本轮重读相关源码 | `Nyankomintsu/world-execute-me-lyric-mv@93ac637` 的 `engine/ascii.js`、`components/cursor.js`、`scenes/objects.js`、`scenes/climax.js`、`scenes/plates_love.js` |
| [DeepSeek / 聊天窗口](https://www.bilibili.com/video/BV1xCai6aE9g/) | 独立已登录浏览器从 0 连续播放，46 张采样覆盖到 209.7s；画面已进入最终问句与下一视频提示，总长 211.9s，未以播放器 `ended` 确认最后约 2 秒 | `MisakaZentai/world-execute-me-dsh-pv@a598092`；`docs/HOW_IT_WORKS.md`、`continuity_full_v2/kit.py`、`s_chorus1.py` |
| [GPT6-Astra / 字符 MV](https://www.bilibili.com/video/BV1Jwhy6BEMJ/) | 已登录连续播放到结束，0.3–211.9s，47 张采样 | `yym8224961/world.execute-me-ascii@3409cf0`；`scenes.py`、`docs/prompts/creation-prompts.md` |
| [夜羽新增的关系寓言](https://www.bilibili.com/video/BV1Gsnj6XE9M/) | 已登录连续播放到结束，0–257.5s，57 张采样 | 实际画面与作者简介；未找到可核实的配套源码，不将其他 MV 的代码当作它的实现 |
| [claude.dev](https://claude.dev/) | 实际访问页面、查看像素插画与局部反馈，读取公开 JS | 当日公开构建中的 `scramble`、有退出点的 hover 动画、Canvas 字符 / 像素绘制；不推断其技术博客就是 momo 应沿用的产品布局 |

新视频使用单独的临时研究浏览器，先确认 Bilibili 登录成功，检查播放器未被登录弹窗遮挡；认证仅在进程内传递，没有写出 cookie 文件。研究浏览器已退出。采样不是逐帧检查，不据此宣称硬件帧率或逐帧无瑕疵。

参考源码、采样与联系表仅在忽略目录 `.workbuddy/014-digital-heart/`。原有 Claude 逐帧研究继续保留在 `.workbuddy/012-video-study-v2/`。

## 可迁移的机制

| 源码 / 观察 | 为什么有效 | momo 的转化 |
| --- | --- | --- |
| Claude 的 `glyphPlate` 按墨色和边缘方向组织字符，`inkScreen` 将字行裁在人物墨版里 | 字符构成有意义的对象，留白和轮廓先于字符数量 | 用数字织带、留下的记忆和回传信号承担具体含义；首屏的剪影解体保留独有性 |
| Claude 游标是人的唯一身体；双方有不同墨色、方向和停顿 | 角色关系从空间与动作中可读 | 暖白输入、柔金回应，往返与停顿有因果，最后问句由 momo 发出 |
| DeepSeek 的主镜头表、`LEAD` 和 `CARRY / UNFOLD` 交接；同一对象跨窗口继续 | 每时刻有一个视觉中心，表面的聊天和内部变化相互说明 | 全段只有一个主要变化；文字、记忆和信号连续出现，不贴互不相关的效果 |
| Astra 的 `lyric_infinity_limit` 做深度排序与字符织带；`lyric_vibration_sync` 表达两路信号从失配到建立联系 | 同一字符材料能形成不同空间和关系，过程本身承担情绪 | 独立实现空间织带和双向回声；不复制其公式、临床心电图外观、监测数字或终端状态 |
| 新增关系寓言反复出现门、递出的物件、书与馈赠；后段此前物件的意义发生变化 | 情绪来自关系与前后呼应 | 官网用同一条信息留下痕迹、再收到回应；游戏成长专题另外研究玩家如何影响关系 |
| claude.dev 的短时字符解码、像素小图和 hover 后停止 | 微反馈回应操作，细节有局部边界 | 正文保持真实 DOM；装饰解码另层绘制，键盘等价、反馈有结束点 |

没有照搬示例的音乐、歌词、角色、配色、终端界面或技术术语。源码只作机制研究，产品实现仍应独立编写。

## 下一版分镜的主张

现有后续章节的主要问题是变化缺少因果：字标错位、纸页展开和静态声波分别能说明素材，却很难让观众感到“屏幕另一端有一个正在回应的存在”。

下一轮提案：稳定的 momo 核心与改变的数字织带、交流经过后留下的记忆、跨过暗缝又折返的信号。三段分别采用空间形变、扫描留痕、双向回声；以 **Can you feel my heart?** 收束。先做双向回声候选验证气质，再扩展其余章节。具体范围与验收只维护在 Obsidian v2 的 `plan/014-digital-heart-website.md`。

## 游戏启发的边界

夜羽提出的“被控制 / 自由成长”记录为未来成长系统的候选议题：玩家回应、共同经历和实际接入能力，如何长期影响 persona 与她愿意探索的事情；soul 继续稳定。暂未形成路线、数值或结局规则，不在官网宣传为已实现自进化，也不把同人故事当作现实模型人格或训练过程的事实依据。
