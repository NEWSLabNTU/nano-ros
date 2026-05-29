#!/bin/sh
# nano-ros's `nros` installer (Phase 195.D) — fetch the prebuilt `nros` host
# binary from the nros-cli GitHub Releases.
#
#   scripts/install-nros.sh
#   curl -fsSL https://raw.githubusercontent.com/NEWSLabNTU/nano-ros/main/scripts/install-nros.sh | sh
#
# `nros` is the build tool (`nros codegen` / `nros generate-rust`) the nano-ros
# build assumes is provided (the "tools are given" principle). It is shipped
# from the standalone NEWSLabNTU/nros-cli repo, NOT built from this tree — the
# former `packages/codegen` submodule was retired in Phase 195.D. This installer
# downloads the libc-only binary for this host, verifies its sha256, and installs
# it to $NROS_HOME/bin (default ~/.nros/bin).
#
# The pinned NROS_VERSION below is the release nano-ros is validated against;
# bump it (Phase 195.D.4) when nros-cli cuts a new main-tracking release.
#
# Env:
#   NROS_VERSION  version to install (default: the nano-ros-pinned release)
#   NROS_HOME     install root (default ~/.nros); binary lands in $NROS_HOME/bin
set -eu

NROS_VERSION="${NROS_VERSION:-0.3.2}"
NROS_HOME="${NROS_HOME:-$HOME/.nros}"
REPO="NEWSLabNTU/nros-cli"
TAG="nros-v${NROS_VERSION}"

err() { echo "nros install: $*" >&2; exit 1; }

# Already on PATH? nothing to do (the build resolves $NROS_CLI / PATH / ~/.nros).
if command -v nros >/dev/null 2>&1; then
  echo "nros install: nros already on PATH ($(command -v nros)); skipping."
  exit 0
fi

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
