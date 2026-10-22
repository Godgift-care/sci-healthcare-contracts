#!/usr/bin/env bash
#
# Tests scripts/deploy.sh without a Stellar network.
#
# Usage:  ./scripts/test-deploy.sh
#
# The script copies deploy.sh into a temporary workspace and puts fake
# `stellar` and `build-wasm.sh` commands on PATH. That lets it assert the
# safety properties that matter without a funded account, a network or a
# real build:
#
#   * a first run writes deployments/<network>.env
#   * a second run without --force exits before calling stellar
#   * --force is required to replace an existing env file
#   * --dry-run sends no transaction, skips the build and writes no files
#
# What breaks if this is wrong: an accidental run replaces the live
# addresses, and a broken dry run either sends a transaction or fails to
# show what would happen.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

WORK="$TMP/work"
STELLAR_LOG="$TMP/stellar.log"
BUILD_LOG="$TMP/build.log"
PASS=0

fail() {
  printf 'FAIL: %s\n' "$1" >&2
  exit 1
}

pass() {
  PASS=$((PASS + 1))
  printf 'ok %d - %s\n' "$PASS" "$1"
}

reset_workspace() {
  rm -rf "$WORK"
  mkdir -p "$WORK/scripts" "$WORK/deployments" "$WORK/bin"
  cp "$ROOT/scripts/deploy.sh" "$WORK/scripts/deploy.sh"
  chmod +x "$WORK/scripts/deploy.sh"

  cat >"$WORK/scripts/build-wasm.sh" <<'FAKE_BUILD'
#!/usr/bin/env bash
set -euo pipefail
out="${1:-dist}"
mkdir -p "$out"
printf 'fake-sha256  sci_registry.wasm\n' >"$out/SHA256SUMS"
printf 'build %s\n' "$out" >>"${BUILD_LOG:?}"
FAKE_BUILD
  chmod +x "$WORK/scripts/build-wasm.sh"

  cat >"$WORK/bin/stellar" <<'FAKE_STELLAR'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >>"${STELLAR_LOG:?}"

case "$*" in
  "keys address sci-admin") echo "GADMINADMINADMINADMINADMINADMINADMINADMINADMINADMINADMINADMIN"; exit 0 ;;
  "keys address sci-issuer") echo "GISSUERISSUERISSUERISSUERISSUERISSUERISSUERISSUERISSUERISSUE"; exit 0 ;;
  "contract asset deploy "*) exit 0 ;;
  "contract id asset "*) echo "CUSDCCONTRACTID"; exit 0 ;;
  "contract deploy "*)
    wasm=""
    previous=""
    for arg in "$@"; do
      if [ "$previous" = "--wasm" ]; then wasm="$arg"; fi
      previous="$arg"
    done
    case "$wasm" in
      *sci_registry.wasm) echo "CREGISTRYCONTRACTID" ;;
      *sci_receipt.wasm) echo "CRECEIPTCONTRACTID" ;;
      *sci_voucher.wasm) echo "CVOUCHERCONTRACTID" ;;
      *) echo "CUNKNOWNCONTRACTID" ;;
    esac
    exit 0
    ;;
  "contract invoke "*) exit 0 ;;
  *) exit 0 ;;
esac
FAKE_STELLAR
  chmod +x "$WORK/bin/stellar"

  : >"$STELLAR_LOG"
  : >"$BUILD_LOG"
}

run_deploy() {
  PATH="$WORK/bin:$PATH" \
    STELLAR_LOG="$STELLAR_LOG" \
    BUILD_LOG="$BUILD_LOG" \
    bash "$WORK/scripts/deploy.sh" "$@"
}

env_file="$WORK/deployments/testnet.env"

# 1. A dry run previews every step but sends nothing and writes nothing.
reset_workspace
run_deploy testnet --dry-run >"$TMP/dry.out"
grep -q 'Dry run complete' "$TMP/dry.out" || fail "dry run has no final summary"
grep -q 'stellar contract deploy' "$TMP/dry.out" || fail "dry run does not print deploy commands"
grep -q '^+ stellar contract invoke' "$TMP/dry.out" || fail "dry run does not print invoke commands"
grep -q '^+ .*build-wasm.sh' "$TMP/dry.out" || fail "dry run does not print the build command"
[ ! -s "$STELLAR_LOG" ] || fail "dry run called stellar"
[ ! -s "$BUILD_LOG" ] || fail "dry run ran the build"
[ ! -f "$env_file" ] || fail "dry run wrote the env file"
pass "dry run prints every command and sends nothing"

# 2. A first real run writes the env file.
reset_workspace
run_deploy testnet >"$TMP/first.out"
[ -f "$env_file" ] || fail "first run did not write $env_file"
grep -q '^REGISTRY_CONTRACT_ID=CREGISTRYCONTRACTID$' "$env_file" ||
  fail "env file has the wrong registry id"
grep -q '^VOUCHER_CONTRACT_ID=CVOUCHERCONTRACTID$' "$env_file" ||
  fail "env file has the wrong voucher id"
grep -q '^RECEIPT_CONTRACT_ID=CRECEIPTCONTRACTID$' "$env_file" ||
  fail "env file has the wrong receipt id"
grep -q '^USDC_CONTRACT_ID=CUSDCCONTRACTID$' "$env_file" ||
  fail "env file has the wrong token id"
grep -q -- '--max_amount 1000000000' "$STELLAR_LOG" ||
  fail "voucher initialization did not receive the default maximum amount"
grep -q 'Saved to ' "$TMP/first.out" || fail "first run has no summary"
pass "first run deploys and writes the env file"

# 3. A second run without --force refuses before it does anything.
cp "$env_file" "$TMP/env.before"
: >"$STELLAR_LOG"
: >"$BUILD_LOG"
if run_deploy testnet >"$TMP/second.out" 2>"$TMP/second.err"; then
  fail "second run without --force succeeded"
fi
grep -qi 'refusing to overwrite' "$TMP/second.err" ||
  fail "second run does not explain why it stopped"
grep -q -- '--force' "$TMP/second.err" || fail "second run does not mention --force"
cmp -s "$env_file" "$TMP/env.before" || fail "second run changed the env file"
[ ! -s "$STELLAR_LOG" ] || fail "second run called stellar before refusing"
[ ! -s "$BUILD_LOG" ] || fail "second run built before refusing"
pass "second run refuses without --force and leaves the deployment alone"

# 4. --force replaces the env file.
printf 'STALE=1\n' >"$env_file"
run_deploy testnet --force >"$TMP/force.out"
if grep -q '^STALE=1$' "$env_file"; then
  fail "--force did not replace the existing env file"
fi
grep -q '^REGISTRY_CONTRACT_ID=CREGISTRYCONTRACTID$' "$env_file" ||
  fail "--force wrote a broken env file"
pass "--force replaces an existing deployment"

# 5. --dry-run with an existing env file is allowed, but still touches nothing.
cp "$env_file" "$TMP/env.before"
: >"$STELLAR_LOG"
: >"$BUILD_LOG"
run_deploy testnet --dry-run >"$TMP/dry-existing.out"
grep -q 'already exists; --dry-run will not touch it' "$TMP/dry-existing.out" ||
  fail "dry run does not say the existing env file is untouched"
cmp -s "$env_file" "$TMP/env.before" || fail "dry run changed an existing env file"
[ ! -s "$STELLAR_LOG" ] || fail "dry run with an existing env file called stellar"
[ ! -s "$BUILD_LOG" ] || fail "dry run with an existing env file ran the build"
pass "dry run with an existing env file sends nothing"

printf '\nAll %d deploy-script tests passed.\n' "$PASS"
