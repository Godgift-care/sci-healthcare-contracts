# Development image for the SCI Healthcare contracts.
#
# Pins every tool to the version CI uses, so a build in the container is the
# build CI checks:
#
#   Rust        1.96.0 with wasm32v1-none   rust-toolchain.toml
#   Stellar CLI 27.1.0                      .github/workflows/e2e-local.yml
#   wasm-opt    binaryen 133                .github/workflows/ci.yml, release.yml
#
# Bump a version here in the same change that bumps it there.

FROM mcr.microsoft.com/devcontainers/rust:1-bookworm

ARG RUST_VERSION=1.96.0
ARG STELLAR_CLI_VERSION=27.1.0
ARG BINARYEN_VERSION=133
ARG JUST_VERSION=1.40.0

# The Stellar CLI links libdbus and libudev (hardware wallet support).
RUN apt-get update \
    && apt-get install -y --no-install-recommends libdbus-1-3 libudev1 \
    && rm -rf /var/lib/apt/lists/*

# The toolchain rust-toolchain.toml selects, installed up front so the first
# `cargo` call in the container does not download it.
RUN rustup toolchain install "${RUST_VERSION}" --profile minimal \
        --component rustfmt --component clippy --target wasm32v1-none \
    && rustup default "${RUST_VERSION}" \
    && chmod -R g+w "${RUSTUP_HOME}" "${CARGO_HOME}"

RUN set -eux; \
    case "$(uname -m)" in \
      x86_64) cli=x86_64-unknown-linux-gnu; bin=x86_64-linux; jst=x86_64-unknown-linux-musl ;; \
      aarch64) cli=aarch64-unknown-linux-gnu; bin=aarch64-linux; jst=aarch64-unknown-linux-musl ;; \
      *) echo "unsupported architecture $(uname -m)"; exit 1 ;; \
    esac; \
    tmp="$(mktemp -d)"; cd "$tmp"; \
    curl -fsSL "https://github.com/stellar/stellar-cli/releases/download/v${STELLAR_CLI_VERSION}/stellar-cli-${STELLAR_CLI_VERSION}-${cli}.tar.gz" | tar xz; \
    install -m755 stellar /usr/local/bin/stellar; \
    curl -fsSL "https://github.com/WebAssembly/binaryen/releases/download/version_${BINARYEN_VERSION}/binaryen-version_${BINARYEN_VERSION}-${bin}.tar.gz" | tar xz; \
    install -m755 "binaryen-version_${BINARYEN_VERSION}/bin/wasm-opt" /usr/local/bin/wasm-opt; \
    curl -fsSL "https://github.com/casey/just/releases/download/${JUST_VERSION}/just-${JUST_VERSION}-${jst}.tar.gz" | tar xz just; \
    install -m755 just /usr/local/bin/just; \
    cd /; rm -rf "$tmp"; \
    stellar --version; wasm-opt --version; just --version
