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

NROS_VERSION="${NROS_VERSION:-0.3.7}"
NROS_HOME="${NROS_HOME:-$HOME/.nros}"
REPO="NEWSLabNTU/nros-cli"
TAG="nros-v${NROS_VERSION}"

err() { echo "nros install: $*" >&2; exit 1; }

bindir="${NROS_HOME}/bin"

# --- RMW host-daemon forwarder shims (Phase 208.B Track A) ---
# Always (re)write these so re-running the installer is idempotent and
# pre-existing-nros installs still get them. The shims resolve the
# latest tool installed under ${NROS_HOME}/sdk/<tool>/<version>/bin/
# at exec time, so a `nros setup --rmw <rmw>` after this script's run
# "just works" without re-running the installer.
mkdir -p "$bindir"
write_shim() {
  shim_name="$1"; tool_dir="$2"; rmw_arg="$3"
  shim_path="${bindir}/${shim_name}"
  # Quoted heredoc — text passes through unchanged; the shim itself
  # holds the variable expansions (resolved at shim-run-time, not now).
  cat > "${shim_path}" <<'SHIM'
#!/bin/sh
# Lazy forwarder shim written by scripts/install-nros.sh. Picks the
# latest __TOOL__ version installed in the nros SDK store.
store_root="${NROS_HOME:-$HOME/.nros}/sdk/__TOOL__"
target="$(ls -d "${store_root}"/*/bin/__SHIM__ 2>/dev/null | tail -1)"
if [ -z "$target" ] || [ ! -x "$target" ]; then
  echo "__SHIM__: not installed in ${store_root}. Run: nros setup <board> --rmw __RMW__" >&2
  exit 127
fi
exec "$target" "$@"
SHIM
  # Patch the install-time placeholders.
  sed -i.bak \
      -e "s|__TOOL__|${tool_dir}|g" \
      -e "s|__SHIM__|${shim_name}|g" \
      -e "s|__RMW__|${rmw_arg}|g" \
      "${shim_path}"
  rm -f "${shim_path}.bak"
  chmod +x "${shim_path}"
}
write_shim zenohd zenohd zenoh
write_shim MicroXRCEAgent xrce-agent xrce

# Already on PATH? nothing more to do (the build resolves $NROS_CLI / PATH / ~/.nros).
if command -v nros >/dev/null 2>&1; then
  echo "nros install: nros already on PATH ($(command -v nros)); shims refreshed; skipping download."
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
tar -C "$bindir" -xf "${tmp}/${asset}" \
  || err "extract failed (need zstd-capable tar)"
chmod +x "${bindir}/nros"

echo "nros install: installed $(${bindir}/nros --version 2>/dev/null || echo nros) → ${bindir}/nros"
echo "nros install: forwarding shims → ${bindir}/{zenohd,MicroXRCEAgent}"

case ":${PATH}:" in
  *":${bindir}:"*) ;;
  *) echo "nros install: add to PATH →  export PATH=\"${bindir}:\$PATH\"" ;;
esac
