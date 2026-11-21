#!/usr/bin/env bash
# check-zephyr-heap-param-floor: a C/C++ Zephyr image's platform heap scales
# with the nodes that serve the parameter services (issue 2780, fix (b)).
#
# Each node carrying the parameter services runs six servers, each owning a
# PARAM_SERVICE_BUFFER_SIZE request/reply pair on the platform heap. The flat
# 112 KiB Kconfig arm was priced on one node, and a four-node image ran it dry
# at boot. The floor is derived now, in zephyr/cmake/nros_heap_floor.cmake.
#
#   1. Kconfig's C/C++ param_services arm is the -1 derive sentinel, and the
#      registry's generated zephyr/Kconfig agrees.
#   2. The Zephyr resolver prices the knob through `_nros_zephyr_heap_floor`
#      and the derivable ladder, with the one-node fallback.
#   3. The CLI publishes NROS_ENTITY_PARAM_SERVICE_NODES, the count the floor reads.
#   4. Behaviour, on the real function (the selftest, run on every invocation):
#      no count or zero nodes derives nothing (the caller falls back to 112 KiB),
#      one node derives exactly 112 KiB (a one-node image does not grow), four
#      nodes derive 112 KiB + 3 x 49,152, and a stated buffer scales the
#      per-node term.
set -euo pipefail
repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
mkdir -p "$repo/tmp"
work="$(mktemp -d "$repo/tmp/zephyr-heap-param-floor.XXXXXX")"
trap 'rm -rf "$work"' EXIT
fail=0
say() { echo "check-zephyr-heap-param-floor: $*"; }
bad() { say "FAIL: $*"; fail=1; }
# shellcheck source=../lib/grep-q.sh
. "$repo/scripts/lib/grep-q.sh"

need() { # <what> <file> <fixed text>
    local r=0
    nros_grep_q -F -- "$3" "$repo/$2" || r=$?
    case $r in
        0) ;;
        1) bad "$1 ($2 no longer says: $3)" ;;
        *) exit 2 ;;
    esac
}

# 1-3. The wiring.
for arm in NROS_CAPABILITY_LIFECYCLE 'RUST && NROS_CAPABILITY_PARAM_SERVICES' NROS_CAPABILITY_PARAM_SERVICES; do
    need "the \`$arm\` arm is not the derive sentinel" \
        packages/platform/nros-platform-zephyr/nros-knobs.toml "\"default -1 if $arm\","
done
need "the generated Kconfig does not carry the derive sentinel" \
    zephyr/Kconfig 'default -1 if NROS_CAPABILITY_PARAM_SERVICES'
need "the resolver does not price the heap from the node count" \
    zephyr/cmake/nros_cargo_build.cmake '_nros_zephyr_heap_floor("${_nros_heap_base}" "${_nros_heap_buf}"'
need "the resolver lost the one-node fallback" \
    zephyr/cmake/nros_cargo_build.cmake '_nros_resolve_knob(NROS_ZEPHYR_HEAP_SIZE "${_nros_heap_base}" default)'
need "the Rust lane no longer receives the configure-decided heap" \
    zephyr/cmake/nros_cargo_build.cmake 'AND NOT "${NROS_KNOB_SOURCE_${_knob}}" STREQUAL "default")'
need "the Zephyr floor no longer uses the shared per-node arithmetic" \
    zephyr/cmake/nros_heap_floor.cmake '_nros_service_heap_extra("${_n}" "${_m}" "${buffer}"'
need "the CLI no longer publishes the node count" \
    packages/cli/nros-cli-core/src/entity_inventory.rs '"set(NROS_ENTITY_PARAM_SERVICE_NODES {})\n"'
need "the CLI no longer publishes the lifecycle machine count" \
    packages/cli/nros-cli-core/src/entity_inventory.rs '"set(NROS_ENTITY_LIFECYCLE_MACHINES {})\n"'

# 4. Behaviour.
selftest() {
    command -v cmake >/dev/null || { bad "cmake not on PATH"; return; }
    local probe="$work/probe.cmake" out
    printf '%s\n' \
        "include(\"$repo/zephyr/cmake/nros_heap_floor.cmake\")" \
        '_nros_zephyr_heap_class_base("${LC}" "${RUST}" "${PARAMS}" _b)' \
        '_nros_zephyr_heap_floor("${_b}" "${BUF}" _f _c)' \
        'message("BASE=[${_b}] FLOOR=[${_f}]")' > "$probe"
    case_() { # <name> <lifecycle> <rust> <params> <nodes|-> <machines|-> <buffer> <want>
        local name=$1 lc=$2 rust=$3 params=$4 nodes=$5 machines=$6 buf=$7 want=$8 defs=()
        [ "$nodes" = - ] || defs+=("-DNROS_ENTITY_PARAM_SERVICE_NODES=$nodes")
        [ "$machines" = - ] || defs+=("-DNROS_ENTITY_LIFECYCLE_MACHINES=$machines")
        local r=0
        out="$(cmake "${defs[@]}" "-DLC=$lc" "-DRUST=$rust" "-DPARAMS=$params" "-DBUF=$buf" \
            -P "$probe" 2>&1)" || r=$?
        if [ "$r" -ne 0 ]; then bad "$name: the probe failed: $out"; return; fi
        case "$out" in
            *"FLOOR=[$want]"*) ;;
            *) bad "$name: expected floor [$want], got: $out" ;;
        esac
    }
    local p=$((12 * 4096)) l=$((10 * 4096))
    #     name                                   lc rust params nodes mach buf  want
    case_ "C: no inventory count"                "" "" y - - "" ""
    case_ "C: zero parameter-serving nodes"      "" "" y 0 0 "" ""
    case_ "C: one node keeps 112 KiB"            "" "" y 1 0 "" 114688
    case_ "C: four nodes"                        "" "" y 4 0 "" $((114688 + 3 * p))
    case_ "C: three nodes, 2048-byte buffer"     "" "" y 3 0 2048 $((114688 + 2 * 12 * 2048))
    case_ "C: a non-number count"                "" "" y abc 0 "" ""
    case_ "Rust: one node keeps 192 KiB"         "" y y 1 0 "" 196608
    case_ "Rust: two nodes"                      "" y y 2 0 "" $((196608 + p))
    case_ "lifecycle: one node, one machine"     y y y 1 1 "" 196608
    case_ "lifecycle: three nodes, two machines" y "" y 3 2 "" $((196608 + 2 * p + l))
    case_ "no family: no base, no floor"         "" "" "" 2 0 "" ""
    # A first include from inside a function frame must not lose the
    # constants: the file is include_guard(GLOBAL), so a later include is a
    # no-op and the arithmetic would run on empty values.
    printf '%s\n' \
        'function(_first_include)' \
        "  include(\"$repo/zephyr/cmake/nros_heap_floor.cmake\")" \
        'endfunction()' \
        '_first_include()' \
        "include(\"$repo/zephyr/cmake/nros_heap_floor.cmake\")" \
        'set(NROS_ENTITY_PARAM_SERVICE_NODES 3)' \
        '_nros_service_heap_extra(3 0 "" 0 0 _e _c)' \
        'message("EXTRA=[${_e}]")' > "$probe"
    local r=0
    out="$(cmake -P "$probe" 2>&1)" || r=$?
    case "$r:$out" in
        "0:"*"EXTRA=[$((2 * p))]"*) ;;
        *) bad "a first include inside a function lost the constants: $out" ;;
    esac
}
selftest

if [ "$fail" -ne 0 ]; then
    exit 1
fi
say "OK (sentinel arms, resolver wiring, CLI counts; the floor behaves for C, Rust and lifecycle images)"
