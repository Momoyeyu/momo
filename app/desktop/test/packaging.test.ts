import { test } from "node:test";
import assert from "node:assert/strict";
import {
  mkdtemp,
  mkdir,
  writeFile,
  symlink,
  readlink,
  readFile,
  rm,
} from "node:fs/promises";
import path from "node:path";
import { tmpdir } from "node:os";
const {
  normalizeResourceLinks,
} = require("../../desktop/packaging/resources.cjs");
test("packaged libraries remain readable after the build inputs are removed", async () => {
  const temp = await mkdtemp(path.join(tmpdir(), "momo-links-"));
  const source = path.join(temp, "source"),
    target = path.join(temp, "installed");
  try {
    for (const root of [source, target]) {
      await mkdir(path.join(root, "backend/PIL"), { recursive: true });
      await writeFile(path.join(root, "backend/PIL/lib.dylib"), "library");
    }
    await symlink(
      path.join(source, "backend/PIL/lib.dylib"),
      path.join(target, "backend/lib.dylib"),
    );
    await normalizeResourceLinks(source, target);
    assert.equal(
      await readlink(path.join(target, "backend/lib.dylib")),
      "PIL/lib.dylib",
    );
    await rm(source, { recursive: true });
    assert.equal(
      await readFile(path.join(target, "backend/lib.dylib"), "utf8"),
      "library",
    );
  } finally {
    await rm(temp, { recursive: true, force: true });
  }
});
test("packaging refuses a resource link escaping its resource closure", async () => {
  const temp = await mkdtemp(path.join(tmpdir(), "momo-links-"));
  try {
    await mkdir(path.join(temp, "target/backend"), { recursive: true });
    await symlink(
      "/usr/lib/libSystem.B.dylib",
      path.join(temp, "target/backend/external"),
    );
    await assert.rejects(
      normalizeResourceLinks(
        path.join(temp, "source"),
        path.join(temp, "target"),
      ),
      /outside/,
    );
  } finally {
    await rm(temp, { recursive: true, force: true });
  }
});

test("QQ framework link chains relocate before final validation", async () => {
  const temp = await mkdtemp(path.join(tmpdir(), "momo-links-"));
  const source = path.join(temp, "source"),
    target = path.join(temp, "target");
  try {
    for (const root of [source, target]) {
      await mkdir(path.join(root, "qq/Versions/A"), { recursive: true });
      await writeFile(path.join(root, "qq/Versions/A/library"), "QQ library");
    }
    await symlink(
      path.join(source, "qq/Versions/Current/library"),
      path.join(target, "qq/Library"),
    );
    await symlink(
      path.join(source, "qq/Versions/A"),
      path.join(target, "qq/Versions/Current"),
    );
    await normalizeResourceLinks(source, target);
    await rm(source, { recursive: true });
    assert.equal(
      await readFile(path.join(target, "qq/Library"), "utf8"),
      "QQ library",
    );
  } finally {
    await rm(temp, { recursive: true, force: true });
  }
});
