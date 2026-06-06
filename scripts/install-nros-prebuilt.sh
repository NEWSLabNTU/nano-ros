#!/bin/sh
# nano-ros's `nros` prebuilt installer (Phase 218.G).
#
# Fetches the `nros` CLI binary published by the `release-nros-cli.yml`
# workflow as a release asset of THIS repo (nano-ros), unpacks it into
# `packages/cli/target/release/nros` — the same path an in-tree `cargo build
# --release --manifest-path packages/cli/Cargo.toml --bin nros` produces, so
# downstream consumers (the justfile, scripts/build/cargo.sh::nros_cli_bin)
# see the same surface either way.
#
# Use cases:
#   * acceptance lane on a bare runner (no Rust toolchain — Gap A).
#   * fresh-machine "I want the CLI without compiling" path for end users.
#
# Anchor: `git describe --tags --abbrev=0` — the closest reachable tag. The
# script is invoked from inside the nano-ros tree (it needs `git` for tag
# discovery + LICENSE/README copies — both ship inside the artifact, so the
# tree isn't strictly required for those, but tag discovery is).
#
# Fall-through:
#   * No tag reachable → print the closest-ancestor info + suggest building
#     from source (`scripts/bootstrap.sh base`, or the cargo invocation).
#   * No asset matching the host triple → same.
#
# Env:
#   NROS_PREBUILT_TAG   override `git describe` (e.g. NROS_PREBUILT_TAG=nros-v0.4.0).
#   NROS_PREBUILT_REPO  override the GitHub repo (default NEWSLabNTU/nano-ros).
#
# Exit codes:
#   0  installed
#   1  fall-through (no tag / no asset / sha256 mismatch / extract failed)
set -eu

REPO_DEFAULT="NEWSLabNTU/nano-ros"
REPO="${NROS_PREBUILT_REPO:-$REPO_DEFAULT}"

err() { echo "install-nros-prebuilt: $*" >&2; exit 1; }
warn() { echo "install-nros-prebuilt: $*" >&2; }

# --- resolve tag --------------------------------------------------------
if [ -n "${NROS_PREBUILT_TAG:-}" ]; then
  tag="$NROS_PREBUILT_TAG"
  echo "install-nros-prebuilt: using NROS_PREBUILT_TAG=$tag"
else
  command -v git >/dev/null 2>&1 || err "git is required (or pass NROS_PREBUILT_TAG)"
  if ! tag="$(git describe --tags --abbrev=0 2>/dev/null)"; then
    warn "no reachable tag in this checkout — cannot resolve a prebuilt artifact"
    warn "fall through to source build:  cargo build --release --manifest-path packages/cli/Cargo.toml --bin nros"
    warn "(or:  scripts/bootstrap.sh base)"
    exit 1
  fi
  echo "install-nros-prebuilt: resolved tag via git describe: $tag"
fi

# --- detect host triple -------------------------------------------------
os="$(uname -s)"
arch="$(uname -m)"
case "$os" in
  Linux) os_part="unknown-linux-gnu" ;;
  Darwin) os_part="apple-darwin" ;;
  *) err "unsupported OS '$os' (linux/macos only)" ;;
esac
case "$arch" in
  x86_64 | amd64) arch_part="x86_64" ;;
  aarch64 | arm64) arch_part="aarch64" ;;
  *) err "unsupported arch '$arch'" ;;
esac
triple="${arch_part}-${os_part}"
asset="nros-${triple}.tar.gz"
echo "install-nros-prebuilt: host triple: $triple"

# --- locate the repo root (we drop the binary at packages/cli/target/...) ---
# Prefer git's notion; fall back to PWD if we're outside a checkout (less
# convenient but supported when NROS_PREBUILT_TAG is set).
if repo_root="$(git rev-parse --show-toplevel 2>/dev/null)"; then
  :
else
  repo_root="$PWD"
fi

# --- fetch -------------------------------------------------------------
base="https://github.com/${REPO}/releases/download/${tag}"
command -v curl >/dev/null 2>&1 || err "curl is required"
command -v tar >/dev/null 2>&1 || err "tar is required"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

echo "install-nros-prebuilt: fetching $asset from $base"
if ! curl -fsSL "${base}/${asset}" -o "${tmp}/${asset}"; then
  warn "download failed: ${base}/${asset}"
  warn "(is $tag released for $triple? https://github.com/${REPO}/releases/tag/${tag})"
  warn "fall through to source build:  cargo build --release --manifest-path packages/cli/Cargo.toml --bin nros"
  exit 1
fi
if ! curl -fsSL "${base}/${asset}.sha256" -o "${tmp}/${asset}.sha256"; then
  err "sha256 sidecar download failed: ${base}/${asset}.sha256"
fi

# --- verify sha256 -----------------------------------------------------
echo "install-nros-prebuilt: verifying sha256"
expected="$(awk '{print $1}' "${tmp}/${asset}.sha256")"
if command -v sha256sum >/dev/null 2>&1; then
  actual="$(sha256sum "${tmp}/${asset}" | awk '{print $1}')"
else
  actual="$(shasum -a 256 "${tmp}/${asset}" | awk '{print $1}')"
fi
[ "$expected" = "$actual" ] || err "sha256 mismatch (expected $expected, got $actual)"

# --- extract + install -------------------------------------------------
dest_dir="${repo_root}/packages/cli/target/release"
mkdir -p "$dest_dir"
tar -C "$tmp" -xzf "${tmp}/${asset}" || err "extract failed"
test -x "$tmp/nros" || err "extracted bundle does not contain a 'nros' binary"
install -m 0755 "$tmp/nros" "${dest_dir}/nros"

echo "install-nros-prebuilt: installed $(${dest_dir}/nros --version 2>/dev/null || echo nros) → ${dest_dir}/nros"
echo "install-nros-prebuilt: add to PATH →  export PATH=\"${dest_dir}:\$PATH\""
