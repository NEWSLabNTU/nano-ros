#!/usr/bin/env bash
set -euo pipefail
echo "=== Verus Setup ==="
VERUS_DIR="tools"
VERUS_BIN="$VERUS_DIR/verus"

# Issue 0368 F6 / phase-327 W6 — PIN the release. `releases/latest` moved to a
# runner whose binary needs `GLIBC_2.39` (Ubuntu 24.04+); on the oldest supported
# LTS (Ubuntu 22.04, glibc 2.35) it dies with a raw loader error. The default is
# the NEWEST release measured to run on 2.35 (bisected 2026-08-01: 0.2026.05.17 +
# 0.2026.06.28 run; 0.2026.07.05/07.12/07.18/07.27 all demand GLIBC_2.39). When
# bumping, test `tools/verus --version` on the oldest supported LTS first.
# Override with VERUS_VERSION=<tag> or VERUS_VERSION=latest; the glibc guard
# below degrades to an informative message instead of a hard failure
# (verification is in no CI tier gate, so a host that cannot run this tool must
# not have its `just verification setup` fail).
VERUS_VERSION="${VERUS_VERSION:-release/0.2026.06.28.1847ab3}"

# Print the glibc-degrade note and exit 0 (informative, not fatal).
verus_glibc_degrade() {
    cat <<EOF
[verus] The downloaded Verus binary could not run on this host — almost always a
        glibc mismatch: recent Verus releases are built against a newer glibc than
        this LTS provides (issue 0368 F6). Verification is not gated in any CI tier,
        so this is informative, not fatal.
        Options: run on a newer host, build Verus from source, or pin an older
        release: VERUS_VERSION=<tag> just verification verus
        (tags: https://github.com/verus-lang/verus/releases)
EOF
    exit 0
}

install_toolchain() {
    local required_tc
    # Two formats: an installed-toolchain verus prints "Toolchain: <tc>";
    # a FRESH one cannot even print --version and errors
    # "required rust toolchain <tc> not found" — parse both (phase-327 W6:
    # the fresh-install path always hit the second and installed nothing).
    required_tc=$("$VERUS_BIN" --version 2>&1 \
        | sed -n -e 's/.*Toolchain: //p' -e 's/.*required rust toolchain \([^ ]*\) not found.*/\1/p' \
        | head -1 || true)
    if [ -n "$required_tc" ]; then
        if rustup run "$required_tc" rustc --version &>/dev/null; then
            echo "Required toolchain already installed: $required_tc"
        else
            echo "Installing required toolchain: $required_tc"
            rustup toolchain install "$required_tc"
        fi
    fi
}

# `verus --version` fails for TWO distinct reasons and they need opposite
# handling: a loader/glibc error (degrade informatively — installing anything
# won't help) vs a missing pinned rust toolchain (install it — the normal
# fresh-install path). Disambiguate on the error text.
verus_ready_or_fix() {
    local out
    if out=$("$VERUS_BIN" --version 2>&1); then
        return 0
    fi
    if echo "$out" | grep -q 'required rust toolchain'; then
        install_toolchain
        return 0
    fi
    verus_glibc_degrade
}

if [ -x "$VERUS_BIN" ]; then
    echo "Verus already installed at $VERUS_BIN"
    # Toolchain / glibc triage FIRST: a verus whose pinned rust toolchain is
    # absent cannot even print --version, and under `set -e` that used to
    # abort this branch before install_toolchain could fix exactly that
    # (phase-327 W6).
    verus_ready_or_fix
    "$VERUS_BIN" --version
    exit 0
fi
# Determine platform suffix for release asset
OS=$(uname -s)
ARCH=$(uname -m)
case "$OS-$ARCH" in
    Linux-x86_64)   PLATFORM="x86-linux" ;;
    Darwin-x86_64)  PLATFORM="x86-macos" ;;
    Darwin-arm64)   PLATFORM="arm64-macos" ;;
    Darwin-aarch64) PLATFORM="arm64-macos" ;;
    *)              echo "Unsupported platform: $OS-$ARCH"; exit 1 ;;
esac
# Query GitHub API for the pinned (or `latest`) release download URL.
if [ "$VERUS_VERSION" = "latest" ]; then
    API_URL="https://api.github.com/repos/verus-lang/verus/releases/latest"
    echo "Querying latest Verus release..."
else
    API_URL="https://api.github.com/repos/verus-lang/verus/releases/tags/$(python3 -c "import urllib.parse,sys;print(urllib.parse.quote(sys.argv[1],safe=''))" "$VERUS_VERSION")"
    echo "Querying pinned Verus release ${VERUS_VERSION}..."
fi
DOWNLOAD_URL=$(curl -fsSL "$API_URL" | python3 -c "import sys,json;[print(a['browser_download_url']) for a in json.load(sys.stdin)['assets'] if a['name'].endswith('-${PLATFORM}.zip')]" | head -1)
if [ -z "$DOWNLOAD_URL" ]; then
    echo "ERROR: No release asset found for platform $PLATFORM"
    exit 1
fi
echo "Downloading $DOWNLOAD_URL..."
# Phase 192.4 — honor TMPDIR instead of hardcoding /tmp.
ZIPFILE="${TMPDIR:-/tmp}/verus-${PLATFORM}.zip"
curl -fsSL "$DOWNLOAD_URL" -o "$ZIPFILE"
# Extract to tools/ (zip contains verus-<platform>/ directory)
TMPDIR=$(mktemp -d)
unzip -q "$ZIPFILE" -d "$TMPDIR"
mkdir -p "$VERUS_DIR"
cp -r "$TMPDIR"/verus-${PLATFORM}/* "$VERUS_DIR/"
rm -rf "$TMPDIR" "$ZIPFILE"
chmod +x "$VERUS_BIN" "$VERUS_DIR/cargo-verus" "$VERUS_DIR/z3" "$VERUS_DIR/rust_verify"
# Issue 0368 F6 — the binary is installed but may not RUN on this host's glibc;
# and a runnable one may still need its pinned rust toolchain installed.
# Triage before using it; degrade informatively instead of a raw loader crash.
verus_ready_or_fix
"$VERUS_BIN" --version
echo "Verus setup complete."
