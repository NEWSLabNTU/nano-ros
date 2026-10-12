#!/usr/bin/env bash
# Issue 1812 — `nros-node` serves `ros2 param` with NO heap.
#
# The parameter services used to need `alloc` (a `compile_error!` said so): the
# service state, one boxed processor per node and the request/reply buffer pair
# were heap allocations even over a store carved from the executor backing.
# They are carved now too, so `param-services` builds without `alloc` — but only
# a build that BUILDS it can say so, and no other lane does: every merge-gating
# build of `nros-node` turns `alloc` on through `std` or through workspace
# feature unification. A capability nothing builds is not a capability.
#
# Three rows, each a `cargo check` of `nros-node` alone, no `std`, no `alloc`:
#
#   1. NROS_PARAM_STORE=1 — the build KNOWS the image builds a store, so the
#      default sizing carves it and the services' home with it. Must compile.
#   1b. NROS_PARAM_STORE=1 NROS_CAPABILITY_PARAM_SERVICES=0 — a store the
#      image holds and does not serve (issue 1856): the home carves the state
#      alone. Must compile.
#   2. No store known — nothing to carve and no heap to fall back to. Must be
#      REFUSED at compile time, naming the knob. This row is the negative
#      control for row 1 as well: if it compiled, row 1 would be proving
#      nothing about the carve.
#
# Each row has its own target dir: they differ only in an env value that
# `nros-params`' build script watches, so sharing one would rebuild both crates
# on every alternation.
set -euo pipefail

cd "${NROS_REPO_DIR:-$(git rev-parse --show-toplevel)}"
# shellcheck source=scripts/build/cargo.sh
source scripts/build/cargo.sh
# shellcheck source=scripts/lib/grep-q.sh
source scripts/lib/grep-q.sh

FEATURES="rmw-cffi,param-services"
KNOB="NROS_PARAM_STORE=1"
# The refusal must name the knob that fixes it; this is the text it is held to.
REMEDY="NROS_PARAM_STORE=1"

# The verdict on the no-store row, from cargo's exit status and its output:
#   refused        -- failed, and the failure names the remedy (the pass)
#   compiled       -- built: a heap-free image that can never declare
#   wrong-refusal  -- failed for some OTHER reason, which proves nothing
refusal_verdict() {
    local rc="$1" log="$2"
    if [ "$rc" -eq 0 ]; then
        echo compiled
        return
    fi
    local named=0
    nros_grep_q -F "$REMEDY" "$log" || named=$?
    case "$named" in
        0) echo refused ;;
        *) echo wrong-refusal ;;
    esac
}

# The negative control for the verdict itself, on the NORMAL path: three
# synthetic outcomes whose right answers are known. Without it the grep could
# stop matching the real message and this gate would pass a build that failed
# for an unrelated reason.
selftest() {
    local tmp got
    tmp="$(mktemp -d)"
    trap 'rm -rf "$tmp"' RETURN
    printf 'error[E0080]: evaluation panicked: ... set %s (sized by ...)\n' "$REMEDY" >"$tmp/named"
    printf 'error[E0433]: cannot find module or crate `alloc`\n' >"$tmp/other"
    : >"$tmp/empty"
    for case in "1 named refused" "1 other wrong-refusal" "0 empty compiled"; do
        set -- $case
        got="$(refusal_verdict "$1" "$tmp/$2")"
        if [ "$got" != "$3" ]; then
            echo "check-param-services-heap-free SELFTEST FAILED: rc=$1 log=$2 -> $got, want $3" >&2
            return 1
        fi
    done
}

selftest

echo "  - nros-node: param-services without alloc, store carved ($KNOB)"
if ! CARGO_TARGET_DIR="$(nros_scoped_target_dir param-heap-free)" NROS_PARAM_STORE=1 \
        cargo check --quiet -p nros-node --no-default-features --features "$FEATURES"; then
    echo "check-param-services-heap-free: nros-node does not build \`param-services\` without \`alloc\`" >&2
    echo "  over a carved store. Something on the service path reached for the heap again" >&2
    echo "  (issue 1812): carve it from the executor backing (\`ParamHome\`) instead." >&2
    exit 1
fi

# Issue 1856 -- the store-only image: a store is known, and the build STATES
# the image serves no parameter services. Its home carries no sets and no
# buffer pair, and nothing on the store's path may reach for the heap either.
echo "  - nros-node: param-services without alloc, store carved, services stated off"
if ! CARGO_TARGET_DIR="$(nros_scoped_target_dir param-heap-free-store-only)" NROS_PARAM_STORE=1 \
        NROS_CAPABILITY_PARAM_SERVICES=0 \
        cargo check --quiet -p nros-node --no-default-features --features "$FEATURES"; then
    echo "check-param-services-heap-free: nros-node does not build a store-only image" >&2
    echo "  (NROS_CAPABILITY_PARAM_SERVICES=0) without \`alloc\` (issue 1856)." >&2
    exit 1
fi

echo "  - nros-node: param-services without alloc, no store known (must be refused)"
log="$(mktemp)"
trap 'rm -f "$log"' EXIT
rc=0
env -u NROS_PARAM_STORE CARGO_TARGET_DIR="$(nros_scoped_target_dir param-heap-free-nostore)" \
    cargo check --quiet -p nros-node --no-default-features --features "$FEATURES" \
    >"$log" 2>&1 || rc=$?
case "$(refusal_verdict "$rc" "$log")" in
    refused) ;;
    compiled)
        echo "check-param-services-heap-free: a heap-free build with no carved store COMPILED." >&2
        echo "  It can never declare a parameter, and must be refused at build time (issue 1812)." >&2
        exit 1
        ;;
    *)
        echo "check-param-services-heap-free: the no-store build failed, but not with the named" >&2
        echo "  refusal (it must name $REMEDY). Its output:" >&2
        cat "$log" >&2
        exit 1
        ;;
esac
echo "check-param-services-heap-free: selftest ok; param-services builds heap-free over a"
echo "  carved store, and is refused by name without one."
