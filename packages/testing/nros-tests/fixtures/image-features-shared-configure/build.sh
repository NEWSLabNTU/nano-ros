#!/usr/bin/env bash
# Issue 2702 (interim) -- a configure VERDICT for the `nros` CLI's refusal of a
# C/C++ workspace image whose own `node_interfaces` differs from `[system]`
# on a shared cmake configure. The refusal is the CLI's, before cmake runs, so
# this `fixture-script` row records it at build time (tests never build).
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
(cd "$out/ws" && nros build probe_bringup:slim) >"$out/stdout" 2>"$out/stderr" || rc=$?
printf '%s\n' "$rc" >"$out/exit"
