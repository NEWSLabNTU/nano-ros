#!/usr/bin/env bash
# Phase 226.D — shared fixture-only Cargo target dir resolver.
#
# Compatible standalone Rust fixture rows (same platform, target triple,
# profile, no-default flag, sorted features, sorted env, build-std/nightly
# mode, generated sync mode) get ONE fixture-only `--target-dir` so the
# shared nano-ros crates (nros-c, …) compile once for the whole group
# instead of once per example dir.
#
# Manual `cargo build` inside an example is NOT affected — only the
# fixture build path (scripts/build/fixtures-build.sh) and the stale
# probe (scripts/test/rust-fixture-stale.sh) call this, and they MUST
# call the SAME resolver so the probe inspects the artifact tree the
# build actually wrote.
#
# Eligibility — returns nothing (caller keeps the example-local
# `target/`) unless the platform has been migrated to a shared fixture dir
# (qemu-arm-baremetal — see NROS_FIXTURE_SHARED_PLATFORMS).
#
# phase-340 W2 (work-order item 5) — an authored `--target-dir` used to be a
# SECOND opt-out: rows spelling `target-zenoh` / `target-safety` stayed
# per-example whatever the platform said. Those authored dirs ARE the R1
# duplicate population phase-340 measures (~21:1 tree-wide), so the opt-out was
# excluding exactly the rows worth grouping. An authored dir now names a GROUP
# instead: the flag is STRIPPED from the row's cargo args
# (`nros_fixture_strip_authored_target_dir`) and the group's dir replaces it.
# The authored STRING carries no information the group key lacks — it is a
# function of the coordinate the key already contains (phase-334 W2.b measured
# 128 of 137 reproducible from kind/platform/rmw and the other 9 from the
# feature signature) — which is why phase-334 W2.b decided the manifest column
# is deleted rather than derived.
#
# The emitted dir is repo-root-ABSOLUTE (`$NROS_REPO_ROOT/build/
# fixtures-cargo/<group>`): fixtures-build.sh runs cargo after
# `cd "$dir"`, so a relative dir would resolve against the example dir.

# RFC-0070 R1/R3 — the root comes from `nros_build_root`, not a literal, so
# `NROS_BUILD_ROOT` moves this family with everything else. The default is
# `<repo>/build`, so the emitted path is unchanged (phase-334 W2.b step 1:
# derivation first, paths later).
#
# Sourced HERE at file scope, not inside `nros_fixture_target_dir_flag`. Step 1
# put it in the function and that silently broke the `export -f` path below: a
# make leaf is a fresh bash where the function came from the ENVIRONMENT, so
# `${BASH_SOURCE[0]}` is not a file path, `. "$(dirname …)/build-root.sh"`
# resolved to `./build-root.sh`, and the resolver emitted ` --target-dir ` with
# an EMPTY value — the build writing somewhere the probe and the test resolver
# would never look. Sourcing at file scope makes the two helpers ordinary
# functions of this file, shipped to the leaf by the same `export -f`.
# shellcheck source=scripts/build/build-root.sh
. "$(dirname "${BASH_SOURCE[0]}")/build-root.sh"

# Platforms whose default Rust fixture rows share one fixture target dir.
# ESP32 flash packaging and RTOS rows are deferred to a later patch
# (they carry extra postprocessing that reads fixed per-fixture paths).
# MUST be `export`ed: fixtures-build.sh schedules eligible rows as `make`
# leaves and ships the resolver functions to those leaves via `export -f`.
# A make leaf is a fresh bash that inherits this var only through the
# environment — a plain (non-exported) assignment is visible in the
# sourcing process but vanishes in the leaf, making every row look
# ineligible and silently fall back to the example-local `target/`.
export NROS_FIXTURE_SHARED_PLATFORMS="${NROS_FIXTURE_SHARED_PLATFORMS:-qemu-arm-baremetal}"

# _nros_fixture_variant_sig <cargo-args> <envstr>
# Signature of everything in the grouping key BEYOND platform+triple.
# `--target <triple>` is dropped (the triple is implied 1:1 by the
# migrated platforms) and `--target-dir` is dropped because WHERE a build
# writes is not part of WHAT it compiles — measured in phase-340's
# incompatibility table: "same build, different target dir" is the one factor
# that does NOT change `-C metadata`. (It used to be dropped "handled by
# eligibility", back when an authored dir opted the row out entirely.)
# `--no-default-features` / `--features` and the sorted env DO change
# shared-crate compilation and are kept. Empty signature = the default group
# (slug is just the platform).
_nros_fixture_variant_sig() {
    local cargo_args="${1:-}" envstr="${2:-}"
    # shellcheck disable=SC2206
    local toks=($cargo_args)
    local filtered=() i=0
    while [ $i -lt ${#toks[@]} ]; do
        case "${toks[$i]}" in
            --target | --target-dir)
                i=$((i + 2))
                continue
                ;;
        esac
        filtered+=("${toks[$i]}")
        i=$((i + 1))
    done
    local feats="${filtered[*]:-}"
    local env_sorted=""
    if [ -n "$envstr" ]; then
        # shellcheck disable=SC2086
        env_sorted="$(printf '%s\n' $envstr | LC_ALL=C sort | tr '\n' ' ')"
    fi
    local sig="${feats}|${env_sorted}"
    [ "$sig" = "|" ] && sig=""
    printf '%s' "$sig"
}

# nros_fixture_group_slug <platform> <cargo-args> <envstr>
# The group KEY. Always echoes a slug — eligibility is NOT its business.
#
# Split out of `nros_fixture_group` by phase-340 W2: the two questions had one
# spelling, so nothing outside the migrated platforms could ask "which group
# WOULD this row land in?". `check-fixture-groups` has to ask exactly that (it
# checks the preconditions for migrating a platform, which by definition is not
# migrated yet), and a gate that re-derived the key itself would pass against
# its own copy of the old rule.
nros_fixture_group_slug() {
    local platform="$1" cargo_args="${2:-}" envstr="${3:-}"
    local variant
    variant="$(_nros_fixture_variant_sig "$cargo_args" "$envstr")"
    if [ -z "$variant" ]; then
        printf '%s' "$platform"
    else
        # `set --` instead of `| cut -d' ' -f1`: cksum prints "<sum> <bytes>",
        # and word-splitting it is one fewer exec per row. The batch driver
        # below runs this over the whole manifest, where the pipeline's third
        # process was a measurable share of the cost.
        # shellcheck disable=SC2046
        set -- $(printf '%s' "$variant" | cksum)
        printf '%s-%s' "$platform" "$1"
    fi
}

# nros_fixture_platform_is_shared <platform>
# True when the platform has been migrated to a shared fixture target dir.
# ONE spelling of the eligibility rule, split out so a caller can ask the
# question WITHOUT paying for the key (`nros_fixture_group` is a command
# substitution, i.e. a fork, and the batch driver below asks this once per row).
nros_fixture_platform_is_shared() {
    case " $NROS_FIXTURE_SHARED_PLATFORMS " in
        *" $1 "*) return 0 ;;
        *) return 1 ;;
    esac
}

# nros_fixture_group <platform> <cargo-args> <envstr>
# Echoes the group slug for an ELIGIBLE row, or nothing (not eligible).
nros_fixture_group() {
    local platform="$1" cargo_args="${2:-}" envstr="${3:-}"
    nros_fixture_platform_is_shared "$platform" || return 0
    nros_fixture_group_slug "$platform" "$cargo_args" "$envstr"
}

# nros_fixture_group_batch
# stdin:  one `<platform>\x1f<cargo-args>\x1f<envstr>` record per line.
# stdout: one `<slug>\x1f<eligible 0|1>` record per line, SAME ORDER.
#
# The batch driver lives HERE, in the file that owns the key, because more than
# one consumer needs "resolve the whole manifest's groups in one process":
# `check-fixture-groups.py` (the migration precondition gate) and
# `fixtures-manifest.py fixture-groups` (the export the Rust test resolver
# reads, phase-340 B2). Each had to spawn `bash` once rather than once per row —
# 240 spawns would make `check-fast` crawl — and the first version of that loop
# was written out longhand inside the gate. A second consumer copying it is the
# R3 drift this phase keeps deleting, so the loop is a function of this file and
# both callers are `bash -c '. fixtures-target-dir.sh; nros_fixture_group_batch'`.
#
# Emits BOTH answers because the two questions are genuinely different and this
# file is where they are separated (see `nros_fixture_group_slug` vs
# `nros_fixture_group`): the gate asks "which group WOULD this row land in?"
# about platforms that are by definition not migrated, while the resolver asks
# "is this row's build actually redirected today?".
#
# Memoised on the raw record because a manifest is overwhelmingly repetitive —
# 122 cargo rows resolve to 19 distinct slugs, and the default group alone is 38
# identical `linux||` records. Each miss costs a subshell plus a `cksum` exec;
# the first version recomputed every row and took 583 ms, which the Rust
# resolver pays once per TEST PROCESS. Memoised (and with the eligibility fork
# removed) it is ~15x cheaper. The memo is per-invocation, so it can never go
# stale.
nros_fixture_group_batch() {
    local platform args envstr slug eligible record
    declare -A _slug_memo=()
    while IFS=$'\x1f' read -r platform args envstr; do
        record="${platform}|${args}|${envstr}"
        slug="${_slug_memo[$record]:-}"
        if [ -z "$slug" ]; then
            slug="$(nros_fixture_group_slug "$platform" "$args" "$envstr")"
            _slug_memo[$record]="$slug"
        fi
        if nros_fixture_platform_is_shared "$platform"; then
            eligible=1
        else
            eligible=0
        fi
        printf '%s\x1f%s\n' "$slug" "$eligible"
    done
}

# nros_fixture_strip_authored_target_dir <cargo-args>
# Echoes <cargo-args> with any `--target-dir <value>` pair removed.
#
# The group governs the target dir for every eligible row, so the row's own
# flag has to GO — appending the group's dir beside it hands cargo two
# `--target-dir` flags, and cargo rejects that outright:
#
#   error: the argument '--target-dir <DIRECTORY>' cannot be used multiple times
#
# (Measured, because the first version of this comment guessed "cargo takes the
# last" and reasoned about a silent wrong-tree build that cannot happen. The
# real hazard is one-sided: strip in the BUILD and not in the staleness probe —
# or the reverse — and one of them errors out while the other quietly inspects
# the wrong tree. That is why both callers strip, and why a test asserts it.)
nros_fixture_strip_authored_target_dir() {
    # shellcheck disable=SC2206
    local toks=(${1:-}) out=() i=0
    while [ $i -lt ${#toks[@]} ]; do
        if [ "${toks[$i]}" = "--target-dir" ]; then
            i=$((i + 2))
            continue
        fi
        out+=("${toks[$i]}")
        i=$((i + 1))
    done
    printf '%s' "${out[*]:-}"
}

# nros_fixture_target_dir_flag <platform> <cargo-args> <envstr>
# Echoes ` --target-dir <abs>` (leading space) for an eligible row, else
# nothing. An authored `--target-dir` no longer opts the row out — the caller
# must strip it with `nros_fixture_strip_authored_target_dir` (see above).
nros_fixture_target_dir_flag() {
    local platform="$1" cargo_args="${2:-}" envstr="${3:-}"
    local group
    group="$(nros_fixture_group "$platform" "$cargo_args" "$envstr")" || return 0
    [ -n "$group" ] || return 0
    printf ' --target-dir %s' "$(nros_build_dir fixtures-cargo "$group")"
}
