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

test("click-through excludes controls, follows dragged window and fails open if renderer stalls", () => {
  const { win, calls } = fixture();
  let clock = 100,
    point = { x: 0, y: 0 };
  const c = new WindowModeController(
    win,
    () => ({ x: 0, y: 0, width: 1440, height: 900 }),
    () => {},
    () => point,
    () => clock,
  );
  c.setMode("pet");
  const bounds = win.getBounds();
  const regions = [{ x: 0.8, y: 0, width: 0.2, height: 0.6 }];
  c.setInteractiveRegions(regions);
  point = { x: bounds.x + 50, y: bounds.y + 100 };
  c.setClickThrough(true);
  assert.deepEqual(calls.at(-1), ["ignore", true]);
  point = { x: bounds.x + 330, y: bounds.y + 100 };
  c.refreshPointer();
  assert.deepEqual(calls.at(-1), ["ignore", false]);
  // Movement uses current window bounds, not cached screen coordinates.
  win.setBounds({ ...bounds, x: bounds.x - 300 });
  c.refreshPointer();
  assert.deepEqual(calls.at(-1), ["ignore", true]);
  clock += 1001;
  c.refreshPointer();
  assert.deepEqual(calls.at(-1), ["ignore", false]);
  c.setInteractiveRegions(regions);
  assert.deepEqual(calls.at(-1), ["ignore", true]);
  c.setInteractiveRegions([]);
  assert.deepEqual(calls.at(-1), ["ignore", false]);
  c.setClickThrough(false);
  c.dispose();
});

test("malformed hit regions cannot poison native hit testing", () => {
  const { controller: c } = fixture();
  for (const value of [
    null,
    {},
    [{ x: NaN, y: 0, width: 1, height: 1 }],
    [{ x: -1, y: 0, width: 1, height: 1 }],
    [{ x: 0.8, y: 0, width: 0.4, height: 1 }],
    Array(65).fill({ x: 0, y: 0, width: 1, height: 1 }),
  ]) {
    assert.throws(() => c.setInteractiveRegions(value));
  }
});

test("pet drag uses native cursor deltas, clamps to work area and ends cleanly", () => {
  const { win } = fixture();
  let point = { x: 100, y: 100 };
  const c = new WindowModeController(
    win,
    () => ({ x: 0, y: 0, width: 1440, height: 900 }),
    () => {},
    () => point,
  );
  c.dragPet("start");
  point = { x: 150, y: 200 };
  c.dragPet("move");
  assert.equal(win.getBounds().x, 40);
  c.setMode("pet");
  const before = win.getBounds();
  c.dragPet("start");
  point = { x: 50, y: 100 };
  c.dragPet("move");
  assert.equal(win.getBounds().x, before.x - 100);
  assert.equal(win.getBounds().y, before.y - 100);
  c.dragPet("end");
  point = { x: 10, y: 10 };
  c.dragPet("move");
  assert.equal(win.getBounds().x, before.x - 100);
  c.dragPet("start");
  point = { x: -9999, y: -9999 };
  c.dragPet("move");
  assert.equal(win.getBounds().x, 0);
  assert.equal(win.getBounds().y, 0);
  c.setClickThrough(true);
  c.dragPet("start");
  point = { x: 200, y: 200 };
  c.dragPet("move");
  assert.equal(win.getBounds().x, 0);
  assert.throws(() => c.dragPet("unknown"));
  c.dispose();
});

test("utility panel preserves avatar position, closes at its moved location and resets for chat", () => {
  const { controller: c, win } = fixture();
  c.setMode("pet");
  const pet = win.getBounds();
  c.setPanel(true);
  const expanded = win.getBounds();
  assert.equal(expanded.width, pet.width + 292);
  assert.equal(expanded.x + 292, pet.x);
  assert.deepEqual(c.getState().panel, { width: 292, petWidth: 360 });
  win.setBounds({ ...expanded, x: expanded.x - 20 });
  c.setPanel(false);
  assert.equal(win.getBounds().width, 360);
  assert.equal(win.getBounds().x, pet.x - 20);
  c.setPanel(true);
  c.setMode("chat");
  assert.equal(c.getState().panel, undefined);
  assert.throws(() => c.setPanel("true"));
});

test("utility panel fits work area without changing avatar width", () => {
  const { controller: c, win, area } = fixture();
  c.setMode("pet");
  Object.assign(area, { width: 600 });
  win.setBounds({ x: 0, y: 0, width: 360, height: 520 });
  c.setPanel(true);
  assert.equal(win.getBounds().x, 0);
  assert.equal(win.getBounds().width, 600);
  assert.equal(c.getState().panel?.petWidth, 360);
  c.setPanel(false);
  assert.equal(win.getBounds().x, 240);
});
