import { test } from "node:test";
import assert from "node:assert/strict";
import { WindowModeController } from "../main/window-mode.ts";
function fixture() {
  let bounds = { x: 40, y: 60, width: 1280, height: 800 };
  const area = { x: 0, y: 0, width: 1440, height: 900 };
  const calls: unknown[][] = [];
  const win = {
    getBounds: () => ({ ...bounds }),
    setBounds: (value: typeof bounds) => {
      bounds = value;
    },
    setMinimumSize: (...a: unknown[]) => calls.push(["min", ...a]),
    setAlwaysOnTop: (v: boolean) => calls.push(["top", v]),
    setIgnoreMouseEvents: (v: boolean) => calls.push(["ignore", v]),
    setBackgroundColor: (v: string) => calls.push(["background", v]),
    setWindowButtonVisibility: (v: boolean) => calls.push(["buttons", v]),
    setResizable: (v: boolean) => calls.push(["resize", v]),
    setHasShadow: (v: boolean) => calls.push(["shadow", v]),
    isFullScreen: () => false,
    isMaximized: () => false,
    isMinimized: () => false,
    restore: () => {},
    show: () => {},
    focus: () => {},
  };
  const states: unknown[] = [];
  const controller = new WindowModeController(
    win,
    () => area,
    (s) => states.push(s),
  );
  return { controller, calls, states, win, area };
}
test("pet uses same window, reduces bounds and restores chat with input enabled", () => {
  const { controller: c, win, calls } = fixture();
  const original = win.getBounds();
  c.setMode("pet");
  assert.equal(c.getState().mode, "pet");
  assert.equal(win.getBounds().width, 360);
  c.setClickThrough(true);
  assert.equal(c.getState().clickThrough, true);
  c.setMode("pet");
  assert.equal(c.getState().clickThrough, true);
  c.setMode("chat");
  assert.deepEqual(win.getBounds(), original);
  assert.deepEqual(c.getState(), { mode: "chat", clickThrough: false });
  assert.ok(calls.some((a) => a[0] === "ignore" && a[1] === false));
});
test("invalid inputs rejected and chat cannot be made click through", () => {
  const { controller: c } = fixture();
  assert.throws(() => c.setMode("bad"));
  assert.throws(() => c.setClickThrough("yes"));
  assert.throws(() => c.setClickThrough(true));
});
test("recovery clears pet pass-through and emits state", () => {
  const { controller: c, states } = fixture();
  c.setMode("pet");
  c.setClickThrough(true);
  c.recover();
  assert.deepEqual(states.at(-1), { mode: "chat", clickThrough: false });
});

test("fullscreen pet entry is rejected without changing state", () => {
  const { controller, win } = fixture();
  win.isFullScreen = () => true;
  assert.throws(() => controller.setMode("pet"));
  assert.deepEqual(controller.getState(), {
    mode: "chat",
    clickThrough: false,
  });
});
test("repeated pet toggle keeps original chat bounds", () => {
  const { controller, win } = fixture();
  const original = win.getBounds();
  controller.setMode("pet");
  win.setBounds({ x: 200, y: 250, width: 360, height: 520 });
  controller.setMode("pet");
  controller.setMode("chat");
  assert.deepEqual(win.getBounds(), original);
});

test("unplugged external display restores chat inside remaining work area", () => {
  const { controller, win, area } = fixture();
  Object.assign(area, { x: 1440, y: 0, width: 1920, height: 1080 });
  win.setBounds({ x: 1800, y: 100, width: 1280, height: 800 });
  controller.setMode("pet");
  Object.assign(area, { x: 0, y: 24, width: 1440, height: 876 });
  controller.recover();
  assert.deepEqual(win.getBounds(), {
    x: 160,
    y: 100,
    width: 1280,
    height: 800,
  });
});
test("already-chat recovery clamps to small offset work area and adjusts minimum size", () => {
  const { controller, win, area, calls } = fixture();
  win.setBounds({ x: 2500, y: -800, width: 1280, height: 800 });
  Object.assign(area, { x: -800, y: 30, width: 800, height: 500 });
  controller.recover();
  assert.deepEqual(win.getBounds(), {
    x: -800,
    y: 30,
    width: 800,
    height: 500,
  });
  assert.ok(calls.some((c) => c[0] === "min" && c[1] === 800 && c[2] === 500));
});
test("pet bounds and minimum size fit a small remaining display", () => {
  const { controller, win, area, calls } = fixture();
  Object.assign(area, { x: 20, y: 30, width: 280, height: 400 });
  controller.setMode("pet");
  assert.deepEqual(win.getBounds(), { x: 20, y: 30, width: 280, height: 400 });
  assert.ok(calls.some((c) => c[0] === "min" && c[1] === 280 && c[2] === 400));
});
