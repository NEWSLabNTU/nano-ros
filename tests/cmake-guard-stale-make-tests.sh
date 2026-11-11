#!/bin/bash
# tests/cmake-guard-stale-make-tests.sh -- issue 2421
#
# `nros_cmake_guard_stale_make` (cmake-cache-guard.sh rule 5) hands a build
# dir to `nros dev jobserver repair-makes` when this unit was given a FIFO
# jobserver, so a cached make older than 4.4 is pointed at the make on PATH
# before the build runs it. The rewrite itself is tested in Rust
# (`builder::jobserver` tests); this checks the shell half against a stub CLI
# named by `$NROS_CLI` (rung 1 of `nros_cli_bin`):
#
#   1. outside a FIFO pool (no `--jobserver-auth=fifo:` in MAKEFLAGS) the CLI
#      is not asked;
#   2. under one it is asked, with the build dir;
#   3. a CLI that cannot answer (a stale binary without the verb) is reported
#      by name, and the guard still returns 0;
#   4. a make leaf's shape (a fresh bash holding only the `export -f`'d
#      function, `nros_cli_bin` undefined) still reaches the CLI.
# Case 2 runs in a shell that sourced only the guard, not cargo.sh, which is
# how `zephyr-fixture-run-one.sh` reaches it.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
fail=0
ok() { echo "  ok    $1"; }
bad() { echo "  FAIL  $1" >&2; fail=1; }

mkdir -p "$WORK/bin" "$WORK/build"
: > "$WORK/build/CMakeCache.txt"
calls="$WORK/calls"
stub="$WORK/bin/nros"
printf '#!/bin/sh\necho "$*" >> "%s"\nexit "${STUB_RC:-0}"\n' "$calls" > "$stub"
chmod +x "$stub"

guard() {
    # A fresh shell each time, so `nros_cli_bin` is resolved the way a unit
    # resolves it, not from this script's state.
    env -u NROS_JOBSERVER NROS_CLI="$stub" MAKEFLAGS="$1" STUB_RC="${2:-0}" \
        bash -c '. "$0/scripts/build/cmake-cache-guard.sh" && nros_cmake_guard_stale_make "$1"' \
        "$ROOT" "$WORK/build" 2>"$WORK/stderr"
}

: > "$calls"
guard "-j8 --jobserver-auth=3,4"
if [ -s "$calls" ]; then bad "1: a pipe jobserver asked the CLI: $(cat "$calls")"
else ok "1: outside a FIFO pool the CLI is not asked"; fi

: > "$calls"
guard "-j8 --jobserver-auth=fifo:/tmp/x"
if [ "$(cat "$calls")" = "dev jobserver repair-makes $WORK/build" ]; then
    ok "2: under a FIFO pool the CLI is asked with the build dir"
else bad "2: expected one repair-makes call, got: $(cat "$calls")"; fi

: > "$calls"
rc=0; guard "-j8 --jobserver-auth=fifo:/tmp/x" 2 || rc=$?
if [ "$rc" -eq 0 ] && [ -s "$calls" ] && grep -q "issue 2421" "$WORK/stderr"; then
    ok "3: a CLI that cannot answer is reported, and the guard returns 0"
else bad "3: rc=$rc, calls: $(cat "$calls"), stderr: $(cat "$WORK/stderr")"; fi

: > "$calls"
# The make-leaf shape: export the function, then run it in a bash that never
# sourced cargo.sh, with only the exported `_NROS_CACHE_GUARD_CARGO_SH`.
env -u NROS_JOBSERVER NROS_CLI="$stub" MAKEFLAGS="--jobserver-auth=fifo:/tmp/x" \
    bash -c '. "$0/scripts/build/cmake-cache-guard.sh"
             export -f nros_cmake_guard_stale_make
             env -u BASH_ENV bash -c "nros_cmake_guard_stale_make \"$1\""' \
    "$ROOT" "$WORK/build" 2>"$WORK/stderr" || true
if [ "$(cat "$calls")" = "dev jobserver repair-makes $WORK/build" ]; then
    ok "4: an export -f copy in a fresh bash reaches the CLI"
else bad "4: got: $(cat "$calls"); stderr: $(cat "$WORK/stderr")"; fi

if [ "$fail" -ne 0 ]; then
    echo "cmake-guard-stale-make: FAILED" >&2
    exit 1
fi
echo "cmake-guard-stale-make: OK (4 cases)"
