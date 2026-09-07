#!/usr/bin/env bash
# phase-418 418.2 — `nvidia-ivc --features fsp` compiles, when the FSP tree is here.
#
# The IVC driver has three configurations and they reach different lanes:
#
#   default stub  the `--workspace --no-default-features` test-compile in
#                 `check-workspace-features` (measured; both crates are members)
#   unix-mock     `check-required-features-tests` (issue 0652)
#   fsp           THIS gate, and nothing else
#
# `fsp` is the one that talks to the real hardware, and it is the one nothing
# built. Its build script REFUSES rather than degrading when the vendor tree is
# absent -- `NV_SPE_FSP_DIR` must point at a directory holding
# `lib/libtegra_aon_fsp.a` -- so on an unprovisioned host this cannot be a
# compile at all.
#
# It is therefore a LEDGERED skip, the same shape `check-sched-dim-arms-compile`
# uses for `NUTTX_DIR` / `THREADX_DIR`: the lane summary counts it, so "not
# checked" survives into the output instead of reading as a pass. Issue 1184 is
# what that rule cost when a skip stayed uncounted.
#
# Provisioning the tree is 418.4's item. Until it lands this gate reports the
# skip on every host; once it lands, the same gate starts compiling with no
# change here.
set -uo pipefail
cd "$(dirname "$0")/.."
repo_root="$(pwd)"

# shellcheck source=scripts/build/check-skip.sh
. "$repo_root/scripts/build/check-skip.sh"

# The three states are a decision about a PATH, and the decision is what can be
# tested without a cross compile. `classify_fsp_dir` is that decision; `main`
# acts on it. Split so the self-test below is a real negative control rather
# than a comment -- it proves the gate can still tell the three apart, which is
# the only way "SKIPPED" keeps meaning "not provisioned" instead of "not
# looked at".
#
#   unprovisioned  the variable is unset      -> ledgered SKIP
#   badpath        set, but no FSP library    -> FAILURE (someone meant to
#                                                point at a tree and missed)
#   provisioned    set, library present       -> compile
classify_fsp_dir() {
    local dir="$1"
    if [ -z "$dir" ]; then
        echo unprovisioned
    elif [ ! -f "$dir/lib/libtegra_aon_fsp.a" ]; then
        echo badpath
    else
        echo provisioned
    fi
}

self_test() {
    local failures=0 got
    _case() {
        got="$(classify_fsp_dir "$2")"
        if [ "$got" = "$3" ]; then
            printf '  %-46s ok\n' "$1"
        else
            printf '  %-46s FAILED (got %s, want %s)\n' "$1" "$got" "$3"
            failures=$((failures + 1))
        fi
    }
    local tmp
    tmp="$(mktemp -d)"
    _case "unset -> ledgered skip" "" unprovisioned
    _case "set but empty dir -> failure, not a skip" "$tmp" badpath
    mkdir -p "$tmp/lib"
    _case "set, dir exists, no library -> failure" "$tmp" badpath
    touch "$tmp/lib/libtegra_aon_fsp.a"
    _case "set with the library -> compile" "$tmp" provisioned
    rm -rf "$tmp"
    echo "check-ivc-fsp --self-test: $failures check(s) failed"
    [ "$failures" -eq 0 ]
}

# The negative control runs on the NORMAL path: a gate whose own proof needs a
# flag is a gate whose proof nobody runs.
self_test || exit 1
if [ "${1:-}" = "--self-test" ]; then
    exit 0
fi

fsp_dir="${NV_SPE_FSP_DIR:-}"
case "$(classify_fsp_dir "$fsp_dir")" in
unprovisioned)
    nros_check_skip ivc-fsp \
        "NV_SPE_FSP_DIR unset — the NVIDIA Orin SPE FSP tree is not provisioned here (phase-418 418.4)"
    exit 0
    ;;
badpath)
    echo "check-ivc-fsp: NV_SPE_FSP_DIR=$fsp_dir has no lib/libtegra_aon_fsp.a" >&2
    echo "       The variable is SET, so this is a wrong path rather than an" >&2
    echo "       unprovisioned host, and it is a failure for that reason." >&2
    exit 1
    ;;
esac

echo "check-ivc-fsp: compiling nvidia-ivc --features fsp against $fsp_dir"
# `-D warnings`, like every other clippy lane here. Compiling this file for the
# first time turned up one lint immediately -- `declare_interior_mutable_const`
# on `Slot::EMPTY` -- which is a false positive for the `[const { … }; N]`
# atomic-array idiom and is now allowed AT the constant with the reason. Without
# `-D warnings` this gate would have watched that scroll past.
cargo clippy --quiet -p nvidia-ivc --no-default-features --features fsp -- -D warnings || exit 1
# The forwarders that sit on top of it, in the same configuration.
cargo clippy --quiet -p zpico-link-ivc --no-default-features -- -D warnings || exit 1
echo "check-ivc-fsp: OK"
