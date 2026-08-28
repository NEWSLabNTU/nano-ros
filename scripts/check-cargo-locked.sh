#!/usr/bin/env bash
#
# Issues 0359 / 0378 — the project-wide cargo flag injection must stay wired.
#
# WHAT THIS REPLACED, AND WHY
#
# The first version of this gate counted cargo invocations missing `--locked`
# (57 of them) and froze that as a shrink-only baseline. That was the wrong
# shape: it asked 57 call sites to each remember a flag, and it could never
# cover the ones that matter most — cmake and corrosion invoke `cargo` BY NAME
# (`cmake/NanoRosGenerateInterfaces.cmake:499`), so no justfile variable or
# shell helper reaches them.
#
# The flags are now defined ONCE (`NROS_CARGO_FLAGS` in `activate.sh`) and
# injected by a PATH shim (`scripts/bin/cargo`) — the same mechanism the
# project already uses to wire `nros`, `play_launch_parser` and `zenohd`.
#
# So there is nothing to count any more. What has to hold is that the
# mechanism is present and reachable, which is what this checks. If someone
# deletes the shim or drops the PATH line, every build silently goes back to
# rewriting `Cargo.lock` on a manifest mismatch — the exact failure that
# produced issue 0359, where locks did not drift on their own: the builds
# rewrote them.

set -euo pipefail
cd "$(dirname "$0")/.."

# issue 0726 — all three conditionals below are `if ! grep -q`, so a grep that
# failed to START would announce that activation stopped wiring the cargo shim
# and send the reader after a mechanism that is intact. `nros_grep_q` exits 2.
# shellcheck source=scripts/lib/grep-q.sh
source scripts/lib/grep-q.sh

status=0
shim="scripts/bin/cargo"

if [ ! -f "$shim" ]; then
    echo "[FAIL] $shim is missing — cargo flag injection is gone." >&2
    status=1
elif [ ! -x "$shim" ]; then
    echo "[FAIL] $shim is not executable, so PATH will skip it silently." >&2
    status=1
fi

if ! nros_grep_q 'scripts/bin' activate.sh; then
    echo "[FAIL] activate.sh no longer puts scripts/bin on PATH." >&2
    status=1
fi

if ! nros_grep_q 'NROS_CARGO_FLAGS' activate.sh; then
    echo "[FAIL] activate.sh no longer defines NROS_CARGO_FLAGS." >&2
    status=1
fi

# The shim must default to --locked. An empty default would disable the whole
# mechanism while leaving every file in place, which is the failure mode most
# likely to pass a casual review.
# The old `2>/dev/null` existed for ONE case — a missing `scripts/bin/cargo`,
# already reported above as its own FAIL — and it also hid every other grep
# error. Name that case instead of muting the channel: search the shim only
# when it is there, so an absent shim still reaches the same verdict it always
# did while a grep that could not run is fatal.
locked_sources=(activate.sh)
[ -f "$shim" ] && locked_sources+=("$shim")
if ! nros_grep_q -E 'NROS_CARGO_FLAGS[:-]?="?--locked' "${locked_sources[@]}"; then
    echo "[FAIL] neither activate.sh nor $shim defaults NROS_CARGO_FLAGS to --locked." >&2
    status=1
fi

if [ "$status" -ne 0 ]; then
    echo "" >&2
    echo "  Without this, \`cargo build\` REWRITES Cargo.lock when a manifest" >&2
    echo "  no longer matches it, instead of failing — verified behaviour, not" >&2
    echo "  a theory. Deliberate dependency changes go through:" >&2
    echo "      just lock-update [crate] [version] [dir]" >&2
    exit 1
fi

echo "cargo flag injection OK — scripts/bin/cargo wired, default --locked."
