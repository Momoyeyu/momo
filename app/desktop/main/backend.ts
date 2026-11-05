import { spawn, type ChildProcessWithoutNullStreams } from "node:child_process";
import { randomBytes } from "node:crypto";
import type { RuntimeConnection } from "../contracts.ts";

export interface BackendOptions {
  command: string;
  args?: string[];
  dataDir: string;
  resourcesDir: string;
  startupTimeoutMs?: number;
  shutdownTimeoutMs?: number;
  onFailure?: () => void;
}

export class BackendProcess {
  private child?: ChildProcessWithoutNullStreams;
  private pending?: Promise<RuntimeConnection>;
  private stopping?: Promise<void>;
  private exited?: Promise<void>;
  private ready = false;
  private failed = false;
  constructor(private readonly options: BackendOptions) {}
  get pid(): number | undefined {
    return this.child?.pid;
  }

  start(): Promise<RuntimeConnection> {
    if (this.failed)
      return Promise.reject(
        new Error("Backend stopped; restart momo to reconnect."),
      );
    this.pending ??= this.launch().catch(async (error) => {
      await this.stop();
      throw error;
    });
    return this.pending;
  }

  private launch(): Promise<RuntimeConnection> {
    const token = randomBytes(32).toString("hex");
    const nonce = randomBytes(32).toString("hex");
    const env = { ...process.env };
    // Parent development configuration must never redirect installed runtime resources or credentials.
    for (const key of Object.keys(env))
      if (
        key.startsWith("MOMO_") ||
        key.startsWith("PYTHON") ||
        key === "NODE_OPTIONS"
      )
        delete env[key];
    const child = spawn(
      this.options.command,
      [
        ...(this.options.args ?? []),
        "--data-dir",
        this.options.dataDir,
        "--resources-dir",
        this.options.resourcesDir,
        "--port",
        "0",
      ],
      {
        env: { ...env, MOMO_DESKTOP_TOKEN: token, MOMO_DESKTOP_NONCE: nonce },
        detached: process.platform !== "win32",
        stdio: ["pipe", "pipe", "pipe"],
        windowsHide: true,
      },
    );
    this.child = child;
    this.exited = new Promise((resolve) => {
      child.once("exit", () => resolve());
      child.once("error", () => resolve());
    });
    child.stdin.on("error", () => {});
    let diagnostic = "";
    child.stderr.setEncoding("utf8");
    child.stderr.on("data", (chunk: string) => {
      diagnostic = (diagnostic + chunk).slice(-8192);
    });
    return new Promise((resolve, reject) => {
      const timer = setTimeout(
        () => reject(new Error("Backend startup timed out")),
        this.options.startupTimeoutMs ?? 55000,
      );
      let buffer = "";
      child.stdout.setEncoding("utf8");
      child.stdout.on("data", (chunk: string) => {
        buffer += chunk;
        if (buffer.length > 65536) {
          clearTimeout(timer);
          reject(new Error("Backend readiness exceeded limit"));
          return;
        }
        let end: number;
        while ((end = buffer.indexOf("\n")) >= 0) {
          const line = buffer.slice(0, end);
          buffer = buffer.slice(end + 1);
          let value;
          try {
            value = JSON.parse(line);
          } catch {
            continue;
          }
          if (value?.type !== "momo-ready" || this.ready) continue;
          clearTimeout(timer);
          if (
            value.nonce !== nonce ||
            value.pid !== child.pid ||
            !Number.isInteger(value.port) ||
            value.port < 1 ||
            value.port > 65535
          ) {
            reject(new Error("Backend readiness verification failed"));
            return;
          }
          this.ready = true;
          resolve({ baseUrl: `http://127.0.0.1:${value.port}`, token });
        }
      });
      child.once("error", () => {
        clearTimeout(timer);
        reject(new Error("Backend could not start"));
      });
      child.once("exit", () => {
        clearTimeout(timer);
        const reason = /Library not loaded|No module named|dlopen\(/.test(
          diagnostic,
        )
          ? "Backend dependency missing; reinstall the application."
          : /already in use|another process/.test(diagnostic)
            ? "Backend data directory is in use; close its other owner."
            : /QQ resource qualification/.test(diagnostic)
              ? "Backend QQ resources failed validation; reinstall the application."
              : "Backend exited before readiness";
        reject(new Error(reason));
        if (this.ready && !this.stopping) {
          this.failed = true;
          void this.stop().then(() => this.options.onFailure?.());
        }
      });
    });
  }

  stop(): Promise<void> {
    this.stopping ??= this.reap().finally(() => {
      this.stopping = undefined;
    });
    return this.stopping;
  }

  private async reap(): Promise<void> {
    const child = this.child;
    if (!child) return;
    child.stdin.end();
    let timer: ReturnType<typeof setTimeout> | undefined;
    await Promise.race([
      this.exited,
      new Promise<void>((resolve) => {
        timer = setTimeout(resolve, this.options.shutdownTimeoutMs ?? 6500);
      }),
    ]);
    clearTimeout(timer);
    if (child.pid) {
      try {
        if (process.platform === "win32") child.kill("SIGKILL");
        else process.kill(-child.pid, "SIGKILL");
      } catch (error) {
        if ((error as NodeJS.ErrnoException).code !== "ESRCH") throw error;
      }
    }
    await this.exited;
    this.child = undefined;
    this.pending = undefined;
    this.ready = false;
  }
}
