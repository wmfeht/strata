#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
set -euo pipefail

# Run a command in the flake development shell so quality checks use the same
# pinned toolchain as `nix develop` locally.

if [[ $# -eq 0 ]]; then
  echo "usage: $0 <command> [args...]" >&2
  exit 2
fi

if ! command -v nix >/dev/null 2>&1; then
  echo "Buildkite quality steps require Nix; see CONTRIBUTING.md" >&2
  exit 1
fi

repository="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repository"

export CARGO_TERM_COLOR="${CARGO_TERM_COLOR:-always}"
export CARGO_INCREMENTAL="${CARGO_INCREMENTAL:-0}"
export CARGO_HOME="${CARGO_HOME:-$repository/target/buildkite/cargo}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$repository/target/buildkite/build}"

exec nix --extra-experimental-features 'nix-command flakes' develop --command "$@"
