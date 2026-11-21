#!/usr/bin/env bash
# phase-583 W3 (RFC-0071 D20) -- a configure VERDICT for the `nros` CLI's
# refusal of an owned-entry bringup whose launch file starts a node from a
# package that is no owned entry. The refusal comes from `nros build`, before
# cmake runs, so no cmake-configure-verdict row can record it; this
# `fixture-script` row does, at build time (tests never run `nros build`).
#
#   build.sh <out-dir>
#
# Into <out-dir>: `exit` (the CLI's exit status) and `stderr` (what it said).
# Exits 0 either way: the recorded verdict is the artifact; the test judges it.
set -euo pipefail
out="$1"
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
rm -rf "$out/ws"
cp -r "$here/ws" "$out/ws"
rc=0
(cd "$out/ws" && nros build foreign_launch_bringup:linux) >"$out/stdout" 2>"$out/stderr" || rc=$?
printf '%s\n' "$rc" >"$out/exit"
