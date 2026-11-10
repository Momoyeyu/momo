# momo 原创人物原型 · 2026-11-10

这是由当前宣传剪影扩展的第一版全彩概念设定，不替换玩家使用的 Live2D。本目录纳入 Git，随源码公开保存；不混入游戏运行资源。

- `01-turnaround.png`：正面、3/4、左侧、背面及配色。
- `02-expressions.png`：六种表情和闭口／A I U E O 口型。
- `03-costume-details.png`：服饰正背面、领口、袖口、头饰、手势、鞋履。
- `momo-mark.svg`：头饰的精确品牌几何源文件。生成图中的金属质感仅作概念参考，制作时以 SVG 轮廓为准。
- 宣传剪影在 `../assets/mascot/momo-presence-v2.png`；历史版本由 Git 保存。
- `prompts.json`：本轮模型图生成提示词。

## 设计锁定

沿用非对称长卷发、墨紫／柔金／暖白、遮住部分右眼的侧刘海、荷叶领和钟形长袖。品牌头饰始终位于角色自身左侧鬓角（正面图的画面右侧）。

原剪影没有定义的部分，本轮补全为暖肤色、琥珀瞳、过膝 A 字裙、深色长袜与低跟短靴。这些是可继续调整的原型设计决策，不是既有角色设定。图中比例标尺仅供概念排版，不能作为精确制版尺寸；后续正式绘制仍需统一线稿、尺寸与结构。图片是合并栅格概念页，不是 Live2D 拆层工程。

## 制作参考

采用常见动画角色设定包结构，而非宣称存在唯一的动漫标准：统一角度的多视图、表情、色板及服装细节，并补充口型与手部参考。

- [SCAD Character Sheets](https://www.scad.edu/sites/default/files/PDF/Animation-design-challenge-character-sheets.pdf)
- [Toon Boom: Finalizing the character sheets](https://helpcentre.toonboom.com/hc/en-ca/articles/57134612631955-Character-Design-03-Finalizing-the-character-sheets)

生成工具为 OpenAI 内置 image_gen，具体模型版本由工具管理。以本项目剪影及第一张多视图作为一致性参考。
