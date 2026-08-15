#!/usr/bin/env bash
# Build-stage zephyr west fixtures (issue 0041 — No compilation inside tests).
# `west build` a zephyr bringup fixture into build/west-fixtures/<id>/, stamping
# `.compile-ok`. Tests inspect the build dir (baked artifacts / CMakeCache /
# zephyr.exe) instead of running west at run time.
#
# Gated: skips cleanly (no stamp → test skips/deselects per tier) when west or a
# provisioned Zephyr workspace is unavailable.
set -u

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../.." && pwd)"
cd "$repo_root"

# RFC-0070 R1/R3 (phase-334 W2.b step 2) — root from the ONE derivation, with
# `NROS_REPO_ROOT` pinned to this script's own repo root so the emitted path is
# byte-identical to the literal it replaces. Paired in the same commit with
# `nros_tests::fixtures::require_west_fixture`, the only reader of this tree.
NROS_REPO_ROOT="$repo_root"
# shellcheck source=scripts/build/build-root.sh
source "$repo_root/scripts/build/build-root.sh"

out_root="$(nros_build_dir "$NROS_KIND_WEST_FIXTURES")"
mkdir -p "$out_root"

# ZEPHYR_BASE: discover the provisioned west workspace the same way the
# `just zephyr` recipes resolve ZEPHYR_WORKSPACE (just/zephyr.just) — an explicit
# `NROS_ZEPHYR_WORKSPACE`, then the in-repo `zephyr-workspace/`, then the sibling
# `../nano-ros-workspace[-4.4]/` checkouts a `just zephyr setup` lands. Without
# this the fixture only saw the in-repo path and skipped whenever the workspace
# lived in the sibling (the common `just zephyr setup` layout).
if [ -z "${ZEPHYR_BASE:-}" ]; then
    for _ws in \
        "${NROS_ZEPHYR_WORKSPACE:-}" \
        "$repo_root/zephyr-workspace" \
        "$repo_root/../nano-ros-workspace" \
        "$repo_root/../nano-ros-workspace-4.4"; do
        if [ -n "$_ws" ] && [ -d "$_ws/zephyr" ]; then
            export ZEPHYR_BASE="$_ws/zephyr"
            break
        fi
    done
fi

if ! command -v west >/dev/null 2>&1; then
    echo "west-fixtures: west unavailable — skipping" >&2
    exit 0
fi
if [ -z "${ZEPHYR_BASE:-}" ] || [ ! -d "$ZEPHYR_BASE" ]; then
    echo "west-fixtures: ZEPHYR_BASE unset/invalid — skipping" >&2
    exit 0
fi

# #185 (the #182 guard, west edition) — every west fixture's bake runs the
# `nros` CLI (nros_system_generate / nros_generate_interfaces), so the fixture
# is a function of the CODEGEN TOOL. Stamp the CLI's content hash next to the
# date; `require_west_fixture` compares it and fails loud on a stale-tool
# fixture instead of soft-passing a museum bake (the #185 half-bake red
# herring). A date-only legacy stamp reads as stale — one rebuild refreshes.
west_fixture_stamp() {
    local bld="$1"
    # phase-350 W2 — the BUILDER rides in the stamp. A `west-configure` fixture
    # and a `west-build` one leave different things on disk, and a consumer that
    # cannot tell them apart is how a build-only lane reads as covered (the
    # failure recorded in issue 0537). Optional so an older stamp still parses.
    local builder="${2:-}"
    local nros_bin="$repo_root/packages/cli/target/release/nros"
    {
        date -u +%Y-%m-%dT%H:%M:%SZ
        if [ -x "$nros_bin" ]; then
            printf 'tool:nros=%s\n' "$(sha256sum "$nros_bin" | awk '{print $1}')"
        else
            printf 'tool:nros=absent\n'
        fi
        [ -z "$builder" ] || printf 'builder=%s\n' "$builder"
    } > "$bld/.compile-ok"
}

# Issue 0574 — ALSO write the `.inputsig` the compile-check staleness probe
# reads, at the root that probe derives.
#
# These four rows are compile-checks that this lane owns because west needs a
# provisioned Zephyr workspace. `compile-check-fixtures.sh` is the only other
# writer of `.inputsig`, and its builder loop covers `cargo-check cargo-build
# cross-build cmake-configure cxx-syntax` — not `west-build` / `west-configure`.
# So nothing wrote these, while `check-fixtures-stale.sh` requires them under
# scope `coords` (tier 2) and `all` (tier 3): a COMPLETE, green
# `build-test-fixtures lane=tier2` was followed by `ci-matrix` failing at
# `_lane-gate` before one test ran, and rebuilding could never help. Tier 1
# escaped only because that gate exempts west rows for `SCOPE = native`.
#
# `.compile-ok` (above) is this lane's own stamp and stays — it records the
# BUILDER, which is what makes "configure only" checkable (phase-350 W2). It
# answers a different question from `.inputsig`, which is "built from the
# sources on disk right now".
#
# The signature MUST be computed from the WHOLE record line, because that is
# what `compile-check-stale.sh` passes when it recomputes the expected value —
# hashing a reconstructed 8-field prefix here would read as permanently stale.
write_compile_check_inputsig() {
    local record="$1" id="$2"
    local stamp_dir
    stamp_dir="$(nros_build_dir "$NROS_KIND_COMPILE_CHECK" "$id")"
    mkdir -p "$stamp_dir"
    bash "$repo_root/scripts/build/compile-check-signature.sh" "$record" \
        > "$stamp_dir/.inputsig" 2>/dev/null || rm -f "$stamp_dir/.inputsig"
}

# phase-350 W2 (issue 0536) — the leaf table is `examples/fixtures.toml`, read
# through `fixtures-manifest.py list-compile-checks --builder west-*`.
#
# This used to be two bash arrays (`WEST_FIXTURES`, `SELF_PKG_FIXTURES`), each
# with its own colon-delimited format — a second spelling of a matrix the
# manifest owns (issue 0535), and the reason these four fixtures had no
# coordinate and no `output` anyone could check.
#
# The BUILDER is the contract now:
#
#   west-build      full `west build`; `output` is the image
#   west-configure  `west build --cmake-only`; `output` is a configure artifact
#
# Three of the four are `west-configure`. Their consumers read a CMakeCache
# variable or a baked `system_config.h` and never touch an image, so they no
# longer pay for a kernel link. The self-pkg pair in particular used to run a
# link this script itself called doomed, discard the failure, and stamp on a
# file written before the link started.
#
# The stamp records the BUILDER, so "configure only" is a property a consumer
# can read rather than a claim in a comment — and `output` is the gate either
# way, which is what makes the two shapes distinguishable at all.
records="$(python3 "$repo_root/scripts/build/fixtures-manifest.py" \
    list-compile-checks --builder west-build; \
    python3 "$repo_root/scripts/build/fixtures-manifest.py" \
    list-compile-checks --builder west-configure)"

n=0
total=0
reused=0
while IFS= read -r record; do
    [ -n "$record" ] || continue
    IFS=$'\x1f' read -r id builder src _pkg _manifest_dir _target _profiles output subdir board extra \
        <<< "$record"
    [ -n "$id" ] || continue
    total=$((total + 1))
    [ -d "$repo_root/$src" ] || { echo "west-fixtures: src missing: $src" >&2; continue; }
    bld="$out_root/$id"
    echo "== west-fixture: $id ($builder, board=${board:-board.cmake}) =="

    # phase-353 W2 (issue 0509) — REUSE an up-to-date build instead of deleting
    # it.
    #
    # `rm -rf "$bld"` ran unconditionally, so this lane had no warm state by
    # construction: every invocation was a cold `west build`. That is what issue
    # 0509 measured as seven consecutive no-op runs each replaying 1244 ninja
    # edges and a 129-crate cargo rebuild of `nros-c`, with byte-identical logs.
    # The lane's own direction list leads with "skip per-leaf prep whose inputs
    # are unchanged"; this is that, at the only place that can decide it.
    #
    # The freshness question was already answered here and thrown away: the
    # signature written below (`write_compile_check_inputsig`) hashes the
    # manifest record, the row's source tree AND the nros CLI's codegen
    # fingerprint. Reading it back is the whole change. Using the SAME signature
    # the test-side probe recomputes is issue 0196's rule — a build-side probe
    # that watches less than the gate lets a museum bake pass.
    #
    # Reuse requires ALL of: an identical signature, the declared `output` still
    # present, and this lane's own `.compile-ok` stamp. Any doubt falls through
    # to the unconditional wipe below, so the failure mode is the old cost, not
    # a stale fixture.
    _wf_sig_dir="$(nros_build_dir "$NROS_KIND_COMPILE_CHECK" "$id")"
    _wf_want="$(bash "$repo_root/scripts/build/compile-check-signature.sh" "$record" 2>/dev/null || true)"
    _wf_have="$(cat "$_wf_sig_dir/.inputsig" 2>/dev/null || true)"
    if [ -n "$_wf_want" ] && [ "$_wf_want" = "$_wf_have" ] &&
        [ -e "$bld/$output" ] && [ -f "$bld/.compile-ok" ]; then
        echo "   reused $bld ($output) — inputs unchanged"
        n=$((n + 1))
        reused=$((reused + 1))
        continue
    fi

    rm -rf "$bld"
    # issue 0533 — resolve each bringup's SystemModel before the west build.
    # The model is a BUILD ARTIFACT (phase-330 W4.a stopped committing them), so
    # a fresh clone has none and `nros_system_generate` fails the configure with
    # "declares system semantics but no SystemModel was found". This lane is
    # invoked with `|| true`, so that failure was INVISIBLE: the build reported
    # success, no stamp was written, and the test failed much later with
    # "fixture binary not prebuilt". Same masking shape as #0510.
    #
    # Sync runs INSIDE the bringup dir, not the workspace root: these fixtures
    # keep their packages at the root rather than under `src/`, which `nros sync`
    # rejects outright — and the shim resolves such a bringup as its OWN
    # workspace (`_nros_system_detect_self_pkg`), so `<bringup>/build/nros/models/`
    # is exactly where the configure looks.
    _wf_cli="${NROS_CLI_BIN:-$repo_root/packages/cli/target/release/nros}"
    if [ -x "$_wf_cli" ]; then
        for _bringup in "$repo_root/$src"/*/; do
            [ -f "$_bringup/system.toml" ] || continue
            ( cd "$_bringup" && "$_wf_cli" sync >/dev/null 2>&1 ) \
                || echo "   nros sync failed in $(basename "$_bringup") (configure may fail)" >&2
        done
    fi
    args=(build -d "$bld")
    [ "$builder" = "west-configure" ] && args+=(--cmake-only)
    [ -n "$board" ] && args+=(-b "$board")
    args+=("$repo_root/$src/$subdir")
    [ -n "$extra" ] && args+=(-- "$extra")
    # issue #87 — native_sim builds with host gcc (no Zephyr SDK); board-keyed,
    # so the FVP board_import entry (empty board → board.cmake) stays SDK-gated.
    tc_env=()
    case "$board" in
        native_sim*) [ -z "${ZEPHYR_TOOLCHAIN_VARIANT:-}" ] && tc_env=(ZEPHYR_TOOLCHAIN_VARIANT=host) ;;
    esac
    # The stamp gate is `output` EXISTS, for both builders — not west's exit
    # code. A `west-configure` row is expected to stop before linking, and a
    # `west-build` row that exits 0 without its image is not built either. One
    # rule, and it is the row's own declaration.
    env "${tc_env[@]}" west "${args[@]}" || true
    if [ -e "$bld/$output" ]; then
        west_fixture_stamp "$bld" "$builder"
        write_compile_check_inputsig "$record" "$id"
        echo "   ok $bld ($output)"
        n=$((n + 1))
    else
        echo "   MISSING $output for $id (no stamp; the test will report)" >&2
    fi
done <<< "$records"
echo "west fixtures: $n/$total ok ($reused reused, $((n - reused)) built)."
