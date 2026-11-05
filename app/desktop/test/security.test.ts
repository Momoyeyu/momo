import { mediaPermission } from "../main/security.ts";
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  trustedFrame,
  assetPath,
  contentSecurityPolicy,
} from "../main/security.ts";

test("only the owned main frame at the exact app origin receives runtime credentials", () => {
  assert.equal(trustedFrame(1, 1, "momo://app/index.html", true), true);
  for (const [sender, url, main] of [
    [2, "momo://app/index.html", true],
    [1, "momo://app/index.html", false],
    [1, "momo://evil/index.html", true],
    [1, "https://app/", true],
    [1, "momo://app.evil/", true],
    [1, "momo://user@app/", true],
  ] as const)
    assert.equal(trustedFrame(sender, 1, url, main), false);
});
test("asset resolution stays inside renderer root and rejects encoded traversal", () => {
  assert.equal(
    assetPath("/bundle", "momo://app/assets/main.js"),
    "/bundle/assets/main.js",
  );
  assert.equal(assetPath("/bundle", "momo://app/"), "/bundle/index.html");
  for (const url of [
    "momo://evil/index.html",
    "momo://app/%2e%2e%2fsecret",
    "momo://app/%00secret",
    "momo://app/%5c..%5csecret",
  ])
    assert.throws(() => assetPath("/bundle", url));
});
test("CSP grants only the owned loopback endpoint, not arbitrary local services", () => {
  const csp = contentSecurityPolicy("http://127.0.0.1:49152");
  assert.match(csp, /http:\/\/127\.0\.0\.1:49152/);
  assert.match(csp, /ws:\/\/127\.0\.0\.1:49152/);
  assert.doesNotMatch(csp, /127\.0\.0\.1:\*/);
  assert.match(csp, /object-src 'none'/);
});

test("screen selection is available only to the trusted main frame", () => {
  assert.equal(mediaPermission("display-capture", true, []), "display");
  assert.equal(mediaPermission("display-capture", false, []), undefined);
  assert.equal(mediaPermission("media", true, ["audio"]), "microphone");
  assert.equal(mediaPermission("media", true, ["audio", "video"]), undefined);
  assert.equal(mediaPermission("media", true, []), undefined);
});
