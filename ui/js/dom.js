// Building the page: h() and fill() make elements (all text goes in as textContent,
// never as HTML, so card data can't inject markup), the status line, dialogs, saved
// preferences and applying the display settings.

import { api, gameAPI, undoChange } from "./api.js";
import { state } from "./state.js";

export const $ = (sel) => document.querySelector(sel);

// h builds an element: h("div.cls", {attr: v, onclick: fn}, child, "text", ...).
export function h(spec, attrs, ...children) {
  const [tag, ...classes] = spec.split(".");
  const el = document.createElement(tag || "div");
  if (classes.length) el.className = classes.join(" ");
  if (attrs && (typeof attrs !== "object" || attrs instanceof Node || Array.isArray(attrs))) {
    children.unshift(attrs);
    attrs = null;
  }
  for (const [k, v] of Object.entries(attrs || {})) {
    if (v === undefined || v === null || v === false) continue;
    if (k.startsWith("on")) el.addEventListener(k.slice(2), v);
    else if (k === "style") el.style.cssText = v;
    else if (k === "checked" || k === "value") el[k] = v;
    else el.setAttribute(k, v === true ? "" : v);
  }
  for (const c of children.flat()) {
    if (c === null || c === undefined || c === false) continue;
    el.append(c instanceof Node ? c : document.createTextNode(String(c)));
  }
  return el;
}

// fill replaces el's children, flattening lists and skipping null/false like h().
export function fill(el, ...children) {
  el.replaceChildren(...children.flat(Infinity).filter((c) => c !== null && c !== undefined && c !== false)
    .map((c) => (c instanceof Node ? c : document.createTextNode(String(c)))));
}

export function store(key, value) {
  try {
    if (value === undefined) return localStorage.getItem(key);
    localStorage.setItem(key, value);
  } catch { return null; }
}

// status shows a message in the status line, optionally with Undo for a change the
// server took a backup before (`undo` from its answer: {backup, revision}).
// status shows a message in the status line, optionally with Undo for a change the
// server took a backup before (`undo` from its answer: {backup, revision}), or a
// progress bar ({done, total}).
export function status(msg, isError, undo, progress) {
  const el = $("#status");
  const undoBtn = undo && h("button.link", { type: "button", onclick: () => undoChange(undo) }, "Undo");
  const bar = progress && progress.total ? h("progress", { value: progress.done, max: progress.total }) : null;
  fill(el, bar, msg || "", undoBtn ? " " : null, undoBtn);
  el.className = isError ? "error" : "";
}

// whileRunning shows a long job's progress in the status line (polling GET /jobs
// once a second) until `promise` settles, and returns its result.
export async function whileRunning(promise) {
  let running = true;
  const timer = setInterval(async () => {
    try {
      const r = await api("GET", gameAPI("/jobs"));
      const j = r.jobs[0];
      if (!running || !j || !j.message) return;
      const count = j.total ? ` — ${j.done.toLocaleString()} of ${j.total.toLocaleString()}` : "";
      status(`${j.message}${count}…`, false, null, j);
    } catch { /* the job's own answer reports failures */ }
  }, 1000);
  try { return await promise; } finally { running = false; clearInterval(timer); }
}

// confirmDialog asks a yes/no question in the app's own dialog and resolves to true
// for yes. (The apps' web views answer window.confirm() with "no" without showing it,
// so the browser's own dialog can't be used.)
export function confirmDialog(message, { danger = false, action = "OK", title = "Are you sure?" } = {}) {
  return askDialog(title, message, [{ label: action, value: true, primary: !danger, danger }]).then((v) => v === true);
}

// ask confirms a deletion, unless the user turned that off in Settings.
export const ask = async (msg, action = "Delete") => !state.settings.confirm_delete || confirmDialog(msg, { danger: true, action });

// askDialog asks one question in a dialog of its own (so it can open over another)
// and resolves to the chosen button's value, or null if closed. buttons: [{label,
// value, primary, danger}]; with cancel false there is no Cancel button (a message
// to acknowledge).
export function askDialog(title, message, buttons, cancel = true) {
  return new Promise((resolve) => {
    const dlg = h("dialog.ask");
    let answered = false;
    const done = (v) => { if (answered) return; answered = true; dlg.close(); dlg.remove(); resolve(v); };
    const choices = buttons.map((b) => h("button" + (b.primary ? ".primary" : b.danger ? ".danger" : ""), { type: "button", onclick: () => done(b.value) }, b.label));
    fill(dlg, h("div.dlg", h("header", h("h2", title)), h("div.body", [message].flat().map((m) => h("p", m))),
      h("footer", cancel ? h("button", { type: "button", onclick: () => done(null) }, "Cancel") : null, choices)));
    dlg.addEventListener("close", () => done(null));
    document.body.append(dlg);
    dlg.showModal();
    (choices.find((b) => b.classList.contains("primary")) || choices[0])?.focus();
  });
}

// askName asks for a list's name in a dialog; null if cancelled.
export function askName(title, value) {
  return new Promise((resolve) => {
    const dlg = h("dialog.ask");
    const input = h("input", { type: "text", value, "aria-label": "Name", placeholder: "e.g. Mono-red burn" });
    let done = false;
    const finish = (v) => { if (done) return; done = true; dlg.close(); dlg.remove(); resolve(v); };
    const ok = () => { if (input.value.trim()) finish(input.value.trim()); else input.focus(); };
    input.addEventListener("keydown", (e) => { if (e.key === "Enter") ok(); });
    fill(dlg, h("div.dlg", h("header", h("h2", title)), h("div.body", input),
      h("footer", h("button", { type: "button", onclick: () => finish(null) }, "Cancel"), h("button.primary", { type: "button", onclick: ok }, "Save"))));
    dlg.addEventListener("close", () => finish(null));
    document.body.append(dlg);
    dlg.showModal();
    input.focus();
  });
}

// toolRow is one action on the Tools page with what it does.
export const toolRow = (button, about) => h("div.tool", button, h("p.muted", about));

// applySettings applies the appearance settings to the page.
export function applySettings() {
  const s = state.settings;
  const root = document.documentElement;
  if (s.theme === "system") delete root.dataset.theme; else root.dataset.theme = s.theme;
  root.style.setProperty("--scale", { small: 0.9, normal: 1, large: 1.15, larger: 1.3 }[s.text_size] || 1);
}
