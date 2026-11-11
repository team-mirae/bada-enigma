// Builds the page into web/dist: the engine (the crate at the repository's root) as WebAssembly,
// named by content hash so it can be cached for good, and the page (web/src). manifest.json says
// which wasm file is current; it is never cached (vercel.json).
//
//   node web/build.mjs     then serve web/dist (e.g. python3 -m http.server -d web/dist)
//
// Needs Rust with the wasm32-unknown-unknown target. On Vercel, web/vercel-build.sh installs it first.
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { copyFileSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const web = fileURLToPath(new URL(".", import.meta.url));
const root = web + "../";
const out = web + "dist/";

execFileSync("cargo", ["build", "--release", "--lib", "--target", "wasm32-unknown-unknown", "--manifest-path", root + "Cargo.toml"], { stdio: "inherit" });

rmSync(out, { recursive: true, force: true });
mkdirSync(out);
for (const f of readdirSync(web + "src")) copyFileSync(web + "src/" + f, out + f);
copyFileSync(root + "LICENSE", out + "LICENSE.txt");

const wasm = readFileSync(root + "target/wasm32-unknown-unknown/release/enigma.wasm");
const version = readFileSync(root + "Cargo.toml", "utf8").match(/^version = "(.+)"/m)[1];
const path = `enigma.${createHash("sha256").update(wasm).digest("hex").slice(0, 12)}.wasm`;
writeFileSync(out + path, wasm);
writeFileSync(out + "manifest.json", JSON.stringify({ version, wasm: path }, null, 1));
console.log(`web/dist: ${path}, ${(wasm.length / 1e3).toFixed(0)} kB`);
