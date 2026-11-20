import { app, BrowserWindow, type IpcMainEvent, screen, shell } from "electron";
import path from "node:path";
import { pathToFileURL } from "node:url";
import {
  momoAppBundle,
  permissionGuideCopy,
  permissionSettingsUrl,
} from "./permission-guide-config.ts";

export function createPermissionGuide(owner: BrowserWindow) {
  let active: { window: BrowserWindow; granted: boolean } | undefined;

  function close() {
    const guide = active;
    active = undefined;
    guide?.window.destroy();
  }

  owner.once("closed", close);
  owner.webContents.on("render-process-gone", close);
  owner.webContents.on("did-start-navigation", (details) => {
    if (details.isMainFrame && !details.isSameDocument) close();
  });

  return {
    close,
    setGranted(value: unknown) {
      if (typeof value !== "boolean") throw Error("Invalid permission status");
      if (!active || active.granted === value) return;
      active.granted = value;
      active.window.webContents.send("momo:permission-guide:status", value);
    },
    async open(kind: unknown, value: unknown): Promise<void> {
      if (process.platform !== "darwin") {
        throw Error("System permission setup is unavailable");
      }
      const settingsUrl = permissionSettingsUrl(kind);
      const copy = permissionGuideCopy(value);
      close();

      const width = 340;
      const height = 222;
      const area = screen.getDisplayMatching(owner.getBounds()).workArea;
      const guide = new BrowserWindow({
        width,
        height,
        x: Math.max(area.x, area.x + area.width - width - 24),
        y: Math.max(area.y, area.y + area.height - height - 24),
        show: false,
        frame: false,
        backgroundColor: "#302639",
        title: copy.title,
        alwaysOnTop: true,
        acceptFirstMouse: true,
        minimizable: false,
        maximizable: false,
        fullscreenable: false,
        resizable: false,
        skipTaskbar: true,
        webPreferences: {
          preload: path.join(__dirname, "../preload/permission-guide.js"),
          partition: "momo-permission-guide",
          contextIsolation: true,
          sandbox: true,
          nodeIntegration: false,
        },
      });
      // Follow a change of desktop without changing the owning app's Dock state.
      guide.setVisibleOnAllWorkspaces(true, { skipTransformProcessType: true });
      const session = { window: guide, granted: false };
      active = session;
      guide.on("closed", () => {
        if (active !== session) return;
        active = undefined;
        if (owner.isDestroyed()) return;
        owner.webContents.send("momo:permission-guide-closed");
        if (owner.isMinimized()) owner.restore();
        owner.show();
        owner.focus();
      });

      const contents = guide.webContents;
      const document = path.join(
        app.getAppPath(),
        "desktop/permission-guide/index.html",
      );
      const documentUrl = pathToFileURL(document).href;
      contents.on("will-frame-navigate", (event) => event.preventDefault());
      contents.setWindowOpenHandler(() => ({ action: "deny" }));
      contents.session.setPermissionCheckHandler(() => false);
      contents.session.setPermissionRequestHandler(
        (_contents, _permission, respond) => respond(false),
      );
      contents.once("render-process-gone", () => guide.destroy());

      const bundle = momoAppBundle(
        app.isPackaged,
        process.execPath,
        process.resourcesPath,
      );
      const listen = (channel: string, action: () => void) =>
        contents.ipc.on(channel, (event: IpcMainEvent, ...args: unknown[]) => {
          if (
            active === session &&
            args.length === 0 &&
            event.sender === contents &&
            event.senderFrame === contents.mainFrame &&
            event.senderFrame.url === documentUrl
          )
            action();
        });

      try {
        const [icon] = await Promise.all([
          bundle
            ? app.getFileIcon(bundle, { size: "normal" }).catch(() => undefined)
            : undefined,
          guide.loadFile(document),
        ]);
        if (active !== session) return;
        const canDrag = Boolean(bundle && icon && !icon.isEmpty());
        listen("momo:permission-guide:drag", () => {
          if (bundle && icon && canDrag && !session.granted) {
            contents.startDrag({ file: bundle, icon });
          }
        });
        listen("momo:permission-guide:reveal", () => {
          if (bundle && !session.granted) shell.showItemInFolder(bundle);
        });
        listen("momo:permission-guide:open-settings", () => {
          if (!session.granted) {
            void shell
              .openExternal(settingsUrl)
              .catch(() => console.warn("System Settings unavailable"));
          }
        });
        listen("momo:permission-guide:close", () => guide.destroy());
        contents.send("momo:permission-guide:init", {
          copy,
          canDrag,
          hasBundle: Boolean(bundle),
          icon: canDrag ? icon?.toDataURL() : undefined,
          granted: session.granted,
        });
        guide.showInactive();
        await shell.openExternal(settingsUrl);
      } catch (error) {
        if (active !== session) return;
        close();
        throw error;
      }
    },
  };
}
