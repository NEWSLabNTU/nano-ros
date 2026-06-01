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
#   NROS_VERSION         version to install (default: the nano-ros-pinned release)
#   NROS_HOME            install root (default ~/.nros); binary lands in $NROS_HOME/bin
#   NROS_INSTALL_FORCE=1 re-install over a present `nros` even if pinned matches
#   NROS_NO_MODIFY_PATH=1 skip the rustup-style shell-rc PATH-append (default:
#                        prompt on a tty when ${NROS_HOME}/bin is missing from PATH).
#   NROS_YES=1           auto-confirm prompts (also `--yes` / `-y` arg).
#
# Args:
#   --no-modify-path     skip the shell-rc PATH-append step.
#   --yes / -y           auto-confirm prompts.
set -eu

NROS_VERSION="${NROS_VERSION:-0.3.7}"
NROS_HOME="${NROS_HOME:-$HOME/.nros}"
REPO="NEWSLabNTU/nros-cli"
TAG="nros-v${NROS_VERSION}"
NO_MODIFY_PATH="${NROS_NO_MODIFY_PATH:-0}"
ASSUME_YES="${NROS_YES:-0}"

for arg in "$@"; do
  case "$arg" in
    --no-modify-path) NO_MODIFY_PATH=1 ;;
    --yes|-y) ASSUME_YES=1 ;;
    *) echo "nros install: unknown argument '$arg' (ignored)" >&2 ;;
  esac
done

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

# Phase 211 — wcr (Why3 Component Registry) CLI. Same lazy-forwarder pattern;
# resolves to the latest installed `~/.nros/sdk/wcr/<ver>/bin/wcr`. Until wcr
# binaries ship from `github.com/NEWSLabNTU/wcr` releases, `nros setup --tool
# wcr` builds from source via `cargo install --path crates/wcr-cli`. The RMW
# arg is "wcr" (not a transport RMW, but the same shim template's --rmw flag
# slot — the error hint will read `Run: nros setup <board> --rmw wcr` which is
# the closest match the shim's templated message supports).
write_shim wcr wcr wcr

# Already on PATH? Bump it if behind the pinned NROS_VERSION (Phase 208.D/A.8,
# pattern P15). Returning users used to be silently stranded on a stale CLI that
# rejected the current SDK-index schema. Now: skip when at-or-above the pin (no
# downgrade surprise); bump when behind; force via NROS_INSTALL_FORCE=1.
if command -v nros >/dev/null 2>&1; then
  nros_path="$(command -v nros)"
  if [ "${NROS_INSTALL_FORCE:-0}" = "1" ]; then
    echo "nros install: NROS_INSTALL_FORCE=1 — re-installing over ${nros_path}"
  else
    installed="$(nros --version 2>/dev/null | awk '{print $NF}')"
    if [ -z "$installed" ]; then
      echo "nros install: ${nros_path} present but --version failed; re-installing."
    elif [ "$installed" = "$NROS_VERSION" ]; then
      echo "nros install: nros ${installed} already at pinned ${NROS_VERSION} (${nros_path}); shims refreshed; skipping download."
      exit 0
    else
      newest="$(printf '%s\n%s\n' "$installed" "$NROS_VERSION" | sort -V | tail -1)"
      if [ "$newest" = "$installed" ]; then
        echo "nros install: nros ${installed} (${nros_path}) is newer than the pinned ${NROS_VERSION}; keeping. (NROS_INSTALL_FORCE=1 to downgrade.)"
        exit 0
      fi
      echo "nros install: bumping nros ${installed} → ${NROS_VERSION} (${nros_path})"
    fi
  fi
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

# --- PATH integration (rustup-style: tty-prompt or print-hint) ------------
#
# Append `export PATH="$NROS_HOME/bin:$PATH"` to the user's shell rc, the
# same way `rustup-init` handles `~/.cargo/bin`. Strategy:
#   1. If $NROS_HOME/bin is already in PATH, do nothing.
#   2. Else, if NROS_NO_MODIFY_PATH=1 / --no-modify-path: print the manual
#      `export PATH=…` line and exit.
#   3. Else detect the user's shell + its rc file; show what would be
#      written; ask Y/n on /dev/tty. Y (default) appends; N skips with
#      the manual hint.
#   4. Non-interactive (no /dev/tty — e.g. `curl … | sh` without `< /dev/tty`):
#      same behaviour as NROS_NO_MODIFY_PATH=1 — print the hint, never
#      mutate the user's rc silently.
#
# Append is idempotent: a grep guards against re-adding the same export.
case ":${PATH}:" in
  *":${bindir}:"*) exit 0 ;;
esac

print_path_hint() {
  echo "nros install: add to PATH →  export PATH=\"${bindir}:\$PATH\""
  echo "nros install: re-run this installer with --yes (or NROS_YES=1) to write it to your shell rc automatically."
}

if [ "$NO_MODIFY_PATH" = "1" ]; then
  print_path_hint
  exit 0
fi

# Detect shell + rc file. Prefer the shell the user is invoking us with
# ($SHELL is set even in non-interactive contexts). Match rustup's
# coverage: bash, zsh, fish, POSIX fallback.
shell_name="$(basename "${SHELL:-/bin/sh}")"
case "$shell_name" in
  bash)
    # rustup writes to ~/.profile + ~/.bashrc (Linux). Match for predictability.
    rc_files="$HOME/.bashrc"
    [ -f "$HOME/.bash_profile" ] && rc_files="$rc_files $HOME/.bash_profile"
    export_line='export PATH="$HOME/.nros/bin:$PATH"'
    ;;
  zsh)
    # rustup uses ~/.zshenv (sourced for every zsh; right for PATH).
    rc_files="$HOME/.zshenv"
    export_line='export PATH="$HOME/.nros/bin:$PATH"'
    ;;
  fish)
    mkdir -p "$HOME/.config/fish/conf.d"
    rc_files="$HOME/.config/fish/conf.d/nros.fish"
    export_line='set -gx PATH $HOME/.nros/bin $PATH'
    ;;
  *)
    rc_files="$HOME/.profile"
    export_line='export PATH="$HOME/.nros/bin:$PATH"'
    ;;
esac

# Already present in any of the rc files? skip silently.
for f in $rc_files; do
  if [ -f "$f" ] && grep -Fq "$export_line" "$f" 2>/dev/null; then
    echo "nros install: PATH export already present in $f — skipping."
    echo "nros install: run \`exec \$SHELL\` (or open a new terminal) to pick it up."
    exit 0
  fi
done

# Decide: prompt? auto-yes? non-interactive?
answer=""
if [ "$ASSUME_YES" = "1" ]; then
  answer="y"
elif [ -r /dev/tty ] && [ -w /dev/tty ]; then
  echo ""
  echo "nros install: ${bindir} is not on PATH."
  echo "Append the following to ${rc_files}:"
  echo "  ${export_line}"
  printf "Proceed? (Y/n) "
  read answer < /dev/tty || answer=""
  answer="${answer:-y}"
else
  print_path_hint
  exit 0
fi

case "$answer" in
  [yY]|[yY][eE][sS]|"")
    target_file=""
    for f in $rc_files; do
      target_file="$f"
      break
    done
    printf '\n# Added by nros installer (%s)\n%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$export_line" \
      >> "$target_file"
    echo "nros install: appended PATH export to $target_file."
    echo "nros install: run \`exec \$SHELL\` (or open a new terminal) to pick it up."
    ;;
  *)
    print_path_hint
    ;;
esac
