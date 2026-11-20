import { ipcRenderer } from "electron";
import type { PermissionGuideCopy } from "../contracts.ts";

interface GuideView {
  copy: PermissionGuideCopy;
  canDrag: boolean;
  hasBundle: boolean;
  icon?: string;
  granted: boolean;
}

window.addEventListener("DOMContentLoaded", () => {
  const title = document.querySelector<HTMLHeadingElement>("#title")!;
  const instructions =
    document.querySelector<HTMLParagraphElement>("#instructions")!;
  const tile = document.querySelector<HTMLButtonElement>("#tile")!;
  const tileLabel = document.querySelector<HTMLSpanElement>("#tile-label")!;
  const icon = document.querySelector<HTMLImageElement>("#icon")!;
  const close = document.querySelector<HTMLButtonElement>("#close")!;
  const done = document.querySelector<HTMLButtonElement>("#done")!;
  const settings = document.querySelector<HTMLButtonElement>("#settings")!;
  const reveal = document.querySelector<HTMLButtonElement>("#reveal")!;
  const pending = document.querySelector<HTMLElement>("#pending")!;
  const result = document.querySelector<HTMLElement>("#result")!;
  const status = document.querySelector<HTMLParagraphElement>("#status")!;
  let dragged = false;

  const displayStatus = (granted: boolean) => {
    pending.hidden = granted;
    result.hidden = !granted;
  };
  ipcRenderer.once("momo:permission-guide:init", (_event, view: GuideView) => {
    const { copy } = view;
    title.textContent = copy.title;
    instructions.textContent = copy.instructions;
    tileLabel.textContent = copy.dragLabel;
    close.setAttribute("aria-label", copy.closeLabel);
    close.title = copy.closeLabel;
    done.textContent = copy.doneLabel;
    status.textContent = copy.grantedLabel;
    settings.textContent = copy.openSettingsLabel ?? "";
    settings.hidden = !copy.openSettingsLabel;
    reveal.textContent = copy.revealLabel ?? "";
    reveal.hidden = !view.hasBundle || !copy.revealLabel;
    if (view.icon) icon.src = view.icon;
    tile.hidden = !view.canDrag;
    if (copy.revealLabel) {
      tile.title = copy.revealLabel;
      tile.setAttribute("aria-label", copy.dragLabel + ". " + copy.revealLabel);
    }
    displayStatus(view.granted);
    document.body.hidden = false;
  });
  ipcRenderer.on("momo:permission-guide:status", (_event, granted: boolean) => {
    displayStatus(granted);
  });

  tile.addEventListener("pointerdown", () => {
    dragged = false;
  });
  tile.addEventListener("dragstart", (event) => {
    event.preventDefault();
    if (!event.isTrusted || tile.hidden) return;
    dragged = true;
    ipcRenderer.send("momo:permission-guide:drag");
  });
  tile.addEventListener("click", (event) => {
    if (event.isTrusted && (!dragged || event.detail === 0)) {
      ipcRenderer.send("momo:permission-guide:reveal");
    }
  });
  const click = (button: HTMLButtonElement, action: string) =>
    button.addEventListener("click", (event) => {
      if (event.isTrusted) ipcRenderer.send(`momo:permission-guide:${action}`);
    });
  click(reveal, "reveal");
  click(settings, "open-settings");
  click(close, "close");
  click(done, "close");
  window.addEventListener("keydown", (event) => {
    if (event.isTrusted && event.key === "Escape") {
      ipcRenderer.send("momo:permission-guide:close");
    }
  });
});
