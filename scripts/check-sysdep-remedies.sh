#!/usr/bin/env bash
# phase-327 W3 (RFC-0062, issue 0368) — remedy text derives from the index.
#
# The dependency SSoT is nros-sdk-index.toml: `nros setup --system` composes
# the native install command for the HOST's package manager, and doctors
# print entry-derived remedies. A hand-written `sudo apt …` line in a just
# recipe re-creates the drift class 0368 measured (three remedies pointed at
# apt/sudo where an index prebuilt existed) and is Debian-only besides.
#
# Scope: just/*.just + the root justfile. Shell scripts under scripts/ may
# keep a distro-labelled fallback line for the no-CLI bootstrap path.
set -euo pipefail
cd "$(dirname "$0")/.."

hits=$(grep -n 'sudo apt' just/*.just justfile 2>/dev/null || true)
if [ -n "$hits" ]; then
    echo "ERROR: hand-written 'sudo apt' remedy in a just recipe — declare the"
    echo "package in nros-sdk-index.toml [system.*] and point the remedy at"
    echo "'nros setup --system' instead (phase-327 W3 / issue 0368):"
    echo "$hits" | sed 's/^/  /'
    exit 1
fi
echo "sysdep remedies OK (no hand-written 'sudo apt' in just recipes)"
