#!/usr/bin/env bash
# tests/zephyr-conf-capability-refusal-tests.sh -- issue 2786.
#
# WHAT THIS MEASURES
#
# The build sets the node-interface capability symbols
# (CONFIG_NROS_CAPABILITY_{PARAM_SERVICES,ROSOUT,SIM_TIME}) from the image's
# `node_interfaces` / `features` (`nros build` on the command line, a leaf in
# its rendered fragment), merged after every conf file. A conf fragment that set
# CONFIG_NROS_CAPABILITY_ROSOUT=n was therefore overridden to y without a
# word. The module's conf gate (`_nros_image_kconfig_refuse_conf_knobs`,
# zephyr/cmake/nros_image_kconfig.cmake) now refuses it, naming the owning
# `system.toml` key, the same way it refuses a registry-owned knob.
#
#   A  a fragment setting CONFIG_NROS_CAPABILITY_ROSOUT=n is refused, and the
#      message names the file, the line and `node_interfaces`
#   B  the same for SIM_TIME and PARAM_SERVICES
#   C  a fragment with only Zephyr-native symbols configures unchanged
#   D  a registry knob is still refused with its `[image.<id>.knobs...]`
#      spelling (the gate it joins is untouched)
#   E  the module's symbol list matches
#      `nros_orchestration_ir::node_interfaces::STOCK`
#
# A probe project that includes the module file, so seconds; no Zephyr
# workspace. PRECONDITIONS ARE HARD FAILURES (issue 1758): no cmake, red.
#
# Usage: ./tests/zephyr-conf-capability-refusal-tests.sh
# Exit:  0 all assertions held; 1 otherwise.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
MODULE="$PROJECT_ROOT/zephyr/cmake/nros_image_kconfig.cmake"
STOCK_SRC="$PROJECT_ROOT/packages/core/nros-orchestration-ir/src/node_interfaces.rs"

FAILURES=0
fail() {
    echo "FAIL: $*" >&2
    FAILURES=$((FAILURES + 1))
}

command -v cmake >/dev/null 2>&1 || { echo "FAIL: cmake not found" >&2; exit 1; }

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
mkdir -p "$WORK/src"
printf '%s\n' \
    'cmake_minimum_required(VERSION 3.22)' \
    'project(probe NONE)' \
    'include("${MODULE}")' \
    '_nros_image_kconfig_refuse_conf_knobs("${FRAG}" "${WORK}/system.toml" "fvp")' \
    'message(STATUS "PROBE_CONFIGURED")' \
    > "$WORK/src/CMakeLists.txt"

# configure <fragment-body>: prints cmake's output; returns cmake's status.
configure() {
    printf '%s\n' "$1" > "$WORK/frag.conf"
    rm -rf "$WORK/b"
    cmake -S "$WORK/src" -B "$WORK/b" "-DMODULE=$MODULE" "-DWORK=$WORK" \
        "-DFRAG=$WORK/frag.conf" 2>&1
}

for sym in ROSOUT SIM_TIME PARAM_SERVICES; do
    if out="$(configure "CONFIG_NROS_CAPABILITY_${sym}=n")"; then
        fail "A/B: a fragment setting CONFIG_NROS_CAPABILITY_${sym}=n configured"
        continue
    fi
    flat="$(printf '%s' "$out" | tr -s ' \n' ' ')"
    case "$flat" in
        *"frag.conf: CONFIG_NROS_CAPABILITY_${sym}=n"*) ;;
        *) fail "A/B: the ${sym} refusal does not name the file and line: $flat" ;;
    esac
    case "$flat" in
        *'`node_interfaces` in system.toml owns it'*) ;;
        *) fail "A/B: the ${sym} refusal does not name node_interfaces: $flat" ;;
    esac
done

if ! out="$(configure 'CONFIG_NET_IPV4=y
CONFIG_MAIN_STACK_SIZE=8192')"; then
    fail "C: a fragment with Zephyr-native symbols only was refused: $out"
elif [[ "$out" != *PROBE_CONFIGURED* ]]; then
    fail "C: the plain configure did not finish: $out"
fi

if out="$(configure 'CONFIG_NROS_ACTION_INBOX_DEPTH=4')"; then
    fail "D: a registry knob in a fragment configured"
elif [[ "$(printf '%s' "$out" | tr -s ' \n' ' ')" != *'in system.toml, or -DNROS_OVERRIDE_ACTION_INBOX_DEPTH=4'* ]]; then
    fail "D: the registry-knob refusal lost its spelling: $out"
fi

stock="$(sed -n 's/^pub const STOCK: &\[&str\] = &\[\(.*\)\];$/\1/p' "$STOCK_SRC" |
    tr -d '" ' | tr ',' '\n' | tr '[:lower:]' '[:upper:]' | sed 's/^/NROS_CAPABILITY_/' | sort)"
mine="$(sed -n '/set(_node_interface_syms/,/)/p' "$MODULE" |
    grep -o 'NROS_CAPABILITY_[A-Z_]*' | sort)"
[ -n "$stock" ] || fail "E: could not read STOCK from $STOCK_SRC"
[ "$stock" = "$mine" ] ||
    fail "E: the module refuses [$(echo $mine)] but STOCK is [$(echo $stock)]"

if [ "$FAILURES" -ne 0 ]; then
    echo "zephyr-conf-capability-refusal: $FAILURES failure(s)" >&2
    exit 1
fi
echo "zephyr-conf-capability-refusal: OK (a conf fragment cannot set a node-interface capability)"
