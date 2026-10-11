#!/usr/bin/env bash
# Bootstrap a Cloud Agent checkout: native libraries, pinned Rust, then a locked type-check.
# Do not apt-get upgrade or rustup update stable here. Those rewrote a working image on every
# recurring build and left install failing on the next system library Cargo started probing.
set -euo pipefail

# bash -s leaves BASH_SOURCE unset; the Cloud Agent install command uses that path.
script_path=${BASH_SOURCE[0]:-}
if [[ $script_path == */cloud-agent-install.sh ]]; then
  cd "$(cd "$(dirname "$script_path")/.." && pwd)"
else
  cd /workspace
fi

export PATH="/usr/local/cargo/bin:${PATH}"
export DEBIAN_FRONTEND=noninteractive
# This image points /usr/bin/c++ at clang, which passes -stdlib=libc++.
# libc++-dev conflicts with libunwind-dev, and libgstreamer1.0-dev needs the latter.
export CC=gcc
export CXX=g++

# Keep in lockstep with mise.toml and tests/e2e/Dockerfile (RUSTUP_TOOLCHAIN).
RUST_VERSION=1.98.1

compile_packages=(
  build-essential
  pkg-config
  libunwind-dev
  libfontconfig1-dev
  libgtk-4-dev
  libgtksourceview-5-dev
  libpoppler-glib-dev
  libgstreamer1.0-dev
  libgstreamer-plugins-base1.0-dev
  bubblewrap
  ffmpeg
  ffmpegthumbnailer
  gstreamer1.0-libav
  gstreamer1.0-plugins-base
  gstreamer1.0-plugins-good
  gvfs
  gvfs-backends
  librsvg2-2
  librsvg2-common
)

agent_packages=(
  adwaita-icon-theme
  at-spi2-core
  dbus-bin
  dbus-daemon
  fonts-cantarell
  fonts-noto-color-emoji
  fonts-noto-core
  gir1.2-atspi-2.0
  imagemagick
  python3-gi
  python3-pip
  python3-venv
  xvfb
)

# Probes match Cargo.toml features: gtk4 v4_12 and gstreamer / gstreamer-app v1_20.
pkgconfig_probes=(
  fontconfig
  'gtk4 >= 4.12'
  gtksourceview-5
  poppler-glib
  'gstreamer-1.0 >= 1.20'
  'gstreamer-app-1.0 >= 1.20'
  'gstreamer-base-1.0 >= 1.20'
)

if dpkg -s libc++-dev >/dev/null 2>&1; then
  sudo -n apt-get remove -y libc++-dev libc++-18-dev libc++abi-18-dev libunwind-18-dev libunwind-18
fi

sudo -n apt-get update
sudo -n apt-get install -y --no-install-recommends \
  "${compile_packages[@]}" \
  "${agent_packages[@]}"

# The image pins c++ to clang in manual mode. cc-rs then passes -stdlib=libc++,
# and the libc++ packages conflict with libunwind-dev. g++ is already the
# higher-priority alternative; put the link back on automatic selection.
if update-alternatives --query c++ >/dev/null 2>&1; then
  sudo -n update-alternatives --auto c++
fi

missing=0
for probe in "${pkgconfig_probes[@]}"; do
  if ! pkg-config --exists "$probe"; then
    printf 'missing pkg-config module: %s\n' "$probe" >&2
    missing=1
  fi
done
if ((missing != 0)); then
  exit 1
fi

if ! rustup toolchain list | grep -q "^${RUST_VERSION}-"; then
  rustup toolchain install "$RUST_VERSION" --profile minimal --component rustfmt,clippy
fi
rustup default "$RUST_VERSION"
hash -r
if ! rustc --version | grep -q " ${RUST_VERSION} "; then
  printf 'rustc is %s; expected %s\n' "$(rustc --version)" "$RUST_VERSION" >&2
  exit 1
fi

cargo fetch --locked
cargo check --locked --all-targets --all-features

printf 'native deps ok; %s\n' "$(rustc --version)"
pkg-config --modversion gtk4 gtksourceview-5 poppler-glib gstreamer-1.0 gstreamer-app-1.0 gstreamer-base-1.0 fontconfig
