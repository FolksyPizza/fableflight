#!/usr/bin/env bash
set -euo pipefail

if [[ ${1:-} == --help || ${1:-} == -h ]]; then
    printf '%s\n' 'Usage: ./scripts/build-linux.sh [cargo build options]' 'Builds FableFlight with the locked dependencies in release mode.'
    exit 0
fi

if [[ $(uname -s) != Linux ]]; then
    printf '%s\n' 'On macOS or Windows, use cargo build --release --locked from the checkout.' >&2
    exit 1
fi

if ! command -v cargo >/dev/null || ! command -v rustc >/dev/null; then
    printf '%s\n' 'Install the current stable Rust toolchain from https://rustup.rs and reopen your shell.' >&2
    exit 1
fi

if ! command -v cc >/dev/null || ! command -v pkg-config >/dev/null || ! pkg-config --exists alsa libudev; then
    cat >&2 <<'PACKAGES'
Install a C compiler, pkg-config, ALSA development files and libudev development files.
Debian or Ubuntu: sudo apt install build-essential pkg-config libasound2-dev libudev-dev
Fedora:           sudo dnf install gcc pkgconf-pkg-config alsa-lib-devel systemd-devel
Arch Linux:       sudo pacman -S --needed base-devel pkgconf alsa-lib systemd-libs
PACKAGES
    exit 1
fi

cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."
exec cargo build --release --locked "$@"
