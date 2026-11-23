#!/usr/bin/env bash
#
# Shared helpers for the SCI Healthcare shell scripts.
#
# Source this file near the top of a script, after set -euo pipefail:
#
#   ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
#   # shellcheck source=scripts/lib.sh
#   source "$ROOT/scripts/lib.sh"
#
# shellcheck shell=bash

# say <message> — print a bold section header to stdout.
say() { printf '\n\033[1m==> %s\033[0m\n' "$1"; }

# die <message> — print an error to stderr and exit 1.
die() { printf 'error: %s\n' "$1" >&2; exit 1; }

# require_command <name> [<install-hint>] — die if the command is not on PATH.
require_command() {
  local cmd="$1"
  local hint="${2:-}"
  command -v "$cmd" >/dev/null 2>&1 || {
    if [ -n "$hint" ]; then
      die "$cmd not found. $hint"
    else
      die "$cmd not found"
    fi
  }
}

# network_env_file <root> <network> — print the path of the deployment env
# file and exit 1 with a helpful message when the file does not exist.
network_env_file() {
  local root="$1"
  local network="$2"
  local path="$root/deployments/$network.env"
  if [ ! -f "$path" ]; then
    die "No deployment found at $path. Run deploy.sh first."
  fi
  printf '%s' "$path"
}

# load_network_env <root> <network> — source the deployment env file for
# <network>, exporting every variable it sets.
load_network_env() {
  local root="$1"
  local network="$2"
  local path
  path="$(network_env_file "$root" "$network")"
  # shellcheck disable=SC1090
  set -a
  # shellcheck disable=SC1090
  source "$path"
  set +a
}
