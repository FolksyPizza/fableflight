#!/usr/bin/env bash
set -euo pipefail

if [[ ${1:-} == --help || ${1:-} == -h ]]; then
    printf '%s\n' 'Usage: ./scripts/build-linux.sh [--check | cargo build options]' 'Checks Linux prerequisites and builds the locked release. --check checks prerequisites only.'
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

missing=()
command -v cc >/dev/null || missing+=(cc)
if command -v pkg-config >/dev/null; then
    for module in alsa libudev; do
        pkg-config --exists "$module" || missing+=("$module")
    done
else
    missing+=(pkg-config)
fi
if (( ${#missing[@]} )); then
    printf 'Missing Linux build prerequisites: %s\n' "${missing[*]}" >&2
    cat >&2 <<'PACKAGES'
Debian or Ubuntu: sudo apt update && sudo apt install build-essential pkg-config libasound2-dev libudev-dev
Fedora:           sudo dnf install gcc pkgconf-pkg-config alsa-lib-devel systemd-devel
Arch Linux:       sudo pacman -S --needed base-devel pkgconf alsa-lib systemd-libs
PACKAGES
    exit 1
fi

if [[ ${1:-} == --check ]]; then
    exit 0
fi

cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."
exec cargo build --release --locked "$@"
