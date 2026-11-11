# bada enigma

- A bada-compatible v1.1 IME (`docs/bada-compatible-v1.md`). `src/compat/` is copied from
  team-mirae/bada-compatible: don't edit it here; changes go upstream.
- Names: bada enigma (lowercase). The IME is `src/enigma.rs`, the machine `src/machine.rs`.
- Keep `include/enigma.h`, `src/lib.rs` (export! and `enigma_machine`) and `web/src/engine.js` in agreement.
- Tests: `cargo test`, `cargo clippy --all-targets`, `node web/build.mjs && node web/test/smoke.mjs`.
- Web (enigma.sjun.me): Vercel Git integration builds on push to main via `web/vercel-build.sh`; no token in CI.
