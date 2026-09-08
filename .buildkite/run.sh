#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
set -euo pipefail

# Run a command in the same pinned toolchain image as scripts/e2e.sh so quality
# checks and the GUI suite share Docker layers. Image tag, platform, UID/GID,
# and build-args must stay aligned with that runner.

if [[ $# -eq 0 ]]; then
  echo "usage: $0 <command> [args...]" >&2
  exit 2
fi

repository="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
engine="${STRATA_CONTAINER_ENGINE:-docker}"
if ! command -v "$engine" >/dev/null 2>&1; then
  echo "Buildkite quality steps require Docker or Podman; see docs/e2e-testing.md" >&2
  exit 1
fi

user_id="$(id -u)"
group_id="$(id -g)"
image_key="$(python3 "$repository/scripts/e2e_bundle.py" image-key)"
image="strata-e2e:${image_key:0:16}-$user_id-$group_id"
"$engine" build --platform=linux/amd64 --target toolchain --tag "$image" \
  --build-arg "E2E_UID=$user_id" --build-arg "E2E_GID=$group_id" \
  --build-arg "E2E_IMAGE_KEY=$image_key" \
  --file "$repository/tests/e2e/Dockerfile" "$repository"

options=(--rm --platform=linux/amd64 --user "$user_id:$group_id")
if [[ "$(basename "$engine")" == podman ]]; then
  options+=(--userns=keep-id --passwd=false)
fi
exec "$engine" run "${options[@]}" \
  --mount "type=bind,source=$repository,target=/workspace" \
  --workdir /workspace \
  --env HOME=/tmp/strata-build-home \
  --env CARGO_HOME=/workspace/target/buildkite/cargo \
  --env CARGO_TARGET_DIR=/workspace/target/buildkite/build \
  --env CARGO_TERM_COLOR=always \
  --env CARGO_INCREMENTAL=0 \
  --env RUSTUP_HOME=/opt/rustup \
  --env RUSTUP_TOOLCHAIN=1.98.1 \
  "$image" bash -euc '
    mkdir -p "$HOME" "$CARGO_HOME"
    rustc --version
    exec "$@"
  ' bash "$@"
