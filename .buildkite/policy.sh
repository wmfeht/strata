#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
set -euo pipefail

# Versions match .github/workflows/ci.yml: cargo-deny-action v2.1.1 ships
# cargo-deny 0.20.2, and crate-ci/typos is pinned to v1.50.1.
DENY_VERSION=0.20.2
TYPOS_VERSION=1.50.1

repository="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repository"

tool="${1:-}"
case "$tool" in
  deny|typos) ;;
  *)
    echo "usage: $0 deny|typos" >&2
    exit 2
    ;;
esac

if ! command -v curl >/dev/null 2>&1; then
  echo "policy checks require curl to download pinned musl binaries" >&2
  exit 1
fi

arch="$(uname -m)"
case "$arch" in
  x86_64) triple=x86_64-unknown-linux-musl ;;
  aarch64|arm64) triple=aarch64-unknown-linux-musl ;;
  *)
    echo "unsupported architecture for policy tools: $arch" >&2
    exit 1
    ;;
esac

workdir="$(mktemp -d)"
trap 'rm -rf "$workdir"' EXIT

if [[ "$tool" == deny ]]; then
  curl -fsSL "https://github.com/EmbarkStudios/cargo-deny/releases/download/${DENY_VERSION}/cargo-deny-${DENY_VERSION}-${triple}.tar.gz" \
    | tar -xz -C "$workdir"
  exec "$workdir/cargo-deny-${DENY_VERSION}-${triple}/cargo-deny" check
fi

curl -fsSL "https://github.com/crate-ci/typos/releases/download/v${TYPOS_VERSION}/typos-v${TYPOS_VERSION}-${triple}.tar.gz" \
  | tar -xz -C "$workdir"
exec "$workdir/typos"
