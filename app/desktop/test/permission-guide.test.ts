import assert from "node:assert/strict";
import {
  mkdirSync,
  mkdtempSync,
  realpathSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import test from "node:test";
import {
  momoAppBundle,
  permissionGuideCopy,
  permissionSettingsUrl,
} from "../main/permission-guide-config.ts";

const copy = {
  title: "授权辅助功能",
  instructions: "将应用拖到系统设置。",
  dragLabel: "拖移 momo.app",
  doneLabel: "完成",
  grantedLabel: "已授权",
  closeLabel: "关闭",
};

test("permission guide accepts only supported permissions and localizable text", () => {
  assert.deepEqual(permissionGuideCopy(copy), copy);
  assert.throws(() => permissionGuideCopy({ ...copy, title: "" }));
  assert.throws(() => permissionGuideCopy({ ...copy, instructions: 123 }));
  assert.throws(() =>
    permissionGuideCopy({ ...copy, dragLabel: "x".repeat(201) }),
  );
  assert.equal(
    permissionSettingsUrl("screen"),
    "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture",
  );
  assert.equal(
    permissionSettingsUrl("input"),
    "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
  );
  assert.throws(() => permissionSettingsUrl("file:///Applications"));
});

test("only a packaged momo executable and its own resources yield a drag bundle", (t) => {
  const root = realpathSync(
    mkdtempSync(path.join(tmpdir(), "momo-permission-guide-")),
  );
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const bundle = path.join(root, "Renamed momo.app");
  const macos = path.join(bundle, "Contents", "MacOS");
  const resources = path.join(bundle, "Contents", "Resources");
  mkdirSync(macos, { recursive: true });
  mkdirSync(resources);
  writeFileSync(path.join(bundle, "Contents", "Info.plist"), "<plist/>");
  const executable = path.join(macos, "momo");
  writeFileSync(executable, "");
  const electron = path.join(macos, "Electron");
  writeFileSync(electron, "");
  const alias = path.join(root, "momo");
  symlinkSync(executable, alias);
  assert.equal(momoAppBundle(true, executable, resources), bundle);
  assert.equal(momoAppBundle(true, alias, resources), bundle);
  assert.equal(momoAppBundle(false, executable, resources), undefined);
  assert.equal(momoAppBundle(true, electron, resources), undefined);
  assert.equal(momoAppBundle(true, executable, root), undefined);
  assert.equal(
    momoAppBundle(true, path.join(macos, "momo Helper"), resources),
    undefined,
  );
});
