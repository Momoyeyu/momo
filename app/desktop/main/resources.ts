import { existsSync } from "node:fs";
import path from "node:path";

export function resourcePaths(root: string) {
  const backend = path.join(
    root,
    "backend",
    process.platform === "win32" ? "momo-backend.exe" : "momo-backend",
  );
  const models = path.join(root, "models");
  const node = path.join(
    root,
    "node",
    process.platform === "win32" ? "node.exe" : "bin/node",
  );
  const required = [
    backend,
    node,
    path.join(root, "plugin-host/src/index.mjs"),
    path.join(
      root,
      "plugin-host/resources/minecraft/momo-fabric-companion-0.1.0.jar",
    ),
    ...(process.platform === "darwin"
      ? [
          path.join(root, "helpers/momo-native-control"),
          path.join(root, "qq/manifest.json"),
        ]
      : []),
    path.join(models, "resource-manifest.json"),
    path.join(models, "live2d/live2dcubismcore.min.js"),
  ];
  if (required.some((file) => !existsSync(file)))
    throw new Error(
      "Bundled runtime resources are incomplete. Reinstall this candidate.",
    );
  return { backend, models };
}
