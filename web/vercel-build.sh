#!/bin/sh
# Vercel's build: its image has no Rust, so install a minimal toolchain with the wasm target, then
# build the page (web/build.mjs). Vercel runs this from the repository's root (vercel.json).
set -e
if ! command -v cargo >/dev/null 2>&1; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --target wasm32-unknown-unknown
  . "$HOME/.cargo/env"
fi
rustup target add wasm32-unknown-unknown
node web/build.mjs
node web/test/smoke.mjs
