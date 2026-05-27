#!/usr/bin/env bash
# Self-provision the differential oracle's ground truth: ensure a real `zig`
# compiler is available, installing it if missing, and print a usable zig path
# on stdout. Idempotent and hermetic — this is WHY the oracle can install its
# own ground truth instead of relying on an operator hand-install: any host
# (operator or worker) that runs the gate gets zig automatically.
#
# Resolution order: PATH -> local cache -> Homebrew -> prebuilt tarball download.
set -uo pipefail

ZIG_VER="${ZIG_VERSION:-0.14.0}"
local_root="$(cd "$(dirname "$0")" && pwd)/.zig-toolchain"

log() { echo "ensure_zig: $*" >&2; }

# 1. Already on PATH.
if command -v zig >/dev/null 2>&1; then command -v zig; exit 0; fi
# 2. Previously cached local install.
if [ -x "$local_root/zig" ]; then echo "$local_root/zig"; exit 0; fi

# 3. Homebrew (macOS / linuxbrew).
if command -v brew >/dev/null 2>&1; then
  log "installing zig via Homebrew..."
  if brew install zig >&2 2>/dev/null && command -v zig >/dev/null 2>&1; then
    command -v zig; exit 0
  fi
  log "brew install did not yield a usable zig; trying direct download"
fi

# 4. Prebuilt tarball from ziglang.org (cached under oracle/.zig-toolchain).
os="$(uname -s)"; arch="$(uname -m)"
case "$os" in Darwin) zos=macos ;; Linux) zos=linux ;; *) log "unsupported OS $os"; exit 3 ;; esac
case "$arch" in arm64|aarch64) zarch=aarch64 ;; x86_64) zarch=x86_64 ;; *) log "unsupported arch $arch"; exit 3 ;; esac
# zig >=0.14 tarball naming is zig-<arch>-<os>-<ver>; older was zig-<os>-<arch>-<ver>.
for name in "zig-${zarch}-${zos}-${ZIG_VER}" "zig-${zos}-${zarch}-${ZIG_VER}"; do
  url="https://ziglang.org/download/${ZIG_VER}/${name}.tar.xz"
  tmp="$(mktemp -d)"
  log "downloading $url"
  if curl -fsSL "$url" -o "$tmp/zig.tar.xz" 2>/dev/null && tar -xf "$tmp/zig.tar.xz" -C "$tmp" 2>/dev/null; then
    dir="$(find "$tmp" -maxdepth 1 -type d -name 'zig-*' | head -1)"
    if [ -n "$dir" ] && [ -x "$dir/zig" ]; then
      mkdir -p "$local_root"
      cp -R "$dir"/. "$local_root"/
      rm -rf "$tmp"
      echo "$local_root/zig"; exit 0
    fi
  fi
  rm -rf "$tmp"
done

log "could not provision zig (no PATH, brew, or download succeeded)"
exit 3
