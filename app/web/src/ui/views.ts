//
// menu 是全屏主页（标题 + 菜单项），stage 与 Live2D 在菜单下继续渲染；
// 进入 scene 前不连 ws——「开始」即进入她的房间。


/** 切换视图；CSS 按 body.view-* 控制各元素显隐与安全区变量。 */
export function showView(view: View): void {
  document.body.classList.add(`view-${view}`);
}

export function currentView(): View {
  if (document.body.classList.contains("view-scene")) return "scene";
  if (document.body.classList.contains("view-settings")) return "settings";
  return "menu";
}
