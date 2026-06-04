#!/bin/sh
# nros installer (Phase 195.A) — fetch the prebuilt `nros` host binary.
#
#   curl -fsSL https://raw.githubusercontent.com/NEWSLabNTU/nros-cli/main/install.sh | sh
#
# No cargo / no just / no checkout. Downloads the libc-only `nros` binary for
# this host from the nros-cli GitHub Releases, verifies its sha256, and installs
# it to $NROS_HOME/bin (default ~/.nros/bin). Launch-file parsing uses the
# separate `play_launch_parser` tool (`pip install play-launch-parser`); the
# prebuilt `nros` itself carries no python.
#
# Env:
#   NROS_VERSION  version to install (default below)
#   NROS_HOME     install root (default ~/.nros); binary lands in $NROS_HOME/bin
set -eu

NROS_VERSION="${NROS_VERSION:-0.2.0}"
NROS_HOME="${NROS_HOME:-$HOME/.nros}"
REPO="NEWSLabNTU/nros-cli"
TAG="nros-v${NROS_VERSION}"

err() { echo "nros install: $*" >&2; exit 1; }

# --- detect host (matches the CLI's SdkIndex::host_key) ---
os="$(uname -s)"
arch="$(uname -m)"
case "$os" in
  Linux) os="linux" ;;
  Darwin) os="macos" ;;
  *) err "unsupported OS '$os' (linux/macos only); build from source: cargo install --git https://github.com/$REPO --tag $TAG nros-cli" ;;
esac
case "$arch" in
  x86_64 | amd64) arch="x86_64" ;;
  aarch64 | arm64) arch="arm64" ;;
  *) err "unsupported arch '$arch'" ;;
esac
host="${os}-${arch}"

asset="nros-${host}.tar.zst"
base="https://github.com/${REPO}/releases/download/${TAG}"
bindir="${NROS_HOME}/bin"

command -v curl >/dev/null 2>&1 || err "curl is required"
command -v zstd >/dev/null 2>&1 || command -v tar >/dev/null 2>&1 || err "tar+zstd are required"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

echo "nros install: fetching $asset ($TAG)…"
curl -fsSL "${base}/${asset}" -o "${tmp}/${asset}" \
  || err "download failed: ${base}/${asset} (is $TAG released for $host?)"
curl -fsSL "${base}/${asset}.sha256" -o "${tmp}/${asset}.sha256" \
  || err "sha256 download failed"

# --- verify ---
echo "nros install: verifying sha256…"
expected="$(awk '{print $1}' "${tmp}/${asset}.sha256")"
if command -v sha256sum >/dev/null 2>&1; then
  actual="$(sha256sum "${tmp}/${asset}" | awk '{print $1}')"
else
  actual="$(shasum -a 256 "${tmp}/${asset}" | awk '{print $1}')"
fi
[ "$expected" = "$actual" ] || err "sha256 mismatch (expected $expected, got $actual)"

# --- install ---
mkdir -p "$bindir"
tar -C "$bindir" -xf "${tmp}/${asset}" \
  || err "extract failed (need zstd-capable tar)"
chmod +x "${bindir}/nros"

echo "nros install: installed $(${bindir}/nros --version 2>/dev/null || echo nros) → ${bindir}/nros"
case ":${PATH}:" in
  *":${bindir}:"*) ;;
  *) echo "nros install: add to PATH →  export PATH=\"${bindir}:\$PATH\"" ;;
esac
