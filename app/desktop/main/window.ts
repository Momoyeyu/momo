import {
  BrowserWindow,
  desktopCapturer,
  dialog,
  shell,
  systemPreferences,
} from "electron";
import path from "node:path";
import { trustedFrame, mediaPermission } from "./security.ts";

import type { DialogTexts } from "./dialog-locale.ts";

export function createWindow(getTexts: () => Promise<DialogTexts>) {
  const window = new BrowserWindow({
    width: 1280,
    height: 800,
    minWidth: 960,
    minHeight: 540,
    show: false,
    transparent: true,
    titleBarStyle: "hidden",
    backgroundColor: "#18151f",
    title: "momo",
    webPreferences: {
      preload: path.join(__dirname, "../preload/index.js"),
      contextIsolation: true,
      sandbox: true,
      nodeIntegration: false,
      webSecurity: true,
    },
  });
  const trusted = (url: string) =>
    trustedFrame(window.webContents.id, window.webContents.id, url, true);
  window.webContents.on("will-navigate", (event, url) => {
    if (!trusted(url)) event.preventDefault();
  });
  window.webContents.on("will-attach-webview", (event) =>
    event.preventDefault(),
  );
  window.webContents.setWindowOpenHandler(({ url }) => {
    if (/^https:\/\//.test(url))
      void getTexts()
        .then((texts) =>
          dialog.showMessageBox(window, {
            type: "question",
            message: new URL(url).hostname,
            detail: url,
            buttons: [texts.cancel, texts.open],
            defaultId: 0,
            cancelId: 0,
          }),
        )
        .then((result) => {
          if (result.response === 1) void shell.openExternal(url);
        });
    return { action: "deny" };
  });
  const session = window.webContents.session;
  session.setPermissionCheckHandler((contents, permission, origin, details) =>
    Boolean(
      mediaPermission(
        permission,
        contents === window.webContents &&
          trusted(origin) &&
          details.isMainFrame !== false,
        "mediaType" in details && details.mediaType ? [details.mediaType] : [],
      ),
    ),
  );
  session.setPermissionRequestHandler(
    (contents, permission, callback, details) => {
      const kind = mediaPermission(
        permission,
        contents === window.webContents &&
          details.isMainFrame &&
          trusted(details.requestingUrl),
        "mediaTypes" in details ? (details.mediaTypes ?? []) : [],
      );
      if (!kind) {
        callback(false);
        return;
      }
      if (kind === "display") {
        callback(true);
        return;
      }
      if (process.platform === "darwin")
        void systemPreferences
          .askForMediaAccess("microphone")
          .then(callback)
          .catch(() => callback(false));
      else callback(true);
    },
  );
  session.setDisplayMediaRequestHandler(
    (request, callback) => {
      if (
        request.frame !== window.webContents.mainFrame ||
        !trusted(request.securityOrigin) ||
        !request.userGesture ||
        !request.videoRequested ||
        request.audioRequested
      ) {
        callback(null);
        return;
      }
      void desktopCapturer
        .getSources({ types: ["screen", "window"] })
        .then(async (sources) => {
          if (!sources.length) {
            callback(null);
            return;
          }
          const texts = await getTexts();
          const selection = await dialog.showMessageBox(window, {
            type: "question",
            message: texts.share,
            buttons: [texts.cancel, ...sources.map((source) => source.name)],
            defaultId: 0,
            cancelId: 0,
            noLink: true,
          });
          if (
            window.isDestroyed() ||
            request.frame !== window.webContents.mainFrame ||
            !trusted(window.webContents.getURL())
          ) {
            callback(null);
            return;
          }
          const source = sources[selection.response - 1];
          callback(source ? { video: source } : null);
        })
        .catch(() => callback(null));
    },
    { useSystemPicker: true },
  );
  window.once("ready-to-show", () => window.show());
  return window;
}
