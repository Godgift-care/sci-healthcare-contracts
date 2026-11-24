#!/usr/bin/env bash
#
# Exports every contract's interface spec as JSON next to its wasm, then
# rewrites SHA256SUMS so the checksum file covers the specs as well.
#
# Usage:  ./scripts/export-specs.sh [dist-dir]      (default: dist)
#
# Input:  <dist-dir>/sci_<contract>.wasm   (from ./scripts/build-wasm.sh)
# Output: <dist-dir>/sci_<contract>.json   and <dist-dir>/SHA256SUMS
#
# The spec is what `stellar contract info interface` reads out of the wasm: the
# functions, types, errors and events the Stellar host exposes. Integrators
# generating bindings get the interface without building or downloading the
# wasm, and a release carries the interface and the bytes it describes.
#
# The CLI version is pinned in .devcontainer/Dockerfile and
# .github/workflows/release.yml (27.1.0). Another version still writes the same
# JSON, but keep the release on the pin so a spec never describes a newer
# compiler's output than the wasm beside it.
#
# What breaks if this is wrong: the release attaches specs that do not describe
# the released wasm, and generated bindings call functions the contract does
# not have.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST="${1:-$ROOT/dist}"

say() { printf '\n\033[1m==> %s\033[0m\n' "$1"; }
die() { printf 'error: %s\n' "$1" >&2; exit 1; }

command -v stellar >/dev/null || die "the stellar CLI is required: https://github.com/stellar/stellar-cli"
[ -d "$DIST" ] || die "no build directory at $DIST. Run ./scripts/build-wasm.sh first."

shopt -s nullglob
wasms=("$DIST"/sci_*.wasm)
[ ${#wasms[@]} -gt 0 ] || die "no contract wasm in $DIST. Run ./scripts/build-wasm.sh first."

say "Exporting the interface specs"
for wasm in "${wasms[@]}"; do
  spec="$DIST/$(basename "${wasm%.wasm}").json"
  stellar contract info interface --wasm "$wasm" --output json >"$spec"
  printf '  %s\n' "$(basename "$spec")"
done

say "Checksums"
(cd "$DIST" && rm -f SHA256SUMS && sha256sum ./*.wasm ./*.json > SHA256SUMS && cat SHA256SUMS)

say "Exported into $DIST"
