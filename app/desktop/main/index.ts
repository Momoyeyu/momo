import { app, BrowserWindow, dialog, ipcMain, Menu, protocol } from "electron";
import { createDialogTexts } from "./dialog-locale.ts";
import type { RuntimeConnection } from "../contracts.ts";
import path from "node:path";
import { BackendProcess } from "./backend.ts";
import { resourcePaths } from "./resources.ts";
import { createWindow } from "./window.ts";
import { registerProtocol } from "./protocol.ts";
import { trustedFrame } from "./security.ts";

protocol.registerSchemesAsPrivileged([
  {
    scheme: "momo",
    privileges: {
      standard: true,
      secure: true,
      supportFetchAPI: true,
      stream: true,
      corsEnabled: true,
    },
  },
]);
const dataRoot = process.env.MOMO_DESKTOP_DATA_DIR
  ? path.resolve(process.env.MOMO_DESKTOP_DATA_DIR)
  : path.join(
      app.getPath("appData"),
      app.isPackaged ? "momo" : "momo-development",
    );
app.setPath("userData", dataRoot);
let backend: BackendProcess | undefined;
let window: BrowserWindow | undefined;
let quitting = false;
let endpoint = "";
let connection: RuntimeConnection | undefined;
let getTexts = createDialogTexts("en", () => connection);

if (!app.requestSingleInstanceLock()) app.quit();
else {
  app.on("second-instance", () => {
    if (window?.isMinimized()) window.restore();
    window?.show();
    window?.focus();
  });
  app.on("window-all-closed", () => app.quit());
  app.on("before-quit", (event) => {
    if (quitting || !backend) return;
    event.preventDefault();
    quitting = true;
    void backend.stop().finally(() => app.quit());
  });
  void app
    .whenReady()
    .then(async () => {
      getTexts = createDialogTexts(app.getLocale(), () => connection);
      Menu.setApplicationMenu(
        Menu.buildFromTemplate([
          { role: "appMenu" },
          { role: "editMenu" },
          { role: "windowMenu" },
        ]),
      );
      const root = app.isPackaged
        ? process.resourcesPath
        : path.join(app.getAppPath(), "resources");
      window = createWindow(getTexts);
      registerProtocol(
        path.join(app.getAppPath(), "frontend/dist"),
        () => endpoint,
      );
      ipcMain.handle("momo:get-runtime", async (event, ...args) => {
        if (
          !window ||
          args.length ||
          !trustedFrame(
            event.sender.id,
            window.webContents.id,
            event.senderFrame?.url ?? "",
            event.senderFrame === window.webContents.mainFrame,
          )
        )
          throw new Error("Runtime access denied");
        if (!backend) throw new Error("Backend unavailable");
        return backend.start();
      });
      while (!quitting) {
        try {
          const resources = resourcePaths(root);
          backend = new BackendProcess({
            command: resources.backend,
            dataDir: path.join(dataRoot, "data"),
            resourcesDir: resources.models,
            onFailure: () => {
              if (!quitting && window)
                void getTexts()
                  .then((texts) =>
                    dialog.showMessageBox(window!, {
                      type: "error",
                      message: texts.stopped,
                      detail: texts.reconnect,
                      buttons: [texts.quit],
                    }),
                  )
                  .then(() => app.quit());
            },
          });
          const runtime = await backend.start();
          endpoint = runtime.baseUrl;
          connection = runtime;
          await getTexts();
          await window.loadURL("momo://app/index.html");
          return;
        } catch (error) {
          console.error(
            "momo startup failed:",
            error instanceof Error ? error.message : "Unknown startup error",
          );
          await backend?.stop();
          backend = undefined;
          connection = undefined;
          const texts = await getTexts();
          const result = await dialog.showMessageBox({
            type: "error",
            message: texts.failed,
            detail: texts.help,
            buttons: [texts.retry, texts.quit],
            defaultId: 0,
            cancelId: 1,
          });
          if (result.response !== 0) {
            app.quit();
            return;
          }
        }
      }
    })
    .catch(async (error) => {
      console.error(
        "momo initialization failed:",
        error instanceof Error ? error.message : "Unknown initialization error",
      );
      const texts = await getTexts();
      await dialog.showMessageBox({
        type: "error",
        message: texts.failed,
        detail: texts.help,
        buttons: [texts.quit],
      });
      app.quit();
    });
}
