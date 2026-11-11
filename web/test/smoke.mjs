// The engine as WebAssembly, through its C ABI as the page calls it: node web/test/smoke.mjs
// (after node web/build.mjs).
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { Engine } from "../src/engine.js";

const dist = fileURLToPath(new URL("../dist/", import.meta.url));
const manifest = JSON.parse(readFileSync(dist + "manifest.json", "utf8"));
const e = await Engine.load(readFileSync(dist + manifest.wasm));

const type = keys => { let out; for (const k of keys) out = e.key(k.length === 1 ? "Key" + k.toUpperCase() : k); return out; };

e.setMode(1);
assert.equal(type("aaaaa").preedit, "BDZGO", "I-II-III, B, AAA: AAAAA → BDZGO");
assert.equal(e.machine().window, "AAF");
assert.equal(type(["Backspace", "Backspace"]).preedit, "BDZ", "Backspace turns the rotors back");
assert.equal(e.machine().window, "AAD");
assert.equal(e.state().staged, "BDZ");
assert.equal(e.key("Space").commit, "BDZ");
assert.equal(type("a").preedit, "B", "a new session starts from the starting state");
e.flush();

const s = e.setSettings({ rotors: ["II", "IV", "V"], rings: "BUL", start: "BLA", plugboard: "av bs cg dl fu hz in km ow rx" });
assert.equal(s.plugboard, "AV BS CG DL FU HZ IN KM OW RX");
assert.equal(type("EDPUDNRGYSZRCXNUYTPOMRMBOFKTBZREZKMLXLVEFGUEYSIOZVEQMIKUBPMM").preedit.slice(0, 20), "AUFKLXABTEILUNGXVONX", "Barbarossa, 1941");
e.flush();

e.setMode(0);
assert.equal(e.key("KeyA").consumed, false, "muhenkan passes letters on");
const ask = e.key("Backquote");
assert.equal(ask.need_selection, "enigma");
assert.equal(e.hostSelection("Hello, world").commit.length, 12);
console.log("smoke: ok");
