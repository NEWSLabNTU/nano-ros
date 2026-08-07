#!/usr/bin/env bash
# RFC-0070 R1/R3 — the build-root derivation.
#
# phase-334 W2.b step 1 moved `fixtures-target-dir.sh` off a hardcoded
# `$root/build/...` literal and onto `nros_build_dir`. The whole point of that
# step is that it changes NOTHING observable, so the test that matters is the
# one asserting the emitted path is byte-identical to the old literal. Without
# it, "derivation first, paths later" is an intention rather than a property.
set -uo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
cd "$repo_root"

# Each scenario runs in a SUBSHELL so it can set env vars without leaking to the
# next one — which means `fail=1` inside it cannot reach this scope. The subshell
# therefore exits with its own status and `scenario` accumulates here. The first
# version of this file accumulated into a plain variable and could never fail:
# it printed FAIL lines and then "all checks passed". Caught by tripwiring the
# derivation, which is the only reason it is not still silently green.
fail=0
check() { # <what> <expected> <actual>  -- sets `rc` in the CURRENT shell
    if [ "$2" = "$3" ]; then
        echo "  ok   $1"
    else
        echo "  FAIL $1"
        echo "        expected: $2"
        echo "        actual:   $3"
        rc=1
    fi
}

# scenario <<'EOF'-style: run a subshell body, propagate its exit status.
scenario() { ( rc=0; eval "$1"; exit "$rc" ) || fail=1; }

# shellcheck source=scripts/build/build-root.sh
source scripts/build/build-root.sh

echo "build-root derivation:"

# R1 — default is <repo>/build, so an unset environment behaves as before.
scenario '
    unset NROS_BUILD_ROOT
    export NROS_REPO_ROOT="$repo_root"
    check "default root is <repo>/build" "$repo_root/build" "$(nros_build_root)"
'

# R1 — NROS_BUILD_ROOT relocates the whole tree, trailing slash tolerated.
scenario '
    export NROS_BUILD_ROOT=/scratch/nros
    check "NROS_BUILD_ROOT relocates" "/scratch/nros" "$(nros_build_root)"
'
scenario '
    export NROS_BUILD_ROOT=/scratch/nros/
    check "trailing slash stripped" "/scratch/nros" "$(nros_build_root)"
'

# R2 — <root>/<kind>/<coordinate>, empty coordinate parts skipped.
scenario '
    export NROS_BUILD_ROOT=/r
    check "kind only"        "/r/cargo"            "$(nros_build_dir cargo)"
    check "kind + coord"     "/r/cargo/linux-zenoh" "$(nros_build_dir cargo linux-zenoh)"
    check "multi-part coord" "/r/cmake/workspace/c" "$(nros_build_dir cmake workspace c)"
    check "empty part skipped" "/r/cargo/x"        "$(nros_build_dir cargo "" x)"
'

# A kind is mandatory: a rootless cache dir is the bug R2 exists to prevent.
scenario '
    export NROS_BUILD_ROOT=/r
    if nros_build_dir "" >/dev/null 2>&1; then
        echo "  FAIL empty kind must be rejected"
        rc=1
    else
        echo "  ok   empty kind rejected"
    fi
'

echo "fixtures-target-dir still emits the pre-migration path:"

# The step-1 invariant. `qemu-arm-baremetal` is the one platform in
# NROS_FIXTURE_SHARED_PLATFORMS, so it is the only row that produces a flag.
scenario '
    unset NROS_BUILD_ROOT
    export NROS_REPO_ROOT="$repo_root"
    source scripts/build/fixtures-target-dir.sh
    check "shared platform -> old literal path" \
        " --target-dir $repo_root/build/fixtures-cargo/qemu-arm-baremetal" \
        "$(nros_fixture_target_dir_flag qemu-arm-baremetal "" "")"
    check "unmigrated platform -> no flag (unchanged)" \
        "" "$(nros_fixture_target_dir_flag linux "" "")"
    check "authored --target-dir still wins" \
        "" "$(nros_fixture_target_dir_flag qemu-arm-baremetal "--target-dir target-zenoh" "")"
'

# And it follows NROS_BUILD_ROOT once set — the reason for the migration.
scenario '
    export NROS_REPO_ROOT="$repo_root" NROS_BUILD_ROOT=/scratch/nros
    source scripts/build/fixtures-target-dir.sh
    check "shared platform follows NROS_BUILD_ROOT" \
        " --target-dir /scratch/nros/fixtures-cargo/qemu-arm-baremetal" \
        "$(nros_fixture_target_dir_flag qemu-arm-baremetal "" "")"
'

if [ "$fail" -ne 0 ]; then
    echo "build_root_derivation: FAILED" >&2
    exit 1
fi
echo "build_root_derivation: all checks passed"
