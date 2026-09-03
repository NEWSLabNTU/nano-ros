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
# cargo-fixtures/<group>`): fixtures-build.sh runs cargo after
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
# phase-340 B3 — `linux` joins the shared groups 2026-08-10. Its preconditions
# were each cleared rather than assumed: A1 reports 0 artifact-name collisions
# under the WIDENED gate (B1 taught it about unhashed staticlib/cdylib output;
# 0 of 22 tracked native manifests emit one), and A2's "the Rust resolver cannot
# express a variant group" blocker was removed by B2, which resolves through the
# manifest row rather than the platform.
#
# The coarse platform-only key that would have made this trivial is REFUTED: it
# puts talker's four variants on one flat path, and cargo overwrites the final
# artifact SILENTLY across invocations (deps/ keeps both identities; the flat
# output does not, and no warning fires because the collision diagnostic only
# triggers within ONE invocation).
# phase-340 B3 wave 2 — nuttx, freertos and both threadx platforms join
# 2026-08-10. Each cleared A1 (0 collisions under B1's widened gate) and A2 (B2
# resolves through the manifest row) BEFORE being added; the check is one
# command per platform and is recorded in the commit.
#
# phase-340 item 7 — `qemu-esp32-baremetal` joins 2026-08-10. Wave 2 recorded
# that "esp32 has no rust rows in the manifest under that name", which is TRUE
# and was the wrong question: the platform string on ESP32's rust rows is
# `qemu-esp32-baremetal`, and it has THREE of them
# (`examples/qemu-esp32-baremetal/rust/{talker,listener}` and
# `packages/testing/nros-tests/bins/logging-smoke-esp32-qemu`). Asking the gate
# about a spelling the manifest does not use returns "no rows" for a platform
# that has them — the answer looked like a verdict and was a typo's shadow.
# Cleared A1/A2/A3 before being added: 7 platforms, 19 groups, 122 rows, 0
# collisions, 122 artifact roots all invertible.
#
# `nuttx-riscv` IS correctly absent, and for the reason wave 2 gave: it has one
# `[[fixture]]` row and that row is C (`examples/qemu-riscv-nuttx/c/talker`).
# There is no alternate spelling — `threadx-riscv64` is a different OS.
#
# Neither absence is about `[[workspace_fixture]]` rows, which DO exist for
# `esp32` and `nuttx-riscv`. Those never enter this mechanism: `fixtures-manifest.py
# fixture-groups` reads only the `[[fixture]]` table, and a workspace row keeps
# its authored `target_dir`. Gate scope and mechanism scope agree — checked,
# not assumed, because a rule enforced over a narrower set than it covers is
# issue 0196's shape.
export NROS_FIXTURE_SHARED_PLATFORMS="${NROS_FIXTURE_SHARED_PLATFORMS:-qemu-arm-baremetal linux nuttx freertos threadx-linux threadx-riscv64 qemu-esp32-baremetal}"

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
    printf ' --target-dir %s' "$(nros_build_dir "$NROS_KIND_CARGO_FIXTURES" "$group")"
}

# nros_fixture_row_artifact_dir <leaf-dir> <platform> [cargo-args] [envstr]
#
# WHERE a row's cargo artifacts actually land — the group dir when the platform
# shares, the leaf's own `target/` when it does not. The inverse of
# `nros_fixture_target_dir_flag`, from the same `nros_fixture_group` call, so a
# consumer cannot disagree with the build about where the build wrote.
#
# phase-340 item 7 — this exists because `just esp32 build-qemu` hand-wrote
# `examples/qemu-esp32-baremetal/rust/$ex/target/...` to find the ELF it packs
# into a flash image. Migrating the platform redirected the BUILD and left that
# path pointing at nothing:
#
#   ERROR: examples/qemu-esp32-baremetal/rust/talker/target/riscv32imc-unknown-none-elf/
#          nros-relwithdebinfo/esp32_qemu_talker is missing, and nothing narrowed this build.
#
# Every gate passed while that happened, which is #393's failure mode and the
# reason a migration's acceptance is a BUILD and never a gate. A post-processing
# step that spells the artifact path by hand is a second derivation of the group
# key; this makes it one.
nros_fixture_row_artifact_dir() {
    local leaf="$1" platform="$2" cargo_args="${3:-}" envstr="${4:-}"
    local group
    group="$(nros_fixture_group "$platform" "$cargo_args" "$envstr")" || group=""
    if [ -n "$group" ]; then
        nros_build_dir "$NROS_KIND_CARGO_FIXTURES" "$group"
    else
        printf '%s/target' "$leaf"
    fi
}

# nros_fixture_row_artifact_dir_by_id <row-id> <platform>
#
# WHERE the row with this id actually landed, with the row's OWN cargo args and
# env — read from the manifest, not supplied by the caller.
#
# Issue 1025. `nros_fixture_row_artifact_dir` made the FORMULA single; its
# INPUTS were still derived twice, and that is where the two sides diverged.
# The group key is a function of (platform, cargo args, env), so a consumer that
# passes `"" ""` for two of the three asks a different question with the same
# function and gets an answer that is wrong exactly when a row has a variant:
#
#   producer  nros_fixture_group_slug qemu-esp32-baremetal "" "ZPICO_MAX_QUERYABLES=2"
#             -> qemu-esp32-baremetal-4118800323
#   consumer  nros_fixture_group_slug qemu-esp32-baremetal "" ""
#             -> qemu-esp32-baremetal
#
# That is what stopped every ESP32 QEMU flash image from being packed the moment
# those rows gained an `env` (41a7d8de7). The manifest is the one place the
# row's args and env are written, so the fix is to read them there rather than
# to spell them a third time at the call site.
#
# A packer names the ROW it is packing — an id it already has, since the build
# loop above it passes the same id to `fixtures-build.sh --id`. Anything that
# post-processes a fixture artifact should call THIS, not the four-argument
# form; the four-argument form remains for callers that genuinely hold the
# row's args and env already (the build itself).
nros_fixture_row_artifact_dir_by_id() {
    local row_id="$1" platform="$2"
    local record dir envstr args
    record="$(python3 scripts/build/fixtures-manifest.py list --id "$row_id")" || return 1
    # Exactly one row, or the id is not an id. Refuse rather than take the
    # first: a prefix that matched two rows would pack one row's ELF under
    # another row's name, silently.
    if [ "$(printf '%s\n' "$record" | grep -c .)" != "1" ]; then
        printf 'nros_fixture_row_artifact_dir_by_id: %s matched %s manifest row(s), expected 1\n' \
            "$row_id" "$(printf '%s\n' "$record" | grep -c .)" >&2
        return 1
    fi
    IFS=$'\x1f' read -r dir envstr args <<< "$record"
    nros_fixture_row_artifact_dir "$dir" "$platform" "$args" "$envstr"
}

# nros_example_build_target_dir_flag <leaf-dir> [<platform>]
#
# issue 0635 — the `--target-dir` for a walk that builds example leaves for
# VERIFICATION rather than to produce a fixture (`build-all-extras`, the
# embedded link check).
#
# Two answers, because such a walk mixes two kinds of leaf:
#
#   * the platform HAS a shared group (`nros_fixture_group` answers) — build
#     into it, so the walk reuses the fixture build's artifacts instead of
#     compiling a second copy of them;
#   * it does not — the leaf has no coordinate, so there is no group to join,
#     and the fallback is `build/check-examples/<leaf>` rather than the leaf's
#     own `target/`, which phase-340 P2 bans outright.
#
# The platform defaults to the leaf's `examples/<platform>/…` component, which
# is the DIRECTORY vocabulary and only sometimes a manifest platform id
# (`examples/native/**` is manifest platform `linux`). That mismatch is why the
# fallback exists and why it must not be a silent leaf-`target/`: a caller that
# knows the manifest id should pass it.
nros_example_build_target_dir() {
    local leaf="${1:?nros_example_build_target_dir: leaf dir}" platform="${2:-}"
    if [ -z "$platform" ]; then
        platform="$(printf '%s' "${leaf#examples/}" | cut -d/ -f1)"
    fi
    local group
    group="$(nros_fixture_group "$platform" "" "")" || group=""
    if [ -n "$group" ]; then
        nros_build_dir "$NROS_KIND_CARGO_FIXTURES" "$group"
        return 0
    fi
    local coord
    coord="$(printf '%s' "${leaf#./}" | tr '/' '-')"
    nros_build_dir "$NROS_KIND_EXAMPLE_BUILD" "$coord"
}

# The same answer as a ready-to-splice flag. Prefer the DIR form at a call site
# that can spell `--target-dir` itself: the gate reads commands as text, and a
# whole `$flag` is opaque to it where `--target-dir "$(…)"` is not.
nros_example_build_target_dir_flag() {
    local dir
    dir="$(nros_example_build_target_dir "$@")" || return $?
    printf ' --target-dir %s' "$dir"
}
