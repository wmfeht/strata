#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
set -euo pipefail

# Install the mise-pinned toolchain from mise.toml, then run the command with
# those tools on PATH. Quality and E2E still compile inside the published
# Docker environment; this wrapper supplies host Python, cargo-deny, and typos.

if [[ $# -eq 0 ]]; then
  echo "usage: $0 <command> [args...]" >&2
  exit 2
fi

if ! command -v mise >/dev/null 2>&1; then
  echo "Buildkite steps require mise; see CONTRIBUTING.md" >&2
  exit 1
fi

repository="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repository"

export CARGO_TERM_COLOR="${CARGO_TERM_COLOR:-always}"
export CARGO_INCREMENTAL="${CARGO_INCREMENTAL:-0}"
export CARGO_HOME="${CARGO_HOME:-$repository/target/buildkite/cargo}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$repository/target/buildkite/build}"
export STRATA_CONTAINER_ENGINE="${STRATA_CONTAINER_ENGINE:-docker}"
export MISE_YES="${MISE_YES:-1}"
export MISE_DATA_DIR="${MISE_DATA_DIR:-$repository/target/buildkite/mise}"
export MISE_CACHE_DIR="${MISE_CACHE_DIR:-$repository/target/buildkite/mise-cache}"

mise install
exec mise exec -- "$@"
