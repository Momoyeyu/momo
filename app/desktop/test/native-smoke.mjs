import { createServer } from "node:http";
import { chromium, expect } from "@playwright/test";
import { spawn, execFileSync } from "node:child_process";
import { mkdtemp, mkdir, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import assert from "node:assert/strict";

const executable = process.env.MOMO_E2E_APP;
if (!executable) throw Error("Set MOMO_E2E_APP to a packaged executable");
const profile = await mkdtemp(path.join(tmpdir(), "momo-electron-smoke-"));
const evidence = path.resolve(".workbuddy/electron-smoke");
await mkdir(evidence, { recursive: true });
const provider = createServer(async (req, res) => {
  let raw = "";
  for await (const chunk of req) raw += chunk;
  const body = JSON.parse(raw || "{}");
  assert.equal(req.url, "/v1/chat/completions");
  if (body.stream) {
    res.writeHead(200, { "content-type": "text/event-stream" });
    res.end(
      "data: " +
        JSON.stringify({
          choices: [{ delta: { content: "桌面测试回复。" } }],
        }) +
        "\n\ndata: [DONE]\n\n",
    );
  } else {
    res.writeHead(200, { "content-type": "application/json" });
    res.end(
      JSON.stringify({
        choices: [
          { message: { role: "assistant", content: "桌面测试回复。" } },
        ],
      }),
    );
  }
});
await new Promise((resolve) => provider.listen(0, "127.0.0.1", resolve));
let child, browser, page, runtime;
const timings = [];
const performanceSamples = [];
async function sampleProcesses(scene) {
  const samples = [];
  for (let count = 0; count < 3; count++) {
    await new Promise((resolve) => setTimeout(resolve, 1000));
    const rows = execFileSync("ps", ["-axo", "pid=,ppid=,%cpu=,rss="], {
      encoding: "utf8",
    })
      .trim()
      .split("\n")
      .map((line) => line.trim().split(/\s+/).map(Number));
    const owned = new Set([child.pid]);
    for (let pass = 0; pass < 6; pass++)
      for (const [pid, parent] of rows) if (owned.has(parent)) owned.add(pid);
    const processes = rows.filter(([pid]) => owned.has(pid));
    samples.push({
      cpuPercent: processes.reduce((sum, row) => sum + row[2], 0),
      rssMiB: Math.round(
        processes.reduce((sum, row) => sum + row[3], 0) / 1024,
      ),
    });
  }
  performanceSamples.push({ scene, samples });
}

async function launch() {
  const started = performance.now();
  child = spawn(
    executable,
    ["--remote-debugging-address=127.0.0.1", "--remote-debugging-port=0"],
    {
      env: { ...process.env, MOMO_DESKTOP_DATA_DIR: profile },
      stdio: ["ignore", "pipe", "pipe"],
    },
  );
  const endpoint = await new Promise((resolve, reject) => {
    const timer = setTimeout(
      () => reject(Error("Native startup timeout")),
      60000,
    );
    let output = "";
    child.stderr.on("data", (chunk) => {
      output += chunk.toString();
      if (chunk.toString().includes("momo startup failed:"))
        console.error(chunk.toString());
      const match = output.match(
        /DevTools listening on (ws:\/\/127\.0\.0\.1:\d+\/\S+)/,
      );
      if (match) {
        clearTimeout(timer);
        resolve(match[1]);
      }
    });
    child.once("exit", (code) => {
      clearTimeout(timer);
      reject(Error(`Native process exited: ${code}`));
    });
  });
  const debugOrigin = new URL(endpoint).origin.replace("ws:", "http:");
  await expect
    .poll(
      async () => {
        const targets = await (await fetch(debugOrigin + "/json/list")).json();
        return targets.some(
          (target) =>
            target.type === "page" && target.url.startsWith("momo://app/"),
        );
      },
      { timeout: 60000 },
    )
    .toBe(true);
  browser = await chromium.connectOverCDP(endpoint);
  const downloads = await browser.newBrowserCDPSession();
  await downloads.send("Browser.setDownloadBehavior", {
    behavior: "allow",
    downloadPath: profile,
  });
  await downloads.detach();
  await expect
    .poll(
      () => browser.contexts().flatMap((context) => context.pages()).length,
      { timeout: 10000 },
    )
    .toBeGreaterThan(0);
  page = browser.contexts().flatMap((context) => context.pages())[0];
  page.setDefaultTimeout(15000);
  page.on("pageerror", (error) => console.error("Renderer:", error.message));
  await expect(page.locator(".title-screen")).toBeVisible({ timeout: 60000 });
  timings.push({ titleReadyMs: Math.round(performance.now() - started) });
  runtime = await page.evaluate(() => window.momoDesktop.getRuntime());
  assert.equal(await page.evaluate(() => typeof window.require), "undefined");
  return runtime;
}
async function api(route, init = {}) {
  return fetch(runtime.baseUrl + route, {
    ...init,
    headers: {
      "x-momo-token": runtime.token,
      Origin: "momo://app",
      "Content-Type": "application/json",
      ...init.headers,
    },
  });
}
async function quit() {
  if (!child) return;
  const process = child;
  const exit = new Promise((resolve) => process.once("exit", resolve));
  if (page && !page.isClosed()) await page.close();
  await Promise.race([
    exit,
    new Promise((_, reject) =>
      setTimeout(() => reject(Error("Native quit timed out")), 12000),
    ),
  ]);
  await browser?.close();
  child = undefined;
  await assert.rejects(
    fetch(runtime.baseUrl + "/api/health", {
      signal: AbortSignal.timeout(1500),
    }),
  );
}
try {
  await launch();
  assert.equal((await api("/api/health")).status, 200);
  assert.equal((await fetch(runtime.baseUrl + "/api/health")).status, 403);
  assert.equal(
    (
      await api("/api/health", {
        headers: { Origin: "https://untrusted.invalid" },
      })
    ).status,
    403,
  );
  const qq = await api("/api/social/accounts/qq/runtime");
  assert.equal(qq.status, 200);
  const qqStatus = await qq.json();
  assert.equal(qqStatus.running, false);
  assert.equal(qqStatus.connected, false);
  const intro = await (await api("/api/intro")).json();
  assert.equal(intro.phase, "setup");
  await expect(page.locator('.intro-guide[data-ready="true"]')).toBeVisible();
  await page.screenshot({ path: path.join(evidence, "fresh-intro.png") });
  await sampleProcesses("title");
  const second = spawn(executable, [], {
    env: { ...process.env, MOMO_DESKTOP_DATA_DIR: profile },
    stdio: "ignore",
  });
  assert.equal(
    await new Promise((resolve, reject) => {
      second.once("exit", resolve);
      setTimeout(() => {
        second.kill("SIGKILL");
        reject(Error("Second instance did not exit"));
      }, 10000).unref();
    }),
    0,
  );
  assert.equal(
    (await page.evaluate(() => window.momoDesktop.getRuntime())).baseUrl,
    runtime.baseUrl,
  );
  const state = await (await api("/api/state")).json();
  state.settings ??= {};
  state.settings.general ??= {};
  state.settings.general.musicVolume = 0;
  state.settings.general.sfxVolume = 0;
  state.settings.general.textSpeed = 100;
  state.settings.llm = {
    url: `http://127.0.0.1:${provider.address().port}`,
    model: "native-smoke",
    key: "synthetic",
    thinking: false,
  };
  assert.equal(
    (await api("/api/state", { method: "PUT", body: JSON.stringify(state) }))
      .status,
    204,
  );
  // Enable only the activity capability in this disposable profile. No capture or OS grant.
  assert.equal(
    (
      await api("/api/extensions/activities", {
        method: "PATCH",
        body: JSON.stringify({ id: "computer", enabled: true }),
      })
    ).status,
    200,
  );
  await quit();
  // Seed a completed intro in this disposable profile only; no production bypass endpoint.
  execFileSync("python3", [
    "-c",
    `import sqlite3,json,sys
c=sqlite3.connect(sys.argv[1])
for key,value in [('intro_v1',{'version':1,'phase':'completed','languageConfirmed':True}),('first_meeting',{'version':1,'stage':'completed'})]:
 c.execute('INSERT OR REPLACE INTO meta(key,value) VALUES(?,?)',(key,json.dumps(value)))
c.commit()`,
    path.join(profile, "data/momo.db"),
  ]);
  await launch();
  await page
    .locator(".menu-item")
    .filter({ hasText: /启动|Boot/ })
    .click();
  await expect(page.locator(".chat-page, .chat-screen")).toBeVisible({
    timeout: 30000,
  });
  await expect(page.locator("canvas").first()).toBeVisible();
  await expect(page.locator(".chat-page")).not.toHaveAttribute("inert", "", {
    timeout: 30000,
  });
  const meetingBeforePet = await page.evaluate(() =>
    sessionStorage.getItem("momo.meeting-owner.v1"),
  );
  assert.ok(JSON.parse(meetingBeforePet ?? "null")?.leaseId);
  const canvasBeforePet = await page.locator("canvas").first().elementHandle();
  assert.ok(canvasBeforePet);
  // Electron does not expose Chrome's Browser.getWindowForTarget CDP command.
  const windowBounds = () =>
    page.evaluate(() => ({
      x: screenX,
      y: screenY,
      width: outerWidth,
      height: outerHeight,
    }));
  const originalWindow = await windowBounds();
  await page.getByRole("button", { name: "进入桌面", exact: true }).click();
  await expect
    .poll(() => page.evaluate(() => window.momoDesktop.getWindowState()))
    .toEqual({ mode: "pet", clickThrough: false });
  await expect(page.locator("html")).toHaveClass(/desktop-pet/);
  await expect(page.locator(".chat-page")).toHaveClass(/pet-dialog-collapsed/);
  await expect(page.locator(".dialog-drawer")).toBeHidden();
  await page.getByRole("button", { name: "观察桌面", exact: true }).click();
  const observation = page.getByRole("dialog", {
    name: "观察桌面",
    exact: true,
  });
  await expect(observation).toBeVisible();
  await expect(
    observation.getByRole("button", { name: "开始观察", exact: true }),
  ).toBeDisabled();
  await observation
    .getByRole("button", { name: "关闭配置", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "展开对话框", exact: true }),
  ).toBeVisible();
  await page.screenshot({
    path: path.join(evidence, "desktop-pet-folded.png"),
    omitBackground: true,
  });
  await page.getByRole("button", { name: "展开对话框", exact: true }).click();
  await expect(page.locator(".chat-page")).not.toHaveClass(
    /pet-dialog-collapsed/,
  );
  const checkPetLayout = async () =>
    page.evaluate(() => {
      const box = document
        .querySelector(".dialog-box")
        ?.getBoundingClientRect();
      const transparent = [
        document.documentElement,
        document.body,
        document.querySelector("#root"),
        document.querySelector(".app-stage"),
      ].every(
        (element) =>
          element &&
          getComputedStyle(element).backgroundColor === "rgba(0, 0, 0, 0)",
      );
      return {
        transparent,
        fits:
          !!box &&
          box.width > 0 &&
          box.height > 0 &&
          box.left >= 0 &&
          box.top >= 0 &&
          box.right <= innerWidth + 1 &&
          box.bottom <= innerHeight + 1,
      };
    });
  await expect.poll(checkPetLayout).toEqual({ transparent: true, fits: true });
  assert.equal(
    await canvasBeforePet.evaluate(
      (canvas) =>
        canvas.isConnected && canvas === document.querySelector("canvas"),
    ),
    true,
  );
  assert.equal(
    await page.evaluate(() => sessionStorage.getItem("momo.meeting-owner.v1")),
    meetingBeforePet,
  );
  assert.equal(
    (await (await api("/api/computer/native/status")).json()).active,
    false,
  );
  await page.screenshot({
    path: path.join(evidence, "desktop-pet.png"),
    omitBackground: true,
  });
  await page.getByRole("button", { name: "返回 chat", exact: true }).click();
  await expect
    .poll(() => page.evaluate(() => window.momoDesktop.getWindowState()))
    .toEqual({ mode: "chat", clickThrough: false });
  await expect.poll(windowBounds).toEqual(originalWindow);
  assert.equal(
    await canvasBeforePet.evaluate(
      (canvas) =>
        canvas.isConnected && canvas === document.querySelector("canvas"),
    ),
    true,
  );
  assert.equal(
    await page.evaluate(() => sessionStorage.getItem("momo.meeting-owner.v1")),
    meetingBeforePet,
  );
  await canvasBeforePet.dispose();
  await page.locator(".input-row input").fill("native smoke");
  await expect(page.locator(".input-row input")).toHaveValue("native smoke");
  await expect(
    page.getByRole("button", { name: "发送", exact: true }),
  ).toBeEnabled();
  const chatResponse = page.waitForResponse(
    (response) => new URL(response.url()).pathname === "/api/chat",
  );
  await page.getByRole("button", { name: "发送", exact: true }).click();
  const reply = await chatResponse;
  assert.equal(reply.status(), 200);
  assert.match(await reply.text(), /桌面测试回复/);
  await expect(page.locator(".chat-page")).toContainText("桌面测试回复", {
    timeout: 30000,
  });
  await page.screenshot({ path: path.join(evidence, "chat.png") });
  await sampleProcesses("chat-live2d");
  await page.getByRole("button", { name: "设置", exact: true }).click();
  await expect(page.locator(".settings-page")).toBeVisible();
  await expect(page.locator(".app-stage")).not.toHaveClass(/tx-active/);
  await expect(
    page
      .locator(".settings-page")
      .locator("xpath=ancestor::*[contains(@class, 'page-wrap')]"),
  ).not.toHaveAttribute("inert", "");
  await page.screenshot({ path: path.join(evidence, "settings.png") });
  await page.evaluate(async () => {
    await new Promise((resolve, reject) => {
      const request = indexedDB.open("momo-web", 1);
      request.onupgradeneeded = () =>
        request.result.createObjectStore("avatars");
      request.onsuccess = () => {
        const db = request.result,
          tx = db.transaction("avatars", "readwrite");
        tx.objectStore("avatars").put(
          new Blob(["synthetic-avatar-bytes"]),
          "portable-smoke",
        );
        tx.oncomplete = () => {
          db.close();
          resolve();
        };
        tx.onerror = () => reject(tx.error);
      };
      request.onerror = () => reject(request.error);
    });
  });
  const savedMemory = await (
    await api("/api/memories", {
      method: "POST",
      body: JSON.stringify({ content: "native backup smoke" }),
    })
  ).json();
  await page.evaluate(() => {
    const create = URL.createObjectURL,
      click = HTMLAnchorElement.prototype.click;
    URL.createObjectURL = function (blob) {
      window.smokeExport = blob;
      return create.call(URL, blob);
    };
    HTMLAnchorElement.prototype.click = function () {
      if (this.download && this.href.startsWith("blob:")) {
        URL.createObjectURL = create;
        HTMLAnchorElement.prototype.click = click;
        return;
      }
      return click.call(this);
    };
  });
  await page.getByRole("button", { name: "导出备份", exact: true }).click();
  await expect
    .poll(() => page.evaluate(() => Boolean(window.smokeExport)))
    .toBe(true);
  const bytes = await page.evaluate(async () =>
    Array.from(new Uint8Array(await window.smokeExport.arrayBuffer())),
  );
  const backup = path.join(profile, "backup.zip");
  await writeFile(backup, Buffer.from(bytes));
  execFileSync("python3", [
    "-c",
    "import zipfile,sys; z=zipfile.ZipFile(sys.argv[1]); assert z.read('avatars/portable-smoke.zip')==b'synthetic-avatar-bytes'; assert z.read('momo.db').startswith(b'SQLite format 3')",
    backup,
  ]);
  await api("/api/memories/" + savedMemory.id, { method: "DELETE" });
  await page.evaluate(
    async () =>
      new Promise((resolve, reject) => {
        const request = indexedDB.open("momo-web", 1);
        request.onsuccess = () => {
          const db = request.result,
            tx = db.transaction("avatars", "readwrite");
          tx.objectStore("avatars").clear();
          tx.oncomplete = () => {
            db.close();
            resolve();
          };
          tx.onerror = () => reject(tx.error);
        };
      }),
  );
  page.once("dialog", (dialog) => void dialog.accept());
  const reloaded = page.waitForEvent("load", { timeout: 45000 });
  await page.locator('input[type=file][accept=".zip"]').setInputFiles(backup);
  try {
    await reloaded;
  } catch (error) {
    await page.screenshot({ path: path.join(evidence, "import-failure.png") });
    console.error("Backup import UI:", await page.locator("body").innerText());
    throw error;
  }
  await expect
    .poll(async () =>
      (await (await api("/api/memories")).json()).some(
        (row) => row.id === savedMemory.id,
      ),
    )
    .toBe(true);
  await expect
    .poll(() =>
      page.evaluate(
        async () =>
          new Promise((resolve, reject) => {
            const request = indexedDB.open("momo-web", 1);
            request.onsuccess = () => {
              const db = request.result,
                tx = db.transaction("avatars", "readonly"),
                blob = tx.objectStore("avatars").get("portable-smoke");
              blob.onsuccess = async () => {
                resolve(await blob.result?.text());
                db.close();
              };
              blob.onerror = () => reject(blob.error);
            };
          }),
      ),
    )
    .toBe("synthetic-avatar-bytes");
  const back = page.getByRole("button", { name: /^返回(对话|标题)$/ });
  await back.click();
  if (await page.locator(".chat-page").isVisible())
    await page.getByRole("button", { name: "返回标题", exact: true }).click();
  await expect(page.locator(".title-screen")).toBeVisible({ timeout: 30000 });
  await quit();
  await writeFile(
    path.join(evidence, "result.json"),
    JSON.stringify(
      {
        desktopPetSameRendererAndLease: true,
        freshIntro: true,
        isolatedRestart: true,
        auth: true,
        qqInstalledAndDormant: true,
        titleBootChatSettingsHome: true,
        exitReclaimsBackend: true,
        singleInstance: true,
        timings,
        backupRoundTrip: true,
        performanceSamples,
      },
      null,
      2,
    ),
  );
  console.log("PASS packaged native smoke");
} finally {
  if (child) {
    const owned = child;
    owned.kill("SIGTERM");
    await new Promise((resolve) => {
      owned.once("exit", resolve);
      setTimeout(() => {
        owned.kill("SIGKILL");
        resolve();
      }, 8000).unref();
    });
  }
  await browser?.close().catch(() => {});
  provider.closeAllConnections();
  await new Promise((resolve) => provider.close(resolve));
  await rm(profile, { recursive: true, force: true });
}
