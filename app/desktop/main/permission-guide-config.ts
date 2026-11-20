import { realpathSync, statSync } from "node:fs";
import path from "node:path";
import type { PermissionGuideCopy } from "../contracts.ts";

export function permissionSettingsUrl(kind: unknown): string {
  if (kind !== "screen" && kind !== "input") {
    throw Error("Invalid system permission");
  }
  const pane =
    kind === "screen" ? "Privacy_ScreenCapture" : "Privacy_Accessibility";
  return "x-apple.systempreferences:com.apple.preference.security?" + pane;
}

export function permissionGuideCopy(value: unknown): PermissionGuideCopy {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw Error("Invalid permission guide copy");
  }
  const input = value as Record<string, unknown>;
  function text(key: keyof PermissionGuideCopy, max: number): string {
    const copy = input[key];
    if (typeof copy !== "string" || !copy.trim() || copy.length > max) {
      throw Error("Invalid permission guide copy");
    }
    return copy;
  }
  return {
    title: text("title", 100),
    instructions: text("instructions", 500),
    dragLabel: text("dragLabel", 200),
    doneLabel: text("doneLabel", 100),
    grantedLabel: text("grantedLabel", 200),
    closeLabel: text("closeLabel", 100),
    ...(input.openSettingsLabel === undefined
      ? {}
      : { openSettingsLabel: text("openSettingsLabel", 100) }),
    ...(input.revealLabel === undefined
      ? {}
      : { revealLabel: text("revealLabel", 100) }),
  };
}

export function momoAppBundle(
  packaged: boolean,
  executable: string,
  resources: string,
): string | undefined {
  if (!packaged) return;
  try {
    const binary = realpathSync(executable);
    const macos = path.dirname(binary);
    const contents = path.dirname(macos);
    const bundle = path.dirname(contents);
    if (
      path.basename(binary) === "momo" &&
      path.basename(macos) === "MacOS" &&
      path.basename(contents) === "Contents" &&
      path.extname(bundle) === ".app" &&
      realpathSync(resources) === path.join(contents, "Resources") &&
      statSync(path.join(contents, "Info.plist")).isFile()
    )
      return bundle;
  } catch {
    return undefined;
  }
}
