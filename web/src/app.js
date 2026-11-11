// bada enigma on a page: a text field as a bada-compatible host (spec §7) drives the engine, and a
// lampboard shows the machine. Settings live in local storage.
import { Engine } from "./engine.js";

const $ = id => document.getElementById(id);
const ta = $("text"), backdrop = $("backdrop"), menuEl = $("menu");
const STORE = "bada-enigma-settings/1";
const LAMPS = ["QWERTZUIO", "ASDFGHJK", "PYXCVBNML"];
const ABC = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const MODIFIERS = new Set(["ShiftLeft", "ShiftRight", "ControlLeft", "ControlRight", "AltLeft", "AltRight", "MetaLeft", "MetaRight", "CapsLock", "Fn"]);

let engine = null, catalog = null;
const state = { mode: 1, mark: null, out: null, tap: null };
let saved = load();

function load() {
  try {
    const s = JSON.parse(localStorage.getItem(STORE) || "{}");
    return { engine: s.engine || {}, web: { theme: "auto", cmdTaps: true, ...s.web } };
  } catch { return { engine: {}, web: { theme: "auto", cmdTaps: true } }; }
}
function save() { localStorage.setItem(STORE, JSON.stringify(saved)); }

// ── Theme ──────────────────────────────────────────────────────────────────────────────────────

const dark = matchMedia("(prefers-color-scheme: dark)");
function applyTheme() {
  const t = saved.web.theme;
  document.documentElement.dataset.theme = t === "auto" ? (dark.matches ? "dark" : "light") : t;
}
dark.addEventListener("change", applyTheme);
applyTheme();

// ── Start ──────────────────────────────────────────────────────────────────────────────────────

async function start() {
  const manifest = await (await fetch("manifest.json", { cache: "no-cache" })).json();
  const res = fetch(manifest.wasm);
  engine = await Engine.load(WebAssembly.compileStreaming
    ? await WebAssembly.compileStreaming(res)
    : await (await res).arrayBuffer());
  catalog = engine.catalog();
  saved.engine = engine.setSettings(saved.engine);
  apply(engine.setMode(state.mode));
  ta.disabled = false;
  ta.placeholder = "Type here. Henkan mode enciphers; 英数 types plainly.";
  ta.focus();
  drawMode();
  drawMachine();
}
start().catch(e => warn(`The engine didn't load: ${e.message}`));

// ── The text field (spec §7.1) ─────────────────────────────────────────────────────────────────

function apply(out, depth = 0) {
  if (!out) return;
  const m = state.mark;
  if (out.commit) {
    const [s, e] = !out.replace_selection && m ? [m.start, m.start + m.len] : [ta.selectionStart, ta.selectionEnd];
    ta.setRangeText(out.commit, s, e, "end");
    state.mark = null;
  }
  if (out.preedit) {
    const [s, e] = state.mark ? [state.mark.start, state.mark.start + state.mark.len] : [ta.selectionStart, ta.selectionEnd];
    ta.setRangeText(out.preedit, s, e, "end");
    state.mark = { start: s, len: out.preedit.length };
  } else if (state.mark) {
    ta.setRangeText("", state.mark.start, state.mark.start + state.mark.len, "end");
    state.mark = null;
  }
  state.out = out;
  if (out.need_selection && depth === 0) {
    const text = !state.mark && ta.selectionEnd > ta.selectionStart ? ta.value.slice(ta.selectionStart, ta.selectionEnd) : "";
    apply(engine.hostSelection(text), 1);
    return;
  }
  drawField();
  drawMachine();
}

const esc = t => t.replace(/[&<>]/g, c => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;" })[c]);

function drawField() {
  const v = ta.value, m = state.mark;
  const at = m ? m.start : ta.selectionStart;
  const marked = m ? `<span class="mark">${esc(v.slice(m.start, m.start + m.len))}</span>` : "";
  backdrop.innerHTML = `${esc(v.slice(0, at))}<span id="anchor"></span>${marked}${esc(v.slice(m ? m.start + m.len : at))}\n`;
  backdrop.scrollTop = ta.scrollTop;
  drawMenu();
}

function drawMenu() {
  const out = state.out;
  if (!out || !out.menu?.length) { menuEl.hidden = true; return; }
  menuEl.replaceChildren(...out.menu.map((item, i) => {
    const row = document.createElement("div");
    row.className = "row" + (i === out.selected ? " sel" : "");
    const text = document.createElement("span");
    text.className = "mono";
    text.textContent = item;
    row.append(text);
    if (out.notes?.[i]) {
      const note = document.createElement("span");
      note.className = "note";
      note.textContent = out.notes[i];
      row.append(note);
    }
    return row;
  }));
  menuEl.hidden = false;
  const anchor = $("anchor"), editor = $("editor").getBoundingClientRect();
  const a = anchor.nextElementSibling?.getClientRects()[0] || anchor.getBoundingClientRect();
  const left = Math.max(4, Math.min(a.left - editor.left - 12, editor.width - menuEl.offsetWidth - 4));
  menuEl.style.left = `${left}px`;
  menuEl.style.top = `${a.bottom - editor.top + 6}px`;
}

ta.addEventListener("scroll", () => { backdrop.scrollTop = ta.scrollTop; drawMenu(); });
new ResizeObserver(() => drawField()).observe(ta);
ta.addEventListener("mousedown", () => { if (state.mark) apply(engine.flush()); });
ta.addEventListener("blur", () => { if (engine && state.mark) apply(engine.flush()); });
ta.addEventListener("input", () => {
  if (state.mark) { state.mark = null; apply(engine.flush()); }   // the text changed under the marked text
  state.out = null;
  drawField();
  drawMachine();
});
ta.addEventListener("select", () => drawField());

// ── Modes and keys ─────────────────────────────────────────────────────────────────────────────

function switchMode(mode) {
  if (!engine) return;
  state.mode = mode;
  apply(engine.setMode(mode));
  drawMode();
}

function drawMode() {
  $("muhenkanBtn").classList.toggle("on", state.mode === 0);
  $("henkanBtn").classList.toggle("on", state.mode === 1);
  document.body.classList.toggle("henkan", state.mode === 1);
  drawStatus();
}
$("muhenkanBtn").onclick = () => { switchMode(0); ta.focus(); };
$("henkanBtn").onclick = () => { switchMode(1); ta.focus(); };

/** One key; true if the page handled it (the browser mustn't). */
function press(code, mods) {
  if (code === "Lang2") { switchMode(0); return true; }
  if (code === "Lang1" && state.mode === 0) { switchMode(1); return true; }
  // A US keyboard has no 半角/全角: ` stands in for it in muhenkan.
  if (code === "Backquote" && state.mode === 0 && ta.selectionEnd === ta.selectionStart) return false;
  const out = engine.key(code, mods);
  apply(out);
  return out.consumed;
}

function keyDown(e) {
  if ($("settings").open || !engine) return;
  if (e.target !== ta && e.target !== document.body) return;
  if (e.isComposing || e.keyCode === 229) {
    warn("Your OS input method is composing: switch it to a plain keyboard (ABC) so bada enigma gets the keys.");
    return;
  }
  if (e.target !== ta) ta.focus();
  if (e.code === "MetaLeft" || e.code === "MetaRight") { state.tap = e.repeat ? null : e.code; return; }
  if (MODIFIERS.has(e.code)) return;
  state.tap = null;
  const code = e.code === "NumpadEnter" ? "Enter" : e.code;
  if (press(code, { shift: e.shiftKey, ctrl: e.ctrlKey, alt: e.altKey, meta: e.metaKey })) e.preventDefault();
}

function keyUp(e) {
  // ⌘ alone, as the ⌘英かな convention has it: left ⌘ 英数, right ⌘ かな.
  if (saved.web.cmdTaps && state.tap === e.code && engine && !$("settings").open) {
    press(e.code === "MetaLeft" ? "Lang2" : "Lang1", { shift: e.shiftKey });
  }
  state.tap = null;
}

addEventListener("keydown", keyDown);
addEventListener("keyup", keyUp);
addEventListener("blur", () => { state.tap = null; });
addEventListener("mousedown", () => { state.tap = null; });

function warn(text) {
  const w = $("warning");
  w.textContent = text;
  w.hidden = false;
  clearTimeout(warn.timer);
  warn.timer = setTimeout(() => { w.hidden = true; }, 6000);
}

// ── The machine ────────────────────────────────────────────────────────────────────────────────

const lampEls = {};
$("lamps").replaceChildren(...LAMPS.map(row => {
  const r = document.createElement("div");
  r.className = "lamprow";
  for (const l of row) {
    const lamp = document.createElement("span");
    lamp.className = "lamp";
    lamp.textContent = l;
    lampEls[l] = lamp;
    r.append(lamp);
  }
  return r;
}));

function drawMachine() {
  if (!engine) return;
  const m = engine.machine();
  $("ukw").textContent = m.reflector;
  $("wheels").replaceChildren(...[...m.window].map((w, i) => {
    const n = ABC.indexOf(w);
    const wheel = document.createElement("div");
    wheel.className = "wheel";
    wheel.innerHTML = `<span class="edge">${ABC[(n + 25) % 26]}</span><b>${w}</b><span class="edge">${ABC[(n + 1) % 26]}</span>`
      + `<small>${m.rotors[i]} · ${m.rings[i]}</small>`;
    return wheel;
  }));
  for (const [l, el] of Object.entries(lampEls)) el.classList.toggle("lit", l === m.lamp);
  $("plugs").textContent = m.plugboard || "(no plugs)";
  state.machine = m;
  drawStatus();
}

function drawStatus() {
  const m = state.machine;
  const parts = [`<span><b>${state.mode ? "Henkan" : "Muhenkan"}</b></span>`];
  if (m) {
    parts.push(`<span>Starting state <b class="mono">${m.start}</b></span>`);
    parts.push(m.count ? `<span>This session: <b>${m.count}</b> letter${m.count > 1 ? "s" : ""}, windows <b class="mono">${m.window}</b></span>`
      : `<span class="muted">No session: the next letter starts one</span>`);
  }
  $("status").innerHTML = parts.join("");
}

// ── Settings ───────────────────────────────────────────────────────────────────────────────────

function setEngine(change) {
  if (state.mark) apply(engine.flush());
  const next = structuredClone(saved.engine);
  change(next);
  saved.engine = engine.setSettings(next);
  save();
  drawMachine();
  drawSettings();
}

function el(tag, props = {}, ...children) {
  const e = Object.assign(document.createElement(tag), props);
  e.append(...children);
  return e;
}

function select(options, value, onChange) {
  const s = el("select");
  for (const [v, label] of options) s.append(el("option", { value: v, textContent: label, selected: v === value }));
  s.onchange = () => onChange(s.value);
  return s;
}

const letters = [...ABC].map(l => [l, l]);
const random = n => crypto.getRandomValues(new Uint32Array(1))[0] % n;

function randomKey() {
  const rotors = catalog.rotors.map(r => r.name);
  const picked = [];
  while (picked.length < 3) { const r = rotors[random(rotors.length)]; if (!picked.includes(r)) picked.push(r); }
  const three = () => Array.from({ length: 3 }, () => ABC[random(26)]).join("");
  const pool = [...ABC];
  const pairs = [];
  for (let i = 0; i < 10; i++) pairs.push(pool.splice(random(pool.length), 1)[0] + pool.splice(random(pool.length), 1)[0]);
  setEngine(s => Object.assign(s, { rotors: picked, reflector: "B", rings: three(), start: three(), plugboard: pairs.join(" ") }));
}

function drawSettings() {
  const s = saved.engine;
  const rotorNames = catalog.rotors.map(r => [r.name, r.name]);
  const hint = text => el("p", { className: "hint", textContent: text });
  const wheelRow = (label, i) => el("div", { className: "row" },
    el("span", { className: "label", textContent: label }),
    ...[0, 1, 2].map(j => i(j)));

  const plug = el("input", { type: "text", value: s.plugboard, placeholder: "AV BS CG DL FU HZ IN KM OW RX", spellcheck: false, className: "mono wide" });
  plug.onchange = () => setEngine(n => { n.plugboard = plug.value; });

  const forms = el("div", { className: "cands" }, ...catalog.forms.map(f => {
    const box = el("input", { type: "checkbox", checked: s.forms.includes(f.id) });
    box.onchange = () => setEngine(n => {
      const on = new Set(n.forms);
      box.checked ? on.add(f.id) : on.delete(f.id);
      n.forms = catalog.forms.map(x => x.id).filter(id => on.has(id));
    });
    return el("label", { className: "check" }, box, el("span", { textContent: f.label }));
  }));

  const cmd = el("input", { type: "checkbox", checked: saved.web.cmdTaps });
  cmd.onchange = () => { saved.web.cmdTaps = cmd.checked; save(); };

  $("pages").replaceChildren(el("div", { className: "page" },
    el("h3", { textContent: "Rotors (Walzenlage)" }),
    wheelRow("Left · middle · right", j => select(rotorNames, s.rotors[j], v => setEngine(n => {
      const other = n.rotors.indexOf(v);
      if (other >= 0 && other !== j) n.rotors[other] = n.rotors[j];   // swap: rotors are all different
      n.rotors[j] = v;
    }))),
    wheelRow("Reflector (UKW)", j => j ? "" : select(catalog.reflectors.map(r => [r.name, r.name]), s.reflector, v => setEngine(n => { n.reflector = v; }))),
    wheelRow("Rings (Ringstellung)", j => select(letters, s.rings[j], v => setEngine(n => { n.rings = n.rings.slice(0, j) + v + n.rings.slice(j + 1); }))),
    el("h3", { textContent: "Starting state (Grundstellung)" }),
    wheelRow("Windows", j => select(letters, s.start[j], v => setEngine(n => { n.start = n.start.slice(0, j) + v + n.start.slice(j + 1); }))),
    hint("Every henkan session starts with these letters in the windows. Backspace turns the rotors back toward them."),
    el("h3", { textContent: "Plugboard (Steckerbrett)" }),
    plug,
    hint("Pairs of letters, up to 13. A letter used twice is ignored."),
    el("div", { className: "row" },
      el("button", { textContent: "Random key", onclick: randomKey }),
      el("button", { textContent: "Barbarossa (1941)", onclick: () => setEngine(n => Object.assign(n, { rotors: ["II", "IV", "V"], reflector: "B", rings: "BUL", start: "BLA", plugboard: "AV BS CG DL FU HZ IN KM OW RX" })) }),
      el("button", { textContent: "Defaults", onclick: () => setEngine(n => { for (const k of Object.keys(n)) delete n[k]; }) })),
    el("h3", { textContent: "Menu" }),
    forms,
    hint("The first form shows while typing; Tab moves down the menu."),
    el("h3", { textContent: "Space" }),
    select([["commit", "Commits the word (the next word starts a new session)"], ["x", "Types X, as the operators did (one long session)"]], s.space, v => setEngine(n => { n.space = v; })),
    el("h3", { textContent: "This page" }),
    el("label", { className: "check" }, cmd, el("span", { textContent: "Tap left ⌘ alone for 英数, right ⌘ alone for かな" })),
    el("div", { className: "row" }, el("span", { className: "label", textContent: "Appearance" }),
      select([["auto", "As the system"], ["light", "Light"], ["dark", "Dark"]], saved.web.theme, v => { saved.web.theme = v; save(); applyTheme(); })),
    hint("Do not use as a password or encryption mechanism for anything serious. Germany lost."),
  ));
}

$("settingsBtn").onclick = () => {
  if (!engine) return;
  drawSettings();
  $("settings").showModal();
};
$("settings").addEventListener("close", () => ta.focus());
