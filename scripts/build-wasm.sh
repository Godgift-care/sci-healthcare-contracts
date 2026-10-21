#!/usr/bin/env bash
#
# Builds the contracts the one way the release, the deployment and the
# verification all agree on.
#
# Usage:  ./scripts/build-wasm.sh [output-dir]      (default: dist)
#
# Output: <output-dir>/sci_<contract>.wasm and <output-dir>/SHA256SUMS
#
# Reproducibility rules, in the order they matter:
#
#   1. The whole workspace is built in one cargo invocation. Cargo unifies
#      features per package, so building a single contract on its own links
#      a different soroban-sdk and produces different bytes.
#   2. The toolchain is pinned in rust-toolchain.toml (1.96.0) and the
#      release profile is fixed in the workspace Cargo.toml.
#   3. The cargo registry path is remapped out of the artefact. The Soroban
#      SDK embeds the absolute path of its own source in panic messages, so
#      without this the wasm carries the builder's home directory and two
#      machines never agree on the hash.
#   4. The result is optimised with wasm-opt -Oz, the same way the Stellar
#      toolchain optimises contracts. The binaryen version is pinned in
#      .github/workflows/release.yml.
#
# What breaks if this is wrong: the deployed contract stops matching the
# released one, and "the code on GitHub is the code on chain" stops being
# checkable.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${1:-$ROOT/dist}"
TARGET="wasm32v1-none"
# Pinned in .github/workflows/release.yml. A different binaryen can produce
# different bytes, so the script warns when it sees another one.
WASM_OPT_VERSION="133"

say() { printf '\n\033[1m==> %s\033[0m\n' "$1"; }
die() { printf 'error: %s\n' "$1" >&2; exit 1; }

command -v cargo >/dev/null || die "cargo not found. Install Rust: https://rustup.rs"
command -v wasm-opt >/dev/null || die "wasm-opt not found. Install binaryen ${WASM_OPT_VERSION} (https://github.com/WebAssembly/binaryen/releases) or 'cargo install wasm-opt'."

WASM_OPT_ACTUAL="$(wasm-opt --version 2>/dev/null | awk '{print $2}')"
if [ "$WASM_OPT_ACTUAL" != "$WASM_OPT_VERSION" ]; then
  printf 'warning: wasm-opt %s is installed, the release is built with %s. The optimised bytes may differ.\n' \
    "${WASM_OPT_ACTUAL:-unknown}" "$WASM_OPT_VERSION" >&2
fi

# The registry directory name carries a hash that depends on the cargo
# version, so it is discovered rather than hard-coded. Every registry the
# toolchain has unpacked is remapped to the same fixed prefix.
CARGO_HOME_DIR="${CARGO_HOME:-$HOME/.cargo}"
REMAP=""
# Use a fixed remapping for the entire cargo home to ensure reproducibility.
# The cargo home contains registry/index and registry/cache which may have
# non-deterministic timestamps. We remap the entire cargo home to a fixed prefix.
if [ -d "$CARGO_HOME_DIR" ]; then
  REMAP="--remap-path-prefix=$CARGO_HOME_DIR=cargo-home"
fi
[ -n "$REMAP" ] || printf 'warning: no cargo home found under %s; the build may embed local paths.\n' "$CARGO_HOME_DIR" >&2

say "Building contracts ($TARGET, release profile)"
(
  cd "$ROOT"
  # RUSTFLAGS changes the crate hash, so this is a separate build from a
  # plain `cargo build`.
  RUSTFLAGS="${RUSTFLAGS:-}$REMAP" cargo build --target "$TARGET" --release --locked
)

say "Optimising"
mkdir -p "$OUT"
rm -f "$OUT"/*.wasm "$OUT/SHA256SUMS"
for wasm in "$ROOT/target/$TARGET/release/"*.wasm; do
  [ -f "$wasm" ] || continue
  name="$(basename "$wasm")"
  wasm-opt -Oz "$wasm" -o "$OUT/$name"
  printf '  %s\n' "$name"
done

say "Checksums"
(cd "$OUT" && sha256sum ./*.wasm > SHA256SUMS && cat SHA256SUMS)

say "Built into $OUT"
