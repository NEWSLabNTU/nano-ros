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
#
# A body that ends >=2 did not FINISH its checks: that is `nros_grep_q` refusing
# to verdict from a grep that did not run (issue 0726), and its `exit 2` can
# only reach the subshell. Folding it into `fail=1` would print
# "build_root_derivation: FAILED" — a claim that a checked property is broken —
# for a tool failure, so it is re-raised here instead. `check` and the two
# cannot-test aborts use 1, which is the ordinary failure path.
scenario() {
    local src=0
    ( rc=0; eval "$1"; exit "$rc" ) || src=$?
    if [ "$src" -ge 2 ]; then
        echo "build_root_derivation: aborting — a check could not be run" >&2
        exit "$src"
    fi
    [ "$src" -eq 0 ] || fail=1
}

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

# Issue 0726 — the greps below are ASSERTIONS about other files (does the build
# strip the authored flag? does the Rust mirror read NROS_BUILD_ROOT?), and
# `grep -q` cannot tell "the text is absent" from "the grep did not run". Under
# the parallel gate fan-out the second becomes the first, and the FAIL text
# names a mechanism that is intact. `nros_grep_q` exits 2 on rc>=2.
# shellcheck source=scripts/lib/grep-q.sh
source scripts/lib/grep-q.sh

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
        " --target-dir $repo_root/build/cargo-fixtures/qemu-arm-baremetal" \
        "$(nros_fixture_target_dir_flag qemu-arm-baremetal "" "")"
    # phase-340 B3 — the "unmigrated" example must be a platform that is
    # actually unmigrated. This arm named `linux`, which B3 migrated, so it
    # started asserting the opposite of its own title. Derive one instead of
    # naming a second value that a later migration will invalidate again.
    unmigrated=""
    for _p in nuttx freertos esp32 threadx-linux; do
        case " $NROS_FIXTURE_SHARED_PLATFORMS " in
            *" $_p "*) ;;
            *) unmigrated="$_p"; break ;;
        esac
    done
    [ -n "$unmigrated" ] || { echo "FAIL no unmigrated platform left to test with"; exit 1; }
    check "unmigrated platform -> no flag (unchanged)" \
        "" "$(nros_fixture_target_dir_flag "$unmigrated" "" "")"
'

# phase-340 W2 — an authored `--target-dir` used to opt the row OUT; it now
# names a GROUP. Those authored dirs are the R1 duplicate population this phase
# measures, so the old rule excluded exactly the rows worth grouping.
echo "phase-340 W2 — an authored --target-dir names a group, not an opt-out:"
scenario '
    unset NROS_BUILD_ROOT
    export NROS_REPO_ROOT="$repo_root"
    source scripts/build/fixtures-target-dir.sh
    # Same group as a bare row: the authored STRING is not in the key, so a row
    # that only differs by spelling its own dir joins the default group.
    check "authored dir no longer opts out" \
        " --target-dir $repo_root/build/cargo-fixtures/qemu-arm-baremetal" \
        "$(nros_fixture_target_dir_flag qemu-arm-baremetal "--target-dir target-zenoh" "")"
    # NOTE: this scenario body is a SINGLE-QUOTED string. No apostrophes in
    # these comments — one closes the body, and everything after it evaluates in
    # the wrong context. (Cost one debugging round; the symptom was
    # "nros_fixture_group_slug: command not found" four checks later.)
    #
    # ...and features still split it. THE REASON IS THE FLAT ARTIFACT NAMESPACE,
    # NOT -C metadata (phase-340 W1, 2026-08-08). The first spelling of this
    # check said "it changes -C metadata", which is true and useless: deps/ puts
    # the metadata hash IN the filename, so two feature variants coexist there
    # perfectly. W1 read that message, reasonably concluded the assertion was an
    # artefact of the abandoned umbrella shape, and proposed a platform-grained
    # key that deletes it.
    #
    # What actually breaks is <group>/<profile>/<bin>, which cargo does NOT
    # hash. Under a platform-grained key the four manifest rows of
    # examples/native/rust/talker (default / rmw-zenoh / rmw-xrce / link-tls)
    # all write ONE path; measured on a provisioned tree they are four different
    # binaries (8616504 / 8616504 / 6514392 / 9034536 bytes, four distinct
    # sha256), and a second cargo invocation replaces the artifact with no
    # warning at all -- the "output filename collision" warning fires only when
    # ONE invocation builds both. Last writer wins and a test greens on the
    # other variant binary.
    #
    # So if you are here because you want a coarser key, this check is the thing
    # you must satisfy, not the thing you must delete. Coarsening also disarms
    # the A2 arm of check-fixture-groups by construction (every group becomes
    # the default group), which is why the key-level assertion lives here and
    # not only in the gate.
    bare="$(nros_fixture_group_slug qemu-arm-baremetal "" "")"
    authored="$(nros_fixture_group_slug qemu-arm-baremetal "--target-dir target-zenoh" "")"
    feats="$(nros_fixture_group_slug qemu-arm-baremetal "--no-default-features --features rmw-zenoh" "")"
    check "authored dir does not change the group key" "$bare" "$authored"
    if [ "$bare" = "$feats" ]; then
        echo "  FAIL a feature set must change the group key — two variants of ONE"
        echo "       package would otherwise share <group>/<profile>/<bin>, which"
        echo "       cargo does not hash, and silently overwrite each other"
        rc=1
    else
        echo "  ok   a feature set changes the group key"
    fi
    # The env is in the key for the same reason: nros-bench/stress-zenoh has a
    # bare row and a ZPICO_SUBSCRIBER_BUFFER_SIZE=8192 row, same package, same
    # binary name.
    envd="$(nros_fixture_group_slug qemu-arm-baremetal "" "ZPICO_SUBSCRIBER_BUFFER_SIZE=8192")"
    if [ "$bare" = "$envd" ]; then
        echo "  FAIL a build env var must change the group key (same artifact path)"
        rc=1
    else
        echo "  ok   a build env var changes the group key"
    fi
    # The slug function is eligibility-FREE: `check-fixture-groups` has to ask
    # "which group WOULD this row land in?" for a platform that is by
    # definition not migrated yet.
    # phase-340 B3 — same correction: pick a platform still outside the shipped
    # list, so this keeps testing "eligibility gates the flag" rather than
    # quietly testing a migrated platform.
    unmigrated=""
    for _p in nuttx freertos esp32 threadx-linux; do
        case " $NROS_FIXTURE_SHARED_PLATFORMS " in
            *" $_p "*) ;;
            *) unmigrated="$_p"; break ;;
        esac
    done
    [ -n "$unmigrated" ] || { echo "FAIL no unmigrated platform left to test with"; exit 1; }
    check "slug is emitted for an unmigrated platform" \
        "$unmigrated" "$(nros_fixture_group_slug "$unmigrated" "" "")"
    check "eligibility still gates the FLAG" \
        "" "$(nros_fixture_target_dir_flag "$unmigrated" "" "")"
'

# The strip. Passing both the authored flag and the group flag hands cargo two
# `--target-dir`s; cargo takes the last, so the build would silently work while
# the manifest lied about where it wrote.
echo "phase-340 W2 — the authored flag is stripped when the group governs:"
scenario '
    source scripts/build/fixtures-target-dir.sh
    check "strips the pair"      "--features a" \
        "$(nros_fixture_strip_authored_target_dir "--target-dir target-zenoh --features a")"
    check "strips mid-string"    "--no-default-features --features a" \
        "$(nros_fixture_strip_authored_target_dir "--no-default-features --target-dir t --features a")"
    check "leaves --target alone" "--target thumbv7m-none-eabi" \
        "$(nros_fixture_strip_authored_target_dir "--target thumbv7m-none-eabi")"
    check "empty in, empty out"  "" "$(nros_fixture_strip_authored_target_dir "")"
'

# Both callers must strip, or the probe builds into the leaf while the build
# writes the group dir — permanent false-stale, which is the family split R3
# exists to prevent. Read out of the files rather than asserted in prose.
echo "phase-340 W2 — build and staleness probe both strip:"
scenario '
    for f in scripts/build/fixtures-build.sh scripts/test/rust-fixture-stale.sh; do
        if nros_grep_q "nros_fixture_strip_authored_target_dir" "$f"; then
            echo "  ok   $f strips the authored flag"
        else
            echo "  FAIL $f appends the group dir without stripping the authored one"
            rc=1
        fi
    done
    # …and the make leaves get the helper, or they die "command not found".
    # Captured, not piped: in a pipeline nros_grep_q is a SUBSHELL and its
    # exit 2 would end only that segment, restoring the ambiguity.
    exported="$(leaf_exported_fns)"
    if nros_grep_q "nros_fixture_strip_authored_target_dir" <<<"$exported"; then
        echo "  ok   the strip helper is shipped to the make leaves"
    else
        echo "  FAIL nros_fixture_strip_authored_target_dir is not in the export -f list"
        rc=1
    fi
'

# And it follows NROS_BUILD_ROOT once set — the reason for the migration.
scenario '
    export NROS_REPO_ROOT="$repo_root" NROS_BUILD_ROOT=/scratch/nros
    source scripts/build/fixtures-target-dir.sh
    check "shared platform follows NROS_BUILD_ROOT" \
        " --target-dir /scratch/nros/cargo-fixtures/qemu-arm-baremetal" \
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
        "$repo_root/build/compile-check-fixtures" "$(nros_build_dir "$NROS_KIND_COMPILE_CHECK")"
    check "compile-check stamp dir" \
        "$repo_root/build/compile-check-fixtures/main_macro_form1" "$(nros_build_dir "$NROS_KIND_COMPILE_CHECK" main_macro_form1)"
    check "cmake-fixtures out_root" \
        "$repo_root/build/cmake-fixtures" "$(nros_build_dir "$NROS_KIND_CMAKE_FIXTURES")"
    check "cmake-fixtures stamp dir" \
        "$repo_root/build/cmake-fixtures/shadowing" "$(nros_build_dir "$NROS_KIND_CMAKE_FIXTURES" shadowing)"
    # idf/west fixture families: <script>.sh (build) + require_{idf,west}_fixture.
    check "idf-fixtures out_root" \
        "$repo_root/build/idf-fixtures" "$(nros_build_dir "$NROS_KIND_IDF_FIXTURES")"
    check "west-fixtures out_root" \
        "$repo_root/build/west-fixtures" "$(nros_build_dir "$NROS_KIND_WEST_FIXTURES")"
    # cargo-fixtures: the shell half moved in step 1, the resolver half here.
    check "cargo-fixtures resolver dir" \
        "$repo_root/build/cargo-fixtures/qemu-arm-baremetal" \
        "$(nros_build_dir "$NROS_KIND_CARGO_FIXTURES" qemu-arm-baremetal)"
'

# …and all of them relocate together, which is the whole point of one root.
scenario '
    export NROS_REPO_ROOT="$repo_root" NROS_BUILD_ROOT=/scratch/nros
    check "compile-check follows NROS_BUILD_ROOT" \
        "/scratch/nros/compile-check-fixtures/main_macro_form1" "$(nros_build_dir "$NROS_KIND_COMPILE_CHECK" main_macro_form1)"
    check "west-fixtures follows NROS_BUILD_ROOT" \
        "/scratch/nros/west-fixtures" "$(nros_build_dir "$NROS_KIND_WEST_FIXTURES")"
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
        " --target-dir $repo_root/build/cargo-fixtures/qemu-arm-baremetal" "$leaf"
    # phase-340 W2 — the strip runs in the leaf too, and it is the half that
    # would fail SILENTLY: a leaf missing it passes cargo two --target-dir
    # flags and the build still succeeds, into the wrong tree.
    stripped="$(cd examples && env bash -c \
        "nros_fixture_strip_authored_target_dir \"--target-dir target-zenoh --features a\"" 2>/dev/null)"
    check "make leaf can strip the authored flag" "--features a" "$stripped"
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
    hits="$(grep -nE "\"build/(compile-check|cmake-fixtures|idf-fixtures|west-fixtures|cargo-fixtures)" "$f" || true)"
    if [ -n "$hits" ]; then
        echo "  FAIL $f still resolves a migrated family from a literal"
        echo "$hits" | sed "s/^/        /"
        rc=1
    else
        echo "  ok   binaries/mod.rs resolves the migrated families via build_dir"
    fi
'

# RFC-0070 R5 (phase-350) — a KIND is a constant, never a bare word.
#
# The kind used to be a literal at every call site, which is what made renaming
# one a search over an overloaded token instead of an edit. Both halves have a
# vocabulary now (`NROS_KIND_*` here, `nros_tests::kind` in Rust); this keeps new
# call sites from going back to bare words, which is the only way the extraction
# stays true.
#
# This file is exempt from both arms ON PURPOSE: it is the test that pins each
# constant to its expected path, so its expected side MUST spell the literal. A
# check written with the constant on both sides checks nothing.
#
# `export -f`/`declare -f` lines are excluded because they NAME functions rather
# than call them: `export -f nros_build_dir nros_build_root` reads to this regex
# as a call passing the bare kind `nros_build_root` (issue 0624 shipped one, and
# a recipe that ships helpers to its subshells has to write that line).
scenario '
    hits="$(git grep -nE "nros_build_dir [a-z0-9]" -- scripts just justfile \
        | grep -v "build-root.sh" | grep -v "build_root_derivation.sh" \
        | grep -vE "^[^:]+:[0-9]+: *(export|declare) -f" || true)"
    if [ -n "$hits" ]; then
        echo "  FAIL a shell call site passes a bare-word kind (use \$NROS_KIND_*)"
        echo "$hits" | sed "s/^/        /"
        rc=1
    else
        echo "  ok   every shell kind comes from \$NROS_KIND_*"
    fi
'
scenario '
    hits="$(git grep -nE "build_dir\(\"" -- packages \
        | grep -v "nros-tests/src/lib.rs" || true)"
    if [ -n "$hits" ]; then
        echo "  FAIL a Rust call site passes a literal kind (use nros_tests::kind::*)"
        echo "$hits" | sed "s/^/        /"
        rc=1
    else
        echo "  ok   every Rust kind comes from nros_tests::kind"
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
    if nros_grep_q "env::var(\"NROS_BUILD_ROOT\")" "$f" \
       && nros_grep_q "pub fn build_root" "$f" \
       && nros_grep_q "pub fn build_dir" "$f"; then
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
    check "borrowed-e2e"                "$repo_root/build/borrowed-e2e"                "$(nros_build_dir "$NROS_KIND_BORROWED_E2E")"
    check "link-determinism"            "$repo_root/build/link-determinism"            "$(nros_build_dir "$NROS_KIND_LINK_DETERMINISM")"
    check "fixture-make-driver"         "$repo_root/build/fixture-make-driver"         "$(nros_build_dir "$NROS_KIND_FIXTURE_MAKE_DRIVER")"
    check "zephyr-fixture-make-driver"  "$repo_root/build/zephyr-fixture-make-driver"  "$(nros_build_dir "$NROS_KIND_ZEPHYR_FIXTURE_MAKE_DRIVER")"
    check "zephyr-fixture-build.lock"   "$repo_root/build/zephyr-fixture-build.lock"   "$(nros_build_dir "$NROS_KIND_ZEPHYR_FIXTURE_BUILD").lock"
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
    if nros_grep_q -F -- 'repo_root/build/' "$repo_root/$f"; then
        echo "  FAIL $f still assigns a rooted literal"
        rc=1
    fi
done
if [ "$rc" -eq 0 ]; then
    echo "  ok   no rooted literal remains beside the derivation"
else
    fail=1
fi

# phase-340 W2 — the THIRD spelling of the cargo-fixtures group path.
# `just/qemu-baremetal.just` computes FIXTURE_TARGET with a parse-time
# `absolute_path()` literal, recorded in phase-334 W2.b as a deliberate
# non-migration (a bash call there needs a `shell()` on every justfile parse).
# Deliberate or not, it is a literal that must track the derivation, and until
# now nothing said so. It compares against the DEFAULT root only — the justfile
# cannot follow NROS_BUILD_ROOT, which is exactly why it was not migrated.
echo "phase-340 W2 — the justfile's FIXTURE_TARGET literal tracks the derivation:"
scenario '
    unset NROS_BUILD_ROOT
    export NROS_REPO_ROOT="$repo_root"
    lit="$(sed -n "s/^FIXTURE_TARGET := absolute_path(\"\([^\"]*\)\").*/\1/p" just/qemu-baremetal.just)"
    if [ -z "$lit" ]; then
        echo "  FAIL could not read FIXTURE_TARGET out of just/qemu-baremetal.just"
        rc=1
    fi
    check "FIXTURE_TARGET == the derived qemu-arm-baremetal group dir" \
        "$(nros_build_dir "$NROS_KIND_CARGO_FIXTURES" qemu-arm-baremetal)" "$repo_root/$lit"
'

echo "phase-340 B2 — the Rust resolver holds NO copy of the eligibility rule:"

# There used to be a mirror here: `SHARED_PLATFORMS_DEFAULT` in nros-tests
# beside `NROS_FIXTURE_SHARED_PLATFORMS` in the shell, and this scenario checked
# the two literals matched. W2.a had already narrowed a hardcoded `match
# platform` down to a duplicated env read, which is a smaller copy of the rule,
# not the absence of one — and a copy that read EMPTY as "share nothing" while
# the shell's `${…:-default}` reads it as "use the default".
#
# phase-340 B2 deleted the copy. Eligibility is decided once, by the shell, and
# arrives PER ROW in `fixtures-manifest.py fixture-groups`, which is also where
# the variant slug comes from. So the assertion flips: the Rust side must
# mention the variable NOWHERE, and must reach the export instead.
#
# Matched on the READ (`env::var…("NROS_FIXTURE_SHARED_PLATFORMS")`), not on the
# name: the name appears in several doc comments and in one error message, which
# is exactly where it SHOULD appear — a resolver that points at the rule it does
# not own. A bare name grep failed on those comments, which is this phase s
# "the tripwire landed in a docstring" trap with the polarity reversed.
scenario '
    cd "$repo_root"
    rs="packages/testing/nros-tests/src"
    if git grep -nE "var(_os)?\(\"NROS_FIXTURE_SHARED_PLATFORMS" -- "$rs"; then
        echo "  FAIL the Rust side READS NROS_FIXTURE_SHARED_PLATFORMS again —"
        echo "       that is a second copy of the eligibility rule. It belongs in"
        echo "       scripts/build/fixtures-target-dir.sh, reported per row by"
        echo "       fixtures-manifest.py fixture-groups."
        rc=1
    else
        echo "  ok   no Rust copy of the eligibility list"
    fi
    if nros_grep_q "fixture-groups" "$rs/fixtures/groups.rs"; then
        echo "  ok   the Rust resolver consumes the manifest group export"
    else
        echo "  FAIL fixtures/groups.rs no longer consumes fixture-groups"
        rc=1
    fi
'

echo "phase-340 B2 — the batch driver agrees with the per-row key:"

# `nros_fixture_group_batch` exists so two Python consumers (the migration gate
# and the resolver export) need one `bash` rather than 240. A batch that drifted
# from the per-row function would hand the test resolver a different dir from
# the one the BUILD uses — #393 with an extra layer. Compare them directly.
scenario '
    cd "$repo_root"
    . scripts/build/fixtures-target-dir.sh
    one="$(nros_fixture_group_slug linux "--no-default-features --features rmw-xrce" "")"
    two="$(printf "linux\x1f--no-default-features --features rmw-xrce\x1f\n" \
            | nros_fixture_group_batch | cut -d"$(printf "\x1f")" -f1)"
    check "batch slug == per-row slug" "$one" "$two"
    # And the memo must not leak one row s answer into the next.
    mixed="$(printf "linux\x1f\x1f\nlinux\x1f--features link-tls\x1f\nlinux\x1f\x1f\n" \
            | nros_fixture_group_batch | cut -d"$(printf "\x1f")" -f1 | tr "\n" " ")"
    check "memo keeps distinct records distinct" \
        "linux linux-$(printf "%s" "--features link-tls|" | cksum | cut -d" " -f1) linux " \
        "$mixed"
'

if [ "$fail" -ne 0 ]; then
    echo "build_root_derivation: FAILED" >&2
    exit 1
fi
echo "build_root_derivation: all checks passed"
