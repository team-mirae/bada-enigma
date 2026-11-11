// bada enigma's engine: the crate compiled to WebAssembly, called through its bada-compatible C
// ABI (include/enigma.h) as a native host would. Strings go in NUL-terminated (memory from
// wasm_alloc) and come back as C strings owned by the engine. Works in the browser and in Node.

const encoder = new TextEncoder();
const decoder = new TextDecoder();

export class Engine {
  /** `wasm`: the module's bytes or a compiled WebAssembly.Module. */
  static async load(wasm) {
    const instance = wasm instanceof WebAssembly.Module
      ? await WebAssembly.instantiate(wasm, {})
      : (await WebAssembly.instantiate(wasm, {})).instance;
    return new Engine(instance.exports);
  }

  constructor(exports) {
    this.x = exports;
    this.h = exports.enigma_new(0);   // P_new(NULL): bada enigma has no data
  }

  /** A C string at `ptr`, as text. */
  read(ptr) {
    if (!ptr) return "null";
    const mem = new Uint8Array(this.x.memory.buffer);
    let end = ptr;
    while (mem[end]) end++;
    return decoder.decode(mem.subarray(ptr, end));
  }

  /** Calls `enigma_<name>(engine, ...args)`; string args are passed as C strings. JSON back. */
  call(name, ...args) {
    const owned = [];
    const raw = args.map(a => {
      if (typeof a !== "string") return a;
      const bytes = encoder.encode(a + "\0");
      const ptr = this.x.wasm_alloc(bytes.length);
      new Uint8Array(this.x.memory.buffer, ptr, bytes.length).set(bytes);
      owned.push([ptr, bytes.length]);
      return ptr;
    });
    const reply = this.read(this.x["enigma_" + name](this.h, ...raw));
    for (const [ptr, len] of owned) this.x.wasm_free(ptr, len);
    return JSON.parse(reply);
  }

  key(code, mods = {}) { return this.call("key", JSON.stringify({ key: code, ...mods })); }
  setMode(mode) { return this.call("set_mode", mode); }
  flush() { return this.call("flush"); }
  setSettings(settings) { return this.call("set_settings", JSON.stringify(settings)); }
  hostSelection(text) { return this.call("host_selection", text); }
  catalog() { return this.call("catalog"); }
  state() { return this.call("state"); }
  machine() { return this.call("machine"); }
}
