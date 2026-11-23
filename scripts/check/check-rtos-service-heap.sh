#!/usr/bin/env bash
# check-rtos-service-heap: the FreeRTOS heap and the ThreadX byte pool grow with
# the nodes that serve the parameter services and the lifecycle machines
# (issue 2812).
#
# Both ports' defaults were measured on one-node images. Every further node
# serving the parameter services puts six servers' buffer pairs on the
# allocator, every further lifecycle machine five, so each road adds a term
# from the image's own counts (`cmake/NanoRosServiceHeap.cmake`'s formula, and
# its Rust twin `nros_sizing_descriptor::service_heap`).
#
#   1. The cargo road: both board builds add the descriptor-priced extra, and
#      the CLI states `[image] param_service_nodes`.
#   2. The cmake road: `nano_ros_entry` applies the extra, and the C defaults
#      carry it (`FreeRTOSConfig.h`, `threadx_hooks.c`, 0 when undefined).
#   3. Behaviour, on the real cmake function (run on every invocation): no
#      fragment, or one node and one machine, adds nothing; four nodes add
#      3 x 12 buffers; three machines add 2 x 10; and it still prices after the
#      first include ran inside a function frame.
set -euo pipefail
repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
mkdir -p "$repo/tmp"
work="$(mktemp -d "$repo/tmp/rtos-service-heap.XXXXXX")"
trap 'rm -rf "$work"' EXIT
fail=0
say() { echo "check-rtos-service-heap: $*"; }
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

# 1. The cargo road.
need "the ThreadX cargo road no longer adds the extra to the derived pool" \
    packages/boards/nros-board-common/src/threadx_sources.rs \
    'let derived = default_byte_pool_bytes(app_stack_bytes, tasks, rmw) + extra;'
need "the FreeRTOS cargo road no longer adds the extra to the derived heap" \
    packages/boards/nros-board-freertos/build.rs \
    '(fc::default_heap_bytes(app_stack_bytes) + extra) / 1024'
need "the CLI no longer states the parameter-serving node count" \
    packages/cli/nros-cli-core/src/sizing_descriptor.rs \
    'img.set_param_service_nodes(param_nodes.filter(|&n| n > 0));'
# 2. The cmake road.
need "nano_ros_entry no longer applies the extra" \
    cmake/NanoRosEntry.cmake \
    'nros_rtos_service_heap_apply(${_NRA_NAME} "${_nra_service_heap_fragment}")'
need "FreeRTOSConfig.h's zenoh default no longer carries the extra" \
    packages/boards/nros-board-freertos/config/nros_freertos_family_config.h \
    '((NROS_FREERTOS_ZENOH_HEAP_KB) + (NROS_FREERTOS_HEAP_RAISE_KB) +'
need "threadx_hooks.c's default pool no longer carries the extra" \
    packages/boards/nros-board-common/c/threadx_hooks.c \
    'NROS_THREADX_SERVICE_HEAP_EXTRA_BYTES)'

# 3. Behaviour.
selftest() {
    command -v cmake >/dev/null || { bad "cmake not on PATH"; return; }
    local probe="$work/probe.cmake" out
    # The first include runs INSIDE a function, as NanoRosEntry.cmake's does
    # when a workspace package imports it; the second is a guarded no-op. The
    # constants must survive the first frame.
    printf '%s\n' \
        'function(_first_include)' \
        "    include(\"$repo/cmake/NanoRosRtosServiceHeap.cmake\")" \
        'endfunction()' \
        '_first_include()' \
        "include(\"$repo/cmake/NanoRosRtosServiceHeap.cmake\")" \
        'nros_rtos_service_heap_extra("${FRAGMENT}" "${PORT}" _e _c)' \
        'message("EXTRA=[${_e}]")' > "$probe"
    case_() { # <name> <port> <nodes|-> <machines|-> <want>   (nodes "none": no fragment)
        local name=$1 port=$2 nodes=$3 machines=$4 want=$5 frag="$work/$1.cmake"
        if [ "$nodes" = none ]; then
            frag="$work/absent.cmake"
        else
            : > "$frag"
            [ "$nodes" = - ] || printf 'set(NROS_ENTITY_PARAM_SERVICE_NODES %s)\n' "$nodes" >> "$frag"
            [ "$machines" = - ] || printf 'set(NROS_ENTITY_LIFECYCLE_MACHINES %s)\n' "$machines" >> "$frag"
        fi
        local r=0
        out="$(cmake "-DFRAGMENT=$frag" "-DPORT=$port" -P "$probe" 2>&1)" || r=$?
        if [ "$r" -ne 0 ]; then bad "$name: the probe failed: $out"; return; fi
        case "$out" in
            *"EXTRA=[$want]"*) ;;
            *) bad "$name: expected extra [$want], got: $out" ;;
        esac
    }
    case_ no-fragment THREADX none - 0
    case_ no-counts FREERTOS - - 0
    case_ one-node THREADX 1 1 0
    case_ zero-nodes FREERTOS 0 0 0
    # Remainders are 0 until measured, so the term is the buffers alone.
    case_ four-nodes-threadx THREADX 4 1 $((3 * 12 * 4096))
    case_ four-nodes-freertos FREERTOS 4 0 $((3 * 12 * 4096))
    case_ three-machines THREADX 1 3 $((2 * 10 * 4096))
    case_ both FREERTOS 2 2 $((12 * 4096 + 10 * 4096))
}
selftest

if [ "$fail" -ne 0 ]; then
    exit 1
fi
say "OK — both RTOS allocators add the per-node service term on both roads, and a one-node image adds nothing."
