// 配置页（/config，issue #56）：
// - GET /api/config?profile= 读 `.env.<profile>` 当前值（敏感键服务端已掩码）；
// - 编辑收集 diff → PUT /api/config（Bearer + X-CSRF-Token 由 app.js api() 统一处理）；
//   敏感键不回显——输入框留空 = 不变更，不会进 changes；
// - 每次 PUT 服务端先 `.env.<profile>.bak` 备份再原子写回；
//   rollback 按钮 → POST /api/config/rollback 从最新 .bak 恢复；
// - 成功 banner：restart_hint + 掩码 diff + /components 跳转链接；
// - 422 `error.fields` 逐字段回显（ApiError.fields 由 app.js 透传）。
// schema 驱动的控件（type/values/component 分组）由 #81 接入。

import { api } from "../app.js";

// ---------- 页面骨架 ----------

export default async function render(root) {
  root.innerHTML = `
    <h1>config</h1>
    <div class="toolbar">
      <label>profile
        <select id="cfg-profile">
          <option value="normal">normal</option>
          <option value="degraded">degraded</option>
        </select>
      </label>
      <span id="cfg-active" class="muted"></span>
      <button id="cfg-reload">reload</button>
      <button id="cfg-rollback">rollback .bak</button>
    </div>
    <div id="cfg-banner" class="hidden"></div>
    <form id="cfg-form"><div id="cfg-rows">loading…</div></form>`;

  const el = (id) => root.querySelector(`#${id}`);
  const profileSel = el("cfg-profile");

  el("cfg-reload").onclick = (e) => {
    e.preventDefault();
    load();
  };
  el("cfg-rollback").onclick = (e) => {
    e.preventDefault();
    rollback();
  };
  profileSel.onchange = () => load();
  el("cfg-form").onsubmit = (e) => {
    e.preventDefault();
    save();
  };

  async function load() {
    banner(null);
    el("cfg-rows").textContent = "loading…";
    try {
      const body = await api(`/api/config?profile=${profileSel.value}`);
      el("cfg-active").textContent =
        `active: ${body.active_profile}` +
        (body.file_exists
          ? ""
          : `  (file missing — run \`momoctl switch --to ${body.profile}\` first)`);
      profileSel.value = body.profile;
      renderRows(el("cfg-rows"), body.entries);
    } catch (e) {
      fail(el("cfg-rows"), e);
    }
  }

  function renderRows(host, entries) {
    host.innerHTML = "";
    const table = document.createElement("table");
    table.className = "cfg-table";
    for (const e of entries) {
      const tr = document.createElement("tr");
      tr.dataset.key = e.key;
      const label = document.createElement("td");
      label.innerHTML =
        `<code>${e.key}</code>` +
        (e.sensitive ? ' <span class="tag">sensitive</span>' : "") +
        (e.key.startsWith("MOMO_")
          ? ""
          : ' <span class="tag">passthrough</span>');
      const cell = document.createElement("td");
      if (!e.key.startsWith("MOMO_")) {
        // passthrough/游离键：只读展示，PUT 只收 MOMO_ schema 键
        cell.innerHTML = `<input value="${esc(e.value ?? "")}" disabled />`;
      } else if (e.sensitive) {
        // 敏感键：不回显——留空=不变更；placeholder 显示掩码/set 状态
        const ph = e.set ? `set (${e.value})` : "unset";
        cell.innerHTML = `<input name="${e.key}" type="password" data-orig="" placeholder="${esc(ph)}" autocomplete="off" />`;
      } else {
        cell.innerHTML = `<input name="${e.key}" value="${esc(e.value ?? "")}" data-orig="${esc(e.value ?? "")}" />`;
      }
      const err = document.createElement("td");
      err.className = "field-err";
      tr.append(label, cell, err);
      table.appendChild(tr);
    }
    host.appendChild(table);
    const submit = document.createElement("button");
    submit.type = "submit";
    submit.textContent = "save changes";
    submit.className = "primary";
    host.appendChild(submit);
  }

  async function save() {
    const changes = {};
    for (const input of root.querySelectorAll("#cfg-rows input[name]")) {
      // 敏感键留空 = 不变更；非敏感键与渲染时原值相等也不进 changes
      if (input.type === "password" && input.value === "") continue;
      if (input.value === (input.dataset.orig ?? "")) continue;
      changes[input.name] = input.value;
    }
    if (Object.keys(changes).length === 0) {
      banner({ kind: "info", text: "no changes" });
      return;
    }
    clearFieldErrors();
    try {
      const body = await api("/api/config", {
        method: "PUT",
        body: { profile: profileSel.value, changes },
      });
      banner({
        kind: "ok",
        text: `${body.restart_hint}`,
        diff: body.diff,
        link: { href: "/components", label: "go to /components" },
      });
      load();
    } catch (e) {
      if (e.status === 422 && e.fields) return showFieldErrors(e.fields);
      banner({ kind: "err", text: errText(e) });
    }
  }

  async function rollback() {
    clearFieldErrors();
    try {
      const body = await api("/api/config/rollback", {
        method: "POST",
        body: { profile: profileSel.value },
      });
      banner({
        kind: "ok",
        text: `restored from ${body.backup}. ${body.restart_hint}`,
        diff: body.diff,
      });
      load();
    } catch (e) {
      banner({ kind: "err", text: errText(e) });
    }
  }

  function showFieldErrors(fields) {
    banner({ kind: "err", text: "validation failed — see fields" });
    for (const [k, msg] of Object.entries(fields)) {
      const tr = root.querySelector(`tr[data-key="${CSS.escape(k)}"]`);
      const td = tr && tr.querySelector(".field-err");
      if (td) td.textContent = msg;
    }
  }

  function clearFieldErrors() {
    for (const td of root.querySelectorAll(".field-err")) td.textContent = "";
  }

  function banner(b) {
    const host = el("cfg-banner");
    if (!b) {
      host.className = "hidden";
      host.innerHTML = "";
      return;
    }
    host.className = `banner ${b.kind}`;
    const parts = [`<div>${esc(b.text)}</div>`];
    if (b.diff && b.diff.length) {
      parts.push(
        "<ul>" +
          b.diff
            .map(
              (d) =>
                `<li><code>${d.key}</code>: ${esc(d.from ?? "(unset)")} → ${esc(
                  d.to ?? "(unset)",
                )}</li>`,
            )
            .join("") +
          "</ul>",
      );
    }
    if (b.link) parts.push(`<a href="${b.link.href}">${esc(b.link.label)}</a>`);
    host.innerHTML = parts.join("");
  }

  await load();
}

function esc(s) {
  return String(s)
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

function errText(e) {
  return e?.message || "request failed";
}

function fail(host, e) {
  host.innerHTML = `<p class="err">${esc(errText(e))}</p>`;
}
