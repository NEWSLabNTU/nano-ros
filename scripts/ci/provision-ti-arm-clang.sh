#!/usr/bin/env bash
# provision-ti-arm-clang: install TI Arm Clang into a self-hosted runner's tool
# store, for the `nros-ti` label (phase-575 W3).
#
# TI Arm Clang is `[gated.ti-arm-clang]` in nros-sdk-index.toml: licence-gated,
# so `nros setup` never fetches it. TI's terms ("TI Text File", in the release
# manifest) license use "only with TI Devices", forbid reverse engineering, and
# the installer carries a click-through; its unattended mode ACCEPTS them. So
# this runs only where the host's operator has said so:
#
#     NROS_RUNNER_ACCEPT_TI_LICENSE=1
#
# The user accepted the licence for nano-ros's own self-hosted runners on
# hosts 118 and 241 (2026-11-18). Never set it on a hosted runner or in an
# image anyone can pull, and never upload what it installs:
# `check-gated-store-not-uploaded` refuses a workflow cache or artifact path
# that could carry it.
#
# Where: <tool store>/gated/ti-arm-clang/<version>, the tool store being
# $NROS_HOME (default ~/.nros), which on a runner is its persistent volume.
# The version is the index row's; the download URL and SHA-256 are TI's,
# per version, below, and a version with none refuses.
#
#   scripts/ci/provision-ti-arm-clang.sh            install (idempotent)
#   scripts/ci/provision-ti-arm-clang.sh --root     print the install root
#   scripts/ci/provision-ti-arm-clang.sh --check    say what would happen
set -euo pipefail
repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
say() { echo "provision-ti-arm-clang: $*"; }

mode=install
case "${1:-}" in
    "") ;;
    --root) mode=root ;;
    --check) mode=check ;;
    *) say "unknown argument: $1 (--root | --check)"; exit 2 ;;
esac

# The index row is the one statement of which version nano-ros builds with.
if ! version="$(python3 - "$repo/nros-sdk-index.toml" <<'PYEOF'
import sys
try:
    import tomllib
except ModuleNotFoundError:  # Python < 3.11
    import tomli as tomllib
with open(sys.argv[1], "rb") as fh:
    row = tomllib.load(fh).get("gated", {}).get("ti-arm-clang")
if not row or "version" not in row:
    sys.exit("no [gated.ti-arm-clang] version in nros-sdk-index.toml")
print(row["version"])
PYEOF
)"; then
    exit 1
fi

store="${NROS_HOME:-$HOME/.nros}"
root="$store/gated/ti-arm-clang/$version"
if [ "$mode" = root ]; then
    echo "$root"
    exit 0
fi

# TI's published installers, per version: the URL on
# https://www.ti.com/tool/download/ARM-CGT-CLANG/<version> and the SHA-256 that
# page states beside it.
case "$version" in
    5.1.1.LTS)
        url="https://dr-download.ti.com/software-development/ide-configuration-compiler-or-debugger/MD-ayxs93eZNN/5.1.1.LTS/ti_cgt_armllvm_5.1.1.LTS_linux-x64_installer.bin"
        sha="ec4c54500b04aa3245d855038a86d3123c53c9eb14c2bfba5fcfb633dc3b3869"
        ;;
    *)
        say "no installer URL and SHA-256 for TI Arm Clang $version here; add the"
        say "  pair from https://www.ti.com/tool/download/ARM-CGT-CLANG/$version"
        exit 1
        ;;
esac
if [ "$(uname -m)" != x86_64 ]; then
    say "TI publishes the $version installer used here for linux-x64 only (this host: $(uname -m))"
    exit 1
fi

if [ -x "$root/bin/tiarmclang" ]; then
    say "TI Arm Clang $version is installed at $root"
    exit 0
fi
if [ "${NROS_RUNNER_ACCEPT_TI_LICENSE:-}" != 1 ]; then
    say "refusing: installing TI Arm Clang accepts TI's licence for this host, and"
    say "  NROS_RUNNER_ACCEPT_TI_LICENSE=1 is not set. Only the host's operator sets it,"
    say "  on a host the licence decision covers (nano-ros's own runners, 118 and 241)."
    exit 1
fi
if [ "$mode" = check ]; then
    say "would download $url"
    say "  verify SHA-256 $sha, install unattended into $root"
    exit 0
fi

mkdir -p "$store/gated/ti-arm-clang"
work="$(mktemp -d "$store/gated/ti-arm-clang/.install.XXXXXX")"
trap 'rm -rf "$work"' EXIT
installer="$work/installer.bin"
say "downloading TI Arm Clang $version (~320 MB)"
curl -fsSL --retry 3 -o "$installer" "$url"
got="$(sha256sum "$installer" | awk '{print $1}')"
if [ "$got" != "$sha" ]; then
    say "SHA-256 mismatch: got $got, TI states $sha; nothing installed"
    exit 1
fi
chmod +x "$installer"
"$installer" --mode unattended --prefix "$work/prefix" > "$work/install.log" 2>&1 || {
    say "the installer failed; its log:"; sed 's/^/    /' "$work/install.log" | tail -20; exit 1; }
built="$work/prefix/ti-cgt-armllvm_$version"
if [ ! -x "$built/bin/tiarmclang" ]; then
    say "the installer finished but left no $built/bin/tiarmclang"
    exit 1
fi
mv "$built" "$root"
say "installed TI Arm Clang $version at $root ($("$root/bin/tiarmclang" --version | head -1))"
