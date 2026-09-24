#!/usr/bin/env bash
# Run the `#[test]`s of workspace-EXCLUDED crates that no other lane runs.
#
# Issue 1472 / phase-451 W4.
#
# `cargo test --workspace` reaches members. A crate in the root `exclude` list
# is reached by nothing, so its tests are not slow or flaky — they simply never
# execute, and nothing says so. phase-451 W4 found this for one crate:
#
#   > `nros-platform-stm32f4`'s three `detect_phy_type` tests DID run while it
#   > was briefly a member (`3 passed`) […] reaching them permanently needs a
#   > per-crate test lane, not membership.
#
# Membership is not available: `cortex-m`, `esp-hal` and
# `nros-platform-critical-section` each select a different `critical-section`
# restore-state width and critical-section refuses more than one, so no
# workspace build can hold them. A per-crate lane is the remaining shape.
#
# MEASURED, and the phase understated it. Three excluded crates have tests that
# run nowhere, and all of them pass:
#
#   openeth-smoltcp          32 passed   (nothing in just/, scripts/ or
#                                         .github/ mentions this crate at all)
#   nros-platform-stm32f4     3 passed
#   nros-smoltcp              1 passed
#
# 36 tests, not 3.
#
# ## Why each exemption is structural, not a backlog
#
# An exempt crate is one whose tests run SOMEWHERE ELSE, or that cannot build
# for the host at all. Neither is a promise to come back to it, and a crate
# that is merely inconvenient does not qualify — an unexempted excluded crate
# with tests FAILS this script, so the list cannot grow by neglect.
#
# ## The target dir
#
# `nros_scoped_target_dir`, never a bare `cd <crate> && cargo test`. That is
# phase-340 P2's defect: a build with no coordinate gets no shared cargo group
# and re-creates a per-leaf `target/`. Measured here — the first hand run of
# these tests left one inside `packages/platform/nros-platform-stm32f4/`.
set -uo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
cd "$repo_root" || exit 2

# shellcheck source=scripts/build/cargo.sh
source scripts/build/cargo.sh

# path-prefix -> why its tests are not this script's business.
exempt_reason() {
    case "$1" in
        packages/cli|packages/cli/*)
            printf 'its own sub-workspace with its own lane (`just check cli-tests`)' ;;
        third-party/*)
            printf 'vendored third-party source; its tests are upstream'"'"'s' ;;
        packages/testing/nros-px4-sitl-test)
            printf 'driven by `just px4`, which owns the SITL it needs' ;;
        packages/boards/nros-board-nuttx-qemu)
            # Measured, not assumed: `cargo test --lib` on the host dies with
            # `error: entry symbol `main` declared multiple times`. A board
            # crate supplies an entry point; the test harness supplies another.
            printf 'cannot link for the host — it defines an entry symbol the test harness also defines' ;;
        *) return 1 ;;
    esac
}

# Every path in the root manifest's `exclude` list.
excluded_paths() {
    awk '
        /^exclude = \[/ { inx = 1; next }
        inx && /^\]/     { inx = 0 }
        inx              { if (match($0, /"[^"]+"/)) print substr($0, RSTART + 1, RLENGTH - 2) }
    ' Cargo.toml
}

has_tests() {
    local dir="$1"
    [ -f "$dir/Cargo.toml" ] || return 1
    # `git grep` over the tracked files under it: an index lookup, not a walk
    # (`check-no-tracked-file-find`), and it cannot see a stray `target/`.
    git grep -q -F -- '#[test]' -- "$dir" 2>/dev/null
}

# The controls, on the normal path — a lane that cannot be seen to refuse is a
# lane that will silently stop covering things.
self_test() {
    local ok=0
    # An unknown path must NOT be exempt: that is how a newly excluded crate
    # with tests gets RUN rather than quietly skipped.
    if exempt_reason "packages/some/crate-nobody-listed" >/dev/null; then
        echo "  FAIL an unlisted path was treated as exempt" >&2
        ok=1
    else
        echo "  ok  an unlisted excluded crate is NOT exempt (it gets run)"
    fi
    # Every exemption must carry a reason; an empty one reads as a decision
    # nobody made.
    local p
    for p in packages/cli third-party/px4/px4-rs \
             packages/testing/nros-px4-sitl-test \
             packages/boards/nros-board-nuttx-qemu; do
        if [ -z "$(exempt_reason "$p" 2>/dev/null)" ]; then
            echo "  FAIL $p is exempt with no reason" >&2
            ok=1
        fi
    done
    [ "$ok" -eq 0 ] && echo "  ok  every exemption carries a reason"
    # Discovery must find something. The FAILURE arm was measured against
    # `nros-board-nuttx-qemu` before it was exempted: `cargo test --lib` there
    # exits 101 with `error: entry symbol \`main\` declared multiple times`,
    # and the loop propagated it.
    local n
    n=$(excluded_paths | wc -l)
    if [ "$n" -lt 1 ]; then
        echo "  FAIL the exclude list parsed to nothing" >&2
        ok=1
    else
        echo "  ok  the exclude list parses ($n entries)"
    fi
    return "$ok"
}

mode="${1:-run}"

# On the NORMAL path, at statement position, unredirected — the shape
# `check-gate-selftests` recognises for shell. Its own comment records why that
# matters: `CALL` requires `name(`, which bash never writes, so for 75 shell
# gates the rule was unsatisfiable and they sat in the "still owe one" count
# looking like ordinary debt. A control nobody runs decays into a comment.
self_test || { echo "run-excluded-crate-tests: SELF-TEST FAILED" >&2; exit 1; }
[ "$mode" = "--self-test" ] && exit 0
tested=0
failed=0
skipped=()
ran=()

for path in $(excluded_paths); do
    [ -d "$path" ] || continue
    has_tests "$path" || continue
    if reason="$(exempt_reason "$path")"; then
        skipped+=("$path — $reason")
        continue
    fi
    if [ "$mode" = "--list" ]; then
        ran+=("$path")
        continue
    fi
    echo "== $path"
    rc=0
    CARGO_TARGET_DIR="$(nros_scoped_target_dir excluded-tests)" \
        cargo test --manifest-path "$path/Cargo.toml" --lib || rc=$?
    if [ "$rc" -ne 0 ]; then
        echo "  FAILED (rc=$rc)" >&2
        failed=$((failed + 1))
    fi
    tested=$((tested + 1))
done

if [ "$mode" = "--list" ]; then
    echo "excluded crates whose tests this lane runs:"
    printf '  %s\n' "${ran[@]}"
    echo "exempt (their tests run elsewhere, or they cannot build for the host):"
    printf '  %s\n' "${skipped[@]}"
    exit 0
fi

if [ "$tested" -eq 0 ]; then
    echo "run-excluded-crate-tests: found NO excluded crate with tests to run." >&2
    echo "  That is not a pass — three were measured when this was written." >&2
    echo "  Either the exclude list stopped parsing, or every crate became exempt." >&2
    exit 1
fi

for s in "${skipped[@]}"; do
    echo "  exempt: $s"
done

if [ "$failed" -ne 0 ]; then
    echo "run-excluded-crate-tests: $failed of $tested crate(s) FAILED." >&2
    exit 1
fi
echo "run-excluded-crate-tests: OK — $tested excluded crate(s) tested, ${#skipped[@]} exempt with a reason."
