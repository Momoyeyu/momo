import { test } from "node:test";
import assert from "node:assert/strict";
import { BackendProcess } from "../main/backend.ts";

function fixture(mode: string) {
  const script = `
    const mode=${JSON.stringify(mode)};
    if(mode==='hang') process.on('SIGTERM',()=>{});
    if(mode==='exit') process.exit(9);
    if(mode==='dependency') {console.error('Library not loaded: private-path token='+process.env.MOMO_DESKTOP_TOKEN);process.exit(1);}
    const ready={type:'momo-ready', nonce:process.env.MOMO_DESKTOP_NONCE,pid:process.pid,port:49152};
    if(mode==='forged') ready.nonce='wrong';
    if(mode!=='timeout') console.log(JSON.stringify(ready));
    if(mode==='dies') setTimeout(()=>process.exit(8),50);
    process.stdin.resume();
    process.stdin.on('end',()=>{if(mode!=='hang')process.exit(0)});
    setInterval(()=>{},1000);
  `;
  return new BackendProcess({
    command: process.execPath,
    args: ["-e", script, "--"],
    dataDir: "/tmp",
    resourcesDir: "/tmp",
    startupTimeoutMs: 150,
    shutdownTimeoutMs: 100,
  });
}
test("concurrent runtime requests share one owned backend and stop reclaims it", async () => {
  const backend = fixture("ready");
  try {
    const [a, b] = await Promise.all([backend.start(), backend.start()]);
    assert.equal(a, b);
    assert.equal(a.baseUrl, "http://127.0.0.1:49152");
    assert.ok(a.token.length >= 32);
    const pid = backend.pid!;
    await backend.stop();
    assert.throws(() => process.kill(pid, 0));
  } finally {
    await backend.stop();
  }
});
for (const mode of ["forged", "timeout", "exit"])
  test(`rejects ${mode} readiness and reclaims the child`, async () => {
    const backend = fixture(mode);
    await assert.rejects(backend.start(), /Backend/);
    assert.equal(backend.pid, undefined);
  });
test("hung child is killed within the bounded shutdown deadline", async () => {
  const backend = fixture("hang");
  await backend.start();
  const pid = backend.pid!;
  const start = Date.now();
  await backend.stop();
  assert.ok(Date.now() - start < 2000);
  assert.throws(() => process.kill(pid, 0));
});
test("stopping an idle instance does not prevent cleanup after a later start", async () => {
  const backend = fixture("ready");
  await backend.stop();
  await backend.start();
  const pid = backend.pid!;
  try {
    await backend.stop();
    assert.throws(() => process.kill(pid, 0));
  } finally {
    try {
      process.kill(-pid, "SIGKILL");
    } catch {}
  }
});

test("startup diagnostics identify missing dependencies without leaking stderr", async () => {
  await assert.rejects(fixture("dependency").start(), {
    message: "Backend dependency missing; reinstall the application.",
  });
});

test("an unexpected exit never respawns on a renderer retry", async () => {
  const backend = fixture("dies");
  await backend.start();
  await new Promise((resolve) => setTimeout(resolve, 150));
  await assert.rejects(backend.start(), /restart/);
  assert.equal(backend.pid, undefined);
});
