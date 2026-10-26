#!/usr/bin/env bash
# Issue 2043 (phase-518 W3) — a ported ament workspace is sized from its own
# program, and no pool the census answers is left at its embedded default.
#
# Builds `packages/testing/nros-tests/bins/ament-census-port`: a stock
# ament_cmake node with six subscriptions, six publishers and two timers, on
# the add_subdirectory road, whose `system.toml` states `entities = "census"`
# and no size. Its configure takes the census (a host build of the same
# sources, run once) and writes the sizing descriptor the runtime crates read.
# The check then asserts, from the build's own outputs:
#
#   1. the descriptor STATES the image-wide counts the census answers
#      (`subscriber_count`, `publisher_count`, `callback_slots`), each at
#      least what the node creates;
#   2. nros-node's generated `MAX_CBS` is the descriptor's `callback_slots`
#      and not the default its own build script prints beside it.
#
# Every sizing knob a user could set is UNSET for the build, so a green run
# cannot have been sized by the caller's environment.
#
# `--self-test` runs the verdict (step 1 and 2) over synthetic outputs, both
# directions; the normal path runs it first.
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
leaf="$repo/packages/testing/nros-tests/bins/ament-census-port"
build="$leaf/build/linux"
logs="$leaf/build"

# verify <descriptor> <nros_node_config.rs>
# Prints the reason and returns 1 when a pool the census answers is unsized.
verify() {
    local desc="$1" cfg="$2" f subs pubs cbs max_cbs default_cbs
    [ -f "$desc" ] || { echo "the configure wrote no sizing descriptor at $desc"; return 1; }
    field() { sed -n "s/^$1 = \([0-9][0-9]*\)$/\1/p" "$desc" | head -1; }
    for f in subscriber_count publisher_count callback_slots; do
        [ -n "$(field "$f")" ] || { echo "the descriptor does not state \`$f\` (refused or absent): $desc"; return 1; }
    done
    subs="$(field subscriber_count)"
    pubs="$(field publisher_count)"
    cbs="$(field callback_slots)"
    [ "$subs" -ge 6 ] || { echo "subscriber_count = $subs, the node creates 6"; return 1; }
    [ "$pubs" -ge 6 ] || { echo "publisher_count = $pubs, the node creates 6"; return 1; }
    [ "$cbs" -ge 8 ] || { echo "callback_slots = $cbs, the node creates 8 callbacks"; return 1; }
    [ -f "$cfg" ] || { echo "no nros-node build output ($cfg)"; return 1; }
    max_cbs="$(sed -n 's/^pub const MAX_CBS: usize = \([0-9][0-9]*\);$/\1/p' "$cfg")"
    default_cbs="$(sed -n 's/.*NROS_EXECUTOR_MAX_CBS, default \([0-9][0-9]*\)).*/\1/p' "$cfg" | head -1)"
    if [ -z "$max_cbs" ] || [ -z "$default_cbs" ]; then
        echo "cannot read MAX_CBS and its default from $cfg"
        return 1
    fi
    [ "$max_cbs" = "$cbs" ] || { echo "nros-node built MAX_CBS = $max_cbs, the descriptor states $cbs ($cfg)"; return 1; }
    [ "$max_cbs" != "$default_cbs" ] || { echo "nros-node built MAX_CBS at its default ($default_cbs)"; return 1; }
    echo "subscribers $subs, publishers $pubs, callback slots $cbs; MAX_CBS $max_cbs, default $default_cbs"
}

self_test() {
    local t rc=0 out
    t="$(mktemp -d)"
    # shellcheck disable=SC2064
    trap "rm -rf '$t'" RETURN
    printf 'subscriber_count = 6\npublisher_count = 7\ncallback_slots = 14\n' >"$t/ok.toml"
    printf '/// Maximum number of executor callback slots (set via NROS_EXECUTOR_MAX_CBS, default 4).\npub const MAX_CBS: usize = 14;\n' >"$t/ok.rs"
    printf '/// (set via NROS_EXECUTOR_MAX_CBS, default 4).\npub const MAX_CBS: usize = 4;\n' >"$t/default.rs"
    printf 'subscriber_count = 6\npublisher_count = 7\n[image.refused]\ncallback_slots = "refused"\n' >"$t/refused.toml"
    out="$(verify "$t/ok.toml" "$t/ok.rs")" || { echo "self-test: a sized build was refused: $out" >&2; rc=1; }
    if out="$(verify "$t/ok.toml" "$t/default.rs")"; then
        echo "self-test: MAX_CBS at its default was accepted" >&2
        rc=1
    fi
    if out="$(verify "$t/refused.toml" "$t/ok.rs")"; then
        echo "self-test: a refused callback_slots was accepted" >&2
        rc=1
    fi
    if out="$(verify "$t/missing.toml" "$t/ok.rs")"; then
        echo "self-test: a missing descriptor was accepted" >&2
        rc=1
    fi
    [ "$rc" -eq 0 ] && echo "check-ament-census-sizing: self-test OK (sized accepted; default, refused and missing rejected)"
    return "$rc"
}

fail() {
    echo "check-ament-census-sizing: FAILED — $*" >&2
    exit 1
}

case "${1:-}" in
    --self-test) self_test; exit $? ;;
    "") ;;
    *) echo "usage: $0 [--self-test]" >&2; exit 2 ;;
esac
self_test || exit 1

# shellcheck source=scripts/build/cargo.sh
source "$repo/scripts/build/cargo.sh"
# shellcheck source=scripts/build/copy-out-cmake.sh
source "$repo/scripts/build/copy-out-cmake.sh"
nros=""
if ! nros="$(nros_cli_bin --checkout-only)"; then
    fail "this checkout has no built nros CLI (run \`just setup-cli\`)"
fi
cli_path="$(dirname "$nros"):$PATH"

knobs=()
for v in $(env | sed -n 's/^\(NROS_\(EXECUTOR\|ZENOH\|RMW\|MAX\)_[A-Z_]*\)=.*/\1/p'); do
    knobs+=("$v")
done

mkdir -p "$build"
echo "check-ament-census-sizing: configure + build $leaf (census taken by the configure)"
if ! (
    [ "${#knobs[@]}" -eq 0 ] || unset "${knobs[@]}"
    export PATH="$cli_path"
    nros_copy_out_cmake_configure -S "$leaf" -B "$build" -G Ninja
) >"$logs/linux.configure.log" 2>&1; then
    tail -40 "$logs/linux.configure.log" >&2
    fail "configure failed (log: $logs/linux.configure.log)"
fi
if ! (
    [ "${#knobs[@]}" -eq 0 ] || unset "${knobs[@]}"
    export PATH="$cli_path"
    cmake --build "$build"
) >"$logs/linux.build.log" 2>&1; then
    grep -m20 -E 'error|Error' "$logs/linux.build.log" >&2 || true
    fail "build failed (log: $logs/linux.build.log)"
fi

cfg=""
if ! cfg="$(find "$build/cargo" -path '*/build/nros-node-*/out/nros_node_config.rs' | head -1)"; then
    fail "cannot search $build/cargo for the nros-node build output"
fi
verdict=""
if ! verdict="$(verify "$build/nros/sizing/ament_census_port.toml" "$cfg")"; then
    fail "$verdict"
fi
echo "check-ament-census-sizing: OK ($verdict)"
