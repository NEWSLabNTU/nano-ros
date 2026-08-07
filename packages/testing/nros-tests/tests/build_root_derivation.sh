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

# The function names `fixtures-build.sh` ships to its make leaves, read OUT of
# the file (continuation lines included) so the leaf scenario below cannot pass
# against a hand-mirrored list that has since drifted. Defined out here because
# a scenario body is single-quoted and cannot hold the awk program.
leaf_exported_fns() {
    awk '/export -f nros_fixture_target_dir_flag/ { c = 1 }
         c { cont = ($0 ~ /\\$/); sub(/export -f/, ""); sub(/\\$/, ""); print; if (!cont) exit }
        ' scripts/build/fixtures-build.sh | tr '\n' ' '
}

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

# --- step 2 (CALLERS) --------------------------------------------------------
#
# Three families migrated off literals, each with its build + staleness probe +
# test resolver in ONE commit (RFC-0070 R3 — a family split across commits is a
# red sweep). Step 2 changes nothing observable, so what has to be asserted is
# that each kind's derived path is byte-identical to the literal it replaced.

echo "migrated families emit the pre-migration path:"

scenario '
    unset NROS_BUILD_ROOT
    export NROS_REPO_ROOT="$repo_root"
    # compile-check family: compile-check-fixtures.sh (build) +
    # compile-check-stale.sh (probe) + require_compile_check{,_bin} (resolver).
    check "compile-check out_root" \
        "$repo_root/build/compile-check" "$(nros_build_dir compile-check)"
    check "compile-check stamp dir" \
        "$repo_root/build/compile-check/n9_form1" "$(nros_build_dir compile-check n9_form1)"
    check "cmake-fixtures out_root" \
        "$repo_root/build/cmake-fixtures" "$(nros_build_dir cmake-fixtures)"
    check "cmake-fixtures stamp dir" \
        "$repo_root/build/cmake-fixtures/shadowing" "$(nros_build_dir cmake-fixtures shadowing)"
    # idf/west fixture families: <script>.sh (build) + require_{idf,west}_fixture.
    check "idf-fixtures out_root" \
        "$repo_root/build/idf-fixtures" "$(nros_build_dir idf-fixtures)"
    check "west-fixtures out_root" \
        "$repo_root/build/west-fixtures" "$(nros_build_dir west-fixtures)"
    # fixtures-cargo: the shell half moved in step 1, the resolver half here.
    check "fixtures-cargo resolver dir" \
        "$repo_root/build/fixtures-cargo/qemu-arm-baremetal" \
        "$(nros_build_dir fixtures-cargo qemu-arm-baremetal)"
'

# …and all of them relocate together, which is the whole point of one root.
scenario '
    export NROS_REPO_ROOT="$repo_root" NROS_BUILD_ROOT=/scratch/nros
    check "compile-check follows NROS_BUILD_ROOT" \
        "/scratch/nros/compile-check/n9_form1" "$(nros_build_dir compile-check n9_form1)"
    check "west-fixtures follows NROS_BUILD_ROOT" \
        "/scratch/nros/west-fixtures" "$(nros_build_dir west-fixtures)"
'

# The migrated writers pin NROS_REPO_ROOT to their OWN repo root, so the last
# resort matters only for callers that pin nothing. It used to be `$PWD`, which
# is wrong for exactly the reason the file's header gives (builders cd into an
# example dir); it is now this file's own checkout.
scenario '
    unset NROS_BUILD_ROOT NROS_REPO_ROOT NROS_REPO_DIR
    cd /
    check "fallback is the script'"'"'s own checkout, not \$PWD" \
        "$repo_root/build" "$(nros_build_root)"
'

# The make-leaf path. `fixtures-build.sh` schedules eligible rows as make leaves
# and ships the resolver to them with `export -f`; a leaf is a fresh bash where
# the function came from the ENVIRONMENT, so `${BASH_SOURCE[0]}` inside it is not
# a file path. Step 1 sourced build-root.sh from INSIDE the resolver, which made
# that source resolve to `./build-root.sh` and the resolver emit an EMPTY
# `--target-dir` — the build writing where neither the probe nor the test
# resolver looks, which is precisely the family split R3 exists to prevent, and
# it was invisible to a test that only ever called the function in-process.
#
# The exported set is READ OUT of fixtures-build.sh so this cannot pass by
# mirroring a list that has since drifted.
echo "resolver survives export -f into a make leaf:"
scenario '
    export NROS_REPO_ROOT="$repo_root"
    source scripts/build/fixtures-target-dir.sh
    fns="$(leaf_exported_fns)"
    if [ -z "${fns// /}" ]; then
        echo "  FAIL could not read the export -f list out of fixtures-build.sh"
        rc=1
    fi
    # shellcheck disable=SC2086
    export -f $fns
    inproc="$(nros_fixture_target_dir_flag qemu-arm-baremetal "" "")"
    leaf="$(cd examples && env bash -c "nros_fixture_target_dir_flag qemu-arm-baremetal \"\" \"\"" 2>/dev/null)"
    check "make leaf resolves the same dir as the parent" "$inproc" "$leaf"
    check "make leaf dir is the pre-migration path" \
        " --target-dir $repo_root/build/fixtures-cargo/qemu-arm-baremetal" "$leaf"
'

echo "no literal remains in the migrated families:"

# The derivation only holds if nothing spells the path beside it. These are
# narrow, per-family greps rather than a repo-wide gate — that gate is step 4.
scenario '
    for f in scripts/build/compile-check-fixtures.sh scripts/test/compile-check-stale.sh \
             scripts/build/idf-fixtures.sh scripts/build/west-fixtures.sh; do
        hits="$(grep -n "\$repo_root/build/" "$f" || true)"
        if [ -n "$hits" ]; then
            echo "  FAIL $f still spells a cache path literally"
            echo "$hits" | sed "s/^/        /"
            rc=1
        else
            echo "  ok   $f has no \$repo_root/build/ literal"
        fi
    done
'
# A DOUBLE-QUOTED "build/<kind>" in the resolver is a `join()` argument; the
# doc comments that name the same paths spell them in `backticks`, so this grep
# sees code only.
scenario '
    f=packages/testing/nros-tests/src/fixtures/binaries/mod.rs
    hits="$(grep -nE "\"build/(compile-check|cmake-fixtures|idf-fixtures|west-fixtures|fixtures-cargo)" "$f" || true)"
    if [ -n "$hits" ]; then
        echo "  FAIL $f still resolves a migrated family from a literal"
        echo "$hits" | sed "s/^/        /"
        rc=1
    else
        echo "  ok   binaries/mod.rs resolves the migrated families via build_dir"
    fi
'

# The Rust mirror cannot source this file, so the one thing that CAN silently
# diverge is the relocation knob: a mirror that ignores NROS_BUILD_ROOT would
# leave the resolver looking in a tree the build no longer writes.
# It must match the READ, not a doc comment that names the variable: the first
# version of this check grepped the bare name and stayed green when the mirror
# was pointed at a different env var entirely.
scenario '
    f=packages/testing/nros-tests/src/lib.rs
    if grep -q "env::var(\"NROS_BUILD_ROOT\")" "$f" && grep -q "pub fn build_root" "$f" \
       && grep -q "pub fn build_dir" "$f"; then
        echo "  ok   nros_tests mirror reads NROS_BUILD_ROOT"
    else
        echo "  FAIL nros_tests::build_root/build_dir mirror missing or ignores NROS_BUILD_ROOT"
        rc=1
    fi
'

echo "phase-334 W2.b step 2 — the rooted writers emit their pre-migration path:"

# Each of these replaced a `$repo_root/build/<kind>` literal. The assertion is
# that the DERIVED value is byte-identical to the literal it replaced — step 2 is
# a pure refactor, so anything else is a regression, not an improvement.
scenario '
    unset NROS_BUILD_ROOT
    export NROS_REPO_ROOT="$repo_root"
    check "borrowed-e2e"                "$repo_root/build/borrowed-e2e"                "$(nros_build_dir borrowed-e2e)"
    check "link-determinism"            "$repo_root/build/link-determinism"            "$(nros_build_dir link-determinism)"
    check "fixture-make-driver"         "$repo_root/build/fixture-make-driver"         "$(nros_build_dir fixture-make-driver)"
    check "zephyr-fixture-make-driver"  "$repo_root/build/zephyr-fixture-make-driver"  "$(nros_build_dir zephyr-fixture-make-driver)"
    check "zephyr-fixture-build.lock"   "$repo_root/build/zephyr-fixture-build.lock"   "$(nros_build_dir zephyr-fixture-build).lock"
'

# And that no literal survives alongside the call — a writer that derives the
# path in one place and spells it in another is the split step 1 left behind in
# fixtures-target-dir.sh, which took a second commit to find.
#
# `grep -F` on the literal text: an interpolating pattern expands `$repo_root`
# under `eval` and then searches for the ABSOLUTE path, which never appears in
# the source — a check that cannot fail. That is exactly what the first version
# of this block did, and the tripwire caught it.
rc=0
for f in scripts/build/borrowed-e2e-fixture.sh \
         scripts/build/link-determinism-fixture.sh \
         scripts/build/fixture-make-driver.sh \
         scripts/build/zephyr-fixture-make-driver.sh; do
    if grep -qF 'repo_root/build/' "$repo_root/$f"; then
        echo "  FAIL $f still assigns a rooted literal"
        rc=1
    fi
done
if [ "$rc" -eq 0 ]; then
    echo "  ok   no rooted literal remains beside the derivation"
else
    fail=1
fi

if [ "$fail" -ne 0 ]; then
    echo "build_root_derivation: FAILED" >&2
    exit 1
fi
echo "build_root_derivation: all checks passed"
