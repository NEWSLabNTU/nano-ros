#!/usr/bin/env bash
# phase-300 W2.2 — strict mode + loud probes: previously a CRASHED
# staleness probe (stderr discarded, no pipefail) was indistinguishable
# from a fresh fixture. Probe stderr now lands in a capture file echoed on
# failure; `parallel --halt now,fail=1` propagates a probe crash.
set -euo pipefail

PROBE_ERR="$(mktemp)"
PROBE_OUT="$(mktemp)"
trap 'rm -f "$PROBE_ERR" "$PROBE_OUT"' EXIT
probe_crash() {
    echo "ERROR: a staleness probe crashed (exit $1) — output is NOT trustworthy:" >&2
    cat "$PROBE_ERR" >&2
    exit 1
}

if [ "${NROS_SKIP_FIXTURE_CHECK:-0}" != "0" ]; then
    exit 0
fi

# RFC-0061 / phase-318 W4 — scope the gate to the LANE that is running.
#
# This gate used to cover every platform unconditionally, which is how a
# native-intent `just ci` came to be blocked by a stale ThreadX fixture
# (2026-07-28: every code stage passed, then 40 cross-platform workspace
# fixtures failed the preflight). A tier's gate must cover exactly that tier's
# fixtures — otherwise the cheap lane inherits the expensive lane's cost and
# stops being runnable per task, which is how `NROS_SKIP_FIXTURE_CHECK=1`
# became routine.
#
#   NROS_FIXTURE_SCOPE=native   tier 1 — host fixtures only
#   NROS_FIXTURE_SCOPE=all      tier 3 — everything (default, unchanged)
#   NROS_FIXTURE_SCOPE=coords   tier 2 — the coordinates the lane selected;
#                               NROS_FIXTURE_COORDS names the file, produced by
#                               `lane-coords <lane>` from the SAME computation the
#                               build used, so gate and build cannot disagree
SCOPE="${NROS_FIXTURE_SCOPE:-all}"
scope_args=()
case "$SCOPE" in
    all) ;;
    # The SCOPE name is the lane (`native`, kept); the --platform value is the
    # fixture TOKEN (`linux` since phase-337 W8.c). Different vocabularies.
    native) scope_args=(--platform linux) ;;
    coords)
        if [ -z "${NROS_FIXTURE_COORDS:-}" ] || [ ! -s "${NROS_FIXTURE_COORDS}" ]; then
            # Silently degrading to "check nothing" would report a lane green
            # having verified none of it — the exact laundering this gate exists
            # to prevent.
            echo "ERROR: NROS_FIXTURE_SCOPE=coords needs NROS_FIXTURE_COORDS to name" >&2
            echo "       a non-empty coordinate file (see: lane-coords <lane>)" >&2
            exit 2
        fi
        scope_args=(--coords-from "$NROS_FIXTURE_COORDS")
        ;;
    *)
        echo "ERROR: NROS_FIXTURE_SCOPE must be 'native', 'coords' or 'all' (got '$SCOPE')" >&2
        exit 2
        ;;
esac

# Issue 0443 — say what is being audited, and where the scope came from.
#
# The defect was that `ci-matrix` set the lane and not the scope, so this gate
# silently audited the whole tier-3 set while the run, the build and the stamp
# were all tier 2. Nothing detected it BECAUSE the gate never said which set it
# had chosen: `all` is a legitimate value, and a green line looks identical
# whether it covered three coordinates or forty-seven. The derivation in
# `_check-fixtures-stale` stops the mismatch happening; this line is what would
# have made it visible the first time, and what makes the next one visible.
#
# Same lesson as issue 0445 on the test side: a gate that does not report its
# own scope cannot be caught having the wrong one.
echo "check-fixtures-stale: scope=${SCOPE} (${NROS_FIXTURE_SCOPE_ORIGIN:-direct})\
${NROS_FIXTURE_COORDS:+ coords=$NROS_FIXTURE_COORDS ($(wc -l < "$NROS_FIXTURE_COORDS") coordinate(s))}"

cmake_records() {
    python3 scripts/build/fixtures-manifest.py list --for-probe --lang c "${scope_args[@]}"
    python3 scripts/build/fixtures-manifest.py list --for-probe --lang cpp "${scope_args[@]}"
}

cmake_stale=()
if command -v parallel >/dev/null 2>&1; then
    cmake_records | parallel --halt now,fail=1 --jobs "$(nproc)" \
        bash scripts/test/cmake-fixture-stale.sh {} >"$PROBE_OUT" 2>>"$PROBE_ERR" \
        || probe_crash $?
    mapfile -t cmake_stale < "$PROBE_OUT"
else
    while IFS= read -r line; do
        out="$(bash scripts/test/cmake-fixture-stale.sh "$line")"
        [ -n "$out" ] && cmake_stale+=("$out")
    done < <(cmake_records)
fi
if [ ${#cmake_stale[@]} -gt 0 ]; then
    echo "WARNING: ${#cmake_stale[@]} C/C++ fixture cell(s) were STALE and have now been rebuilt (cmake):" >&2
    printf '  %s\n' "${cmake_stale[@]}" >&2
    echo "  (cmake/ninja incremental self-heal; bypass with  NROS_SKIP_FIXTURE_CHECK=1 )" >&2
fi

# Issue 0466 — REPORT EVERY STALE FAMILY, then exit once.
#
# This gate checks three families (rust, workspace, compile-check) and each used
# to `exit 1` on its own. So a tree with two stale families reported ONE, you
# rebuilt it, and the next run reported the next — one discovery per attempt,
# which is the very thing `check-tier-preconditions` exists to prevent and which
# cost four rebuild-and-rerun rounds on 2026-08-15 (two workspace fixtures, then
# ten compile-checks, then the main set).
#
# The probes were already independent; only the early exits coupled them. On the
# success path this costs nothing — all three ran anyway. On the failure path it
# pays for the later families in order to name them, which is the trade the
# rounds already paid, serially, with a rebuild in between.
stale_families=0

# BUILDER-keyed, not language-keyed (phase-344 W2's rule, applied here at last).
#
# `is_cargo_row` stopped keying on `lang` when the six
# `examples/qemu-riscv64-threadx/rust/*` cyclonedds rows turned out to build
# through cmake; this probe kept `--lang rust` and so kept handing those rows —
# twelve of them, zenoh and cyclonedds — to `cargo build`. A threadx C/C++ leaf
# cannot be built that way, so the probe failed on all twelve EVERY run:
#
#   ERROR: 12 rust fixture(s) could NOT be built by the staleness probe
#     error: could not compile `nros-rmw-zenoh-staticlib` (lib)
#
# and a row the probe cannot build is never fresh, so it is stale again on the
# next run. That is not a treadmill converging — it is a fixed point, and it
# cost ~190 test failures on every `just ci-matrix` (issue 0828's neighbourhood:
# a row in the run set that no lane can make fresh). The rows were ALSO in the
# cmake list, where they self-healed correctly, so each was reported twice under
# two labels — which is why the ERROR block named them `build-zenoh` with no
# leaf path and read as unattributable rather than as a partition bug.
rust_stale=()
if command -v parallel >/dev/null 2>&1; then
    python3 scripts/build/fixtures-manifest.py list --for-probe --with-platform --builder cargo "${scope_args[@]}" \
        | parallel --halt now,fail=1 --jobs "$(nproc)" \
        bash scripts/test/rust-fixture-stale.sh {} >"$PROBE_OUT" 2>>"$PROBE_ERR" \
        || probe_crash $?
    mapfile -t rust_stale < "$PROBE_OUT"
else
    while IFS= read -r line; do
        out="$(bash scripts/test/rust-fixture-stale.sh "$line")"
        [ -n "$out" ] && rust_stale+=("$out")
    done < <(python3 scripts/build/fixtures-manifest.py list --for-probe --with-platform --builder cargo "${scope_args[@]}")
fi
# issue 0466 — split the probe's two outcomes. "Stale, and cargo rebuilt it" is a
# WARNING because the artifact now exists; "could not build" is an ERROR, because
# nothing was verified and no artifact was produced. Folding the second into the
# first is how a green lane preceded ~100 tests panicking with "Test fixture
# binary not prebuilt": the probe swallowed cargo's failure, so the gate reported
# self-heal for a fixture that had never compiled.
rust_failed=()
rust_rebuilt=()
for line in ${rust_stale[@]+"${rust_stale[@]}"}; do
    case "$line" in
        FAILED$'\t'*) rust_failed+=("${line#FAILED$'\t'}") ;;
        *)            rust_rebuilt+=("$line") ;;
    esac
done

if [ ${#rust_rebuilt[@]} -gt 0 ]; then
    echo "WARNING: ${#rust_rebuilt[@]} rust fixture(s) were STALE and have now been rebuilt by cargo:" >&2
    printf '  %s\n' "${rust_rebuilt[@]}" >&2
    echo "  (cargo incremental self-heal; bypass with  NROS_SKIP_FIXTURE_CHECK=1 )" >&2
fi

if [ ${#rust_failed[@]} -gt 0 ]; then
    echo "ERROR: ${#rust_failed[@]} rust fixture(s) could NOT be built by the staleness probe:" >&2
    printf '  %s\n' "${rust_failed[@]}" | sed 's/\t/\n      /' >&2
    echo "  These are NOT fresh — the probe never verified them and no artifact was" >&2
    echo "  produced, so the tests that consume them will fail with" >&2
    echo "  \"Test fixture binary not prebuilt\". Build the lane before testing:" >&2
    echo "      source ./activate.sh && just build-test-fixtures lane=<the lane you will test>" >&2
    echo "  A leaf that needs codegen also needs \`nros sync\` first (its generated/" >&2
    echo "  tree is not in a fresh clone). Bypass with  NROS_SKIP_FIXTURE_CHECK=1 ." >&2
    stale_families=$((stale_families + 1))
fi

# issue 0030 — gate the workspace-fixture preflight on cross-toolchain presence.
# build-test-fixtures builds each platform's workspace Entry via that platform's
# `build-examples` lane, which skips cleanly when its cross toolchain is absent.
# So on a lighter tier the fixture legitimately does not exist — requiring it
# would hard-fail the WHOLE `test-all` preflight, even though the matching e2e
# test already `skip!`s at runtime on the absent binary. Mirror the
# embedded-Cyclone gate in the `test-all` recipe (justfile): require a cross
# workspace fixture only when its toolchain is present; otherwise drop it from
# the required set with an info note.
#
# phase-320 W1.d — this used to say esp32 was "excluded entirely via
# `skip_probe = true` … not in the build-test-fixtures fan-out". esp32 IS in the
# fan-out (justfile `build-test-fixtures`), and has been for a while; the
# skip_probe justification had simply never been revisited, so a fixture with a
# real two-way QEMU e2e sat outside the staleness gate — the museum-binary class
# (issues 0148/0164/0196). It now rides the toolchain-conditional path instead.
# Only the cargo/cmake-lane workspace fixtures (freertos, threadx-linux, plus the
# always-host native/c/cpp/mixed rows) reach this probe and write the
# `.nros-workspace-fixture.*.inputsig` stamp the stale check demands.
# zephyr/nuttx remain `skip_probe = true` own-lane artifacts (west / nuttx
# machinery, each with its own sig) and never appear here.
source scripts/test/toolchain-gate.sh   # phase-300 W4 — shared predicate
workspace_toolchain_present() {
    case "$1" in
        workspace-rust-qemu-freertos) nros_toolchain_present arm-none-eabi ;;
        workspace-rust-threadx-linux) nros_toolchain_present threadx ;;
        workspace-rust-esp32) nros_toolchain_present esp32 ;;
        *) return 0 ;;
    esac
}

workspace_records=()
while IFS= read -r line; do
    id="${line%%$'\x1f'*}"
    if workspace_toolchain_present "$id"; then
        workspace_records+=("$line")
    else
        echo "info: workspace fixture '$id' not required in preflight — cross toolchain absent (issue 0030)" >&2
    fi
done < <(python3 scripts/build/fixtures-manifest.py list-workspaces --for-probe "${scope_args[@]}")

workspace_stale=()
if [ ${#workspace_records[@]} -eq 0 ]; then
    :
elif command -v parallel >/dev/null 2>&1; then
    printf '%s\n' "${workspace_records[@]}" \
        | parallel --halt now,fail=1 --jobs "$(nproc)" \
        bash scripts/test/workspace-fixture-stale.sh {} >"$PROBE_OUT" 2>>"$PROBE_ERR" \
        || probe_crash $?
    mapfile -t workspace_stale < "$PROBE_OUT"
else
    for line in "${workspace_records[@]}"; do
        out="$(bash scripts/test/workspace-fixture-stale.sh "$line")"
        [ -n "$out" ] && workspace_stale+=("$out")
    done
fi
if [ ${#workspace_stale[@]} -gt 0 ]; then
    echo "ERROR: ${#workspace_stale[@]} workspace fixture(s) are missing or stale:" >&2
    printf '  %s\n' "${workspace_stale[@]}" >&2
    echo "  Run \`just native build-workspace-fixtures\` before test-all." >&2
    echo "  (bypass with  NROS_SKIP_FIXTURE_CHECK=1 )" >&2
    stale_families=$((stale_families + 1))
fi

# phase-319 W3 (issue 0351) — the compile-check lane, which this gate could not
# see at all: its inventory lived in hardcoded shell arrays until W2 moved it
# into the manifest, so a lane that had stopped building entirely (issue 0350)
# passed here for three days. Same probe shape as the workspace fan-out above.
compile_check_records=()
while IFS= read -r line; do
    [ -n "$line" ] && compile_check_records+=("$line")
done < <(
    # issue 0554 — a NATIVE run must not demand west-built rows.
    #
    # `list-compile-checks` returns every row regardless of builder, and
    # `#536 / phase-350 W2` added four west compile-checks. TWO builders, not
    # one — `west-build` (1 row) and `west-configure` (3) — so the predicate is
    # the `west-` PREFIX. Matching the literal `west-build` would have dropped
    # one of the four and left the other three failing exactly as before.
    # Their own manifest comment says it plainly: "Built by the WEST lane
    # (west-fixtures.sh), never by compile-check-fixtures.sh: west needs a
    # provisioned Zephyr workspace, so the lane that owns one runs them."
    #
    # So `NROS_FIXTURE_SCOPE=native` — tier 1, host fixtures only — started
    # failing on four `.inputsig` files the native lane has no way to produce,
    # and `just ci` could not reach a single test. That is #482's distinction:
    # which fixtures must be FRESH is the lane's cell cover, not every row in
    # the manifest.
    #
    # Scoped to `native` deliberately. `all` (tier 3) and `coords` (tier 2)
    # keep demanding them: those lanes either build west or select by
    # coordinate, and silently dropping a west row there would hide a real
    # staleness — the failure mode this gate exists to prevent.
    if [ "$SCOPE" = "native" ]; then
        python3 scripts/build/fixtures-manifest.py list-compile-checks 2>/dev/null \
            | awk -F'\x1f' '$2 !~ /^west-/'
    else
        python3 scripts/build/fixtures-manifest.py list-compile-checks 2>/dev/null
    fi
)

compile_check_stale=()
if [ ${#compile_check_records[@]} -eq 0 ]; then
    :
elif command -v parallel >/dev/null 2>&1; then
    printf '%s\n' "${compile_check_records[@]}" \
        | parallel --halt now,fail=1 --jobs "$(nproc)" \
        bash scripts/test/compile-check-stale.sh {} >"$PROBE_OUT" 2>>"$PROBE_ERR" \
        || probe_crash $?
    mapfile -t compile_check_stale < "$PROBE_OUT"
else
    for line in "${compile_check_records[@]}"; do
        out="$(bash scripts/test/compile-check-stale.sh "$line")"
        [ -n "$out" ] && compile_check_stale+=("$out")
    done
fi
if [ ${#compile_check_stale[@]} -gt 0 ]; then
    echo "ERROR: ${#compile_check_stale[@]} compile-check fixture(s) are missing or stale:" >&2
    printf '  %s\n' "${compile_check_stale[@]}" >&2

    # issue 0599 direction (3) — name the CAUSE, not just the artifact. These
    # rows are built by the west lane and by nothing else, so when the Zephyr
    # workspace is unprovisioned the lane SKIPPED and the remedy below is the
    # command that just reported success. Telling the reader to re-run it sends
    # them round a twenty-minute loop that cannot succeed.
    #
    # The west-owned ids are DERIVED from the manifest (`[[compile_check_fixture]]`
    # with a `west-*` builder), not listed here: a hand-copied list is the thing
    # that goes stale when a row is added, and this file would have no way to
    # notice.
    _west_ids="$(awk '
        # `in_block` is load-bearing: `id`/`builder` keys appear in [[fixture]]
        # and [[workspace_fixture]] too, and without the flag an id from one
        # block pairs with a builder from another — the first cut of this
        # emitted 58 rows that do not exist.
        /^\[\[/            { in_block = ($0 ~ /^\[\[compile_check_fixture\]\]/); id=""; next }
        !in_block          { next }
        /^id[ \t]*=/        { v=$0; sub(/^[^"]*"/, "", v); sub(/".*$/, "", v); id=v }
        /^builder[ \t]*=/   { v=$0; sub(/^[^"]*"/, "", v); sub(/".*$/, "", v)
                             if (v ~ /^west/ && id != "") print id }
    ' examples/fixtures.toml 2>/dev/null)"
    _west_missing=""
    for _id in $_west_ids; do
        for _row in "${compile_check_stale[@]}"; do
            case "$_row" in *"$_id"*) _west_missing="$_west_missing $_id" ;; esac
        done
    done
    if [ -n "$_west_missing" ]; then
        _zws="${NROS_ZEPHYR_WORKSPACE:-}"
        if [ -z "$_zws" ]; then
            for _c in zephyr-workspace ../nano-ros-workspace; do
                [ -d "$_c/zephyr" ] && _zws="$_c" && break
            done
        fi
        if [ -z "$_zws" ] || [ ! -d "$_zws/zephyr" ]; then
            echo "" >&2
            echo "  CAUSE: no provisioned Zephyr workspace, so the west lane SKIPPED." >&2
            echo "  These are built by that lane and by no other, and they are" >&2
            echo "  unattributable to a coordinate, so every run scope requires them:" >&2
            echo "   $_west_missing" >&2
            echo "  Re-running the build below will skip the lane again and report OK." >&2
            echo "  Provision first:  just zephyr setup" >&2
            echo "" >&2
        fi
    fi

    echo "  Run \`just build-test-fixtures\` before test-all." >&2
    echo "  (bypass with  NROS_SKIP_FIXTURE_CHECK=1 )" >&2
    stale_families=$((stale_families + 1))
fi

# One exit, after every family has had its say (issue 0466).
if [ "$stale_families" -gt 0 ]; then
    echo "" >&2
    echo "check-fixtures-stale: $stale_families fixture family/families are stale — all of" >&2
    echo "  them are listed above, deliberately: rebuilding one re-stamps inputs the" >&2
    echo "  next one's signature covers, so discovering them one per run costs a" >&2
    echo "  rebuild cycle each time." >&2
    exit 1
fi
