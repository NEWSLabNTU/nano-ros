#!/usr/bin/env bash
#
# Provision mdBook — the book builder — as a pinned dev tool.
#
# `just book` needs it and nothing installed it, so the failure was
# `mdbook: command not found` at the END of a build that had already spent
# minutes on rustdoc and doxygen. A tool the build requires should be
# provisionable by the repo, not left to each host.
#
# PREBUILT BINARY, not `cargo install`: mdBook builds a large dependency tree,
# and a docs tool has no business costing a compile. Downloaded from the
# project's own GitHub releases and pinned, so two hosts render the same book.
#
# Install:  just setup-mdbook          (or: bash scripts/setup-mdbook.sh)
# Override: MDBOOK_VERSION=v0.5.4 bash scripts/setup-mdbook.sh
set -euo pipefail

MDBOOK_VERSION="${MDBOOK_VERSION:-v0.5.4}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TOOLS_DIR="$ROOT/tools"
BIN="$TOOLS_DIR/mdbook"
SUMS="$ROOT/scripts/mdbook-checksums.txt"

if [ -x "$BIN" ]; then
    # `mdbook --version` prints `mdbook v0.5.4`; the pin is `v0.5.4`. Strip the
    # program name AND the leading `v` from both, or the compare is
    # "v0.5.4" != "0.5.4" and the tool re-downloads on every single run while
    # reporting that it is replacing a version with itself.
    have="$("$BIN" --version 2>/dev/null || true)"
    have="${have#mdbook }"
    if [ "${have#v}" = "${MDBOOK_VERSION#v}" ]; then
        echo "mdbook ${MDBOOK_VERSION} already at $BIN"
        exit 0
    fi
    echo "mdbook at $BIN is '$have', want ${MDBOOK_VERSION} — replacing."
fi

# WHY musl ON BOTH LINUX ARCHES
#
# The release ships two Linux x86_64 builds, `-gnu` and `-musl`, and for aarch64
# only `-musl`. We take musl for both.
#
# A `-gnu` build is dynamically linked against the glibc of the machine that
# BUILT it, and glibc's symbols are versioned. A binary compiled against 2.39
# asks for `GLIBC_2.39` symbols, so it runs on a newer glibc and dies on an older
# one with `version \`GLIBC_2.xx\' not found` — at exec time, from the loader,
# with no useful context. That is not hypothetical here: `setup-verus.sh` had to
# grow a whole "does this actually run on this host?" triage path
# (`verus_glibc_degrade`) for exactly this, because its upstream ships gnu only.
#
# A `-musl` build is statically linked: libc is inside the binary, so there is no
# host glibc to be too old, and the same file runs on Ubuntu 20.04, a 2026 Arch
# box, and Alpine in CI. For a tool that only reads markdown and writes HTML, the
# static-linking trade-offs that matter elsewhere — NSS, dlopen, locale — do not
# apply.
#
# Taking musl on x86_64 too, rather than gnu-where-available, keeps ONE code path
# per OS. A per-arch split is the kind of asymmetry that works until the day the
# aarch64 runner is the one that breaks.
OS="$(uname -s)"
ARCH="$(uname -m)"
case "$OS-$ARCH" in
    Linux-x86_64)          TARGET="x86_64-unknown-linux-musl" ;;
    Linux-aarch64|Linux-arm64) TARGET="aarch64-unknown-linux-musl" ;;
    Darwin-x86_64)         TARGET="x86_64-apple-darwin" ;;
    Darwin-arm64|Darwin-aarch64) TARGET="aarch64-apple-darwin" ;;
    *)
        echo "ERROR: no mdBook release asset for $OS-$ARCH." >&2
        echo "       Assets exist for linux-musl (x86_64, aarch64) and" >&2
        echo "       apple-darwin (x86_64, aarch64). Build from source instead:" >&2
        echo "           cargo install mdbook --version ${MDBOOK_VERSION#v} --locked" >&2
        exit 1
        ;;
esac

ASSET="mdbook-${MDBOOK_VERSION}-${TARGET}.tar.gz"
URL="https://github.com/rust-lang/mdBook/releases/download/${MDBOOK_VERSION}/${ASSET}"

echo "Downloading ${ASSET}…"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
if ! curl -fsSL --retry 3 --max-time 300 "$URL" -o "$work/$ASSET"; then
    echo "ERROR: download failed: $URL" >&2
    echo "       If the tag moved, pass the one you want:" >&2
    echo "           MDBOOK_VERSION=vX.Y.Z bash scripts/setup-mdbook.sh" >&2
    exit 1
fi

# VERIFY before unpacking. The expected hash is GitHub's own published digest,
# committed to the repo — so this checks the bytes against what the release says
# it serves, and a tag re-cut under the same name fails LOUDLY instead of
# silently changing what everyone's book is built with.
WANT="$(awk -v a="$ASSET" '$2 == a {print $1}' "$SUMS" | head -1)"
if [ -z "$WANT" ]; then
    echo "ERROR: no checksum for $ASSET in $(basename "$SUMS")." >&2
    echo "       A pinned tool without a pinned hash is not pinned. Refresh with" >&2
    echo "       the command in that file's header, then re-run." >&2
    exit 1
fi
GOT="$(sha256sum "$work/$ASSET" | cut -d" " -f1)"
if [ "$GOT" != "$WANT" ]; then
    echo "ERROR: checksum mismatch for $ASSET" >&2
    echo "         expected  $WANT" >&2
    echo "         got       $GOT" >&2
    echo "       Do NOT install this. Either the release was re-cut under the same" >&2
    echo "       tag, or the download was tampered with." >&2
    exit 1
fi
echo "  sha256 ok"

# The tarball is a single `mdbook` at its root.
tar -xzf "$work/$ASSET" -C "$work"
if [ ! -f "$work/mdbook" ]; then
    echo "ERROR: $ASSET did not contain an 'mdbook' binary at its root." >&2
    ls -la "$work" >&2
    exit 1
fi

mkdir -p "$TOOLS_DIR"
install -m 0755 "$work/mdbook" "$BIN"

# Report the version the BINARY claims, not the one we asked for: the two differ
# whenever a tag is re-cut, and the printed line is the only evidence a reader
# has of which build is on disk (the Corrosion `via <origin>` lesson, issue 0500).
echo "installed $("$BIN" --version) at $BIN"
