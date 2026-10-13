// 视图路由：menu（主页）/ scene（对话场景）/ settings（设置），body class 驱动显隐。
//
// menu 是全屏主页（标题 + 菜单项），stage 与 Live2D 在菜单下继续渲染；
// 进入 scene 前不连 ws——「开始」即进入她的房间。

export type View = "menu" | "scene" | "settings";

/** 切换视图；CSS 按 body.view-* 控制各元素显隐与安全区变量。 */
export function showView(view: View): void {
  document.body.classList.remove("view-menu", "view-scene", "view-settings");
  document.body.classList.add(`view-${view}`);
}

export function currentView(): View {
  if (document.body.classList.contains("view-scene")) return "scene";
  if (document.body.classList.contains("view-settings")) return "settings";
  return "menu";
}
