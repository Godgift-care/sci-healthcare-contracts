# Common tasks for the SCI Healthcare contracts.
#
#   just            list the recipes
#   just check      what CI checks: fmt, clippy and the tests
#   just wasm       the release build, the one deploy.sh deploys
#
# Recipes are thin wrappers: each one runs the command the docs and CI
# already use, so `just <recipe>` and running the command by hand never
# disagree. The deploy, seed and verify recipes take a network name and
# default to testnet; set NETWORK to change the default.

set shell := ["bash", "-euo", "pipefail", "-c"]

network := env_var_or_default("NETWORK", "testnet")

# List the recipes.
default:
    @just --list

# Build every contract for the host (what the tests link).
build:
    cargo build --all

# Run every contract's tests.
test:
    cargo test --all

# Format the workspace.
fmt:
    cargo fmt --all

# Fail if anything is not formatted (the CI fmt job).
fmt-check:
    cargo fmt --all -- --check

# Lint with warnings as errors (the CI clippy job).
clippy:
    cargo clippy --all-targets --all-features -- -D warnings

# Everything CI runs on a pull request before the wasm build.
check: fmt-check clippy test

# Build the release wasm and SHA256SUMS into OUT (needs wasm-opt, binaryen 133).
wasm out="dist":
    ./scripts/build-wasm.sh {{out}}

# Deploy to NETWORK; extra flags pass through, e.g. `just deploy testnet --dry-run`.
deploy net=network *flags="":
    ./scripts/deploy.sh {{net}} {{flags}}

# Seed a deployed NETWORK with a demo clinic and vouchers.
seed net=network:
    ./scripts/seed.sh {{net}}

# Check the contracts deployed on NETWORK are the ones this source builds.
verify net=network:
    ./scripts/verify-deployments.sh {{net}}

# Test deploy.sh's safety rules without a network (the CI deploy script job).
test-deploy:
    ./scripts/test-deploy.sh

# Deploy, seed and check against a local node already running on port 8000.
e2e-local:
    ./scripts/e2e-local.sh
