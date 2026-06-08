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
# `target/`) unless BOTH hold:
#   * the platform has been migrated to a shared fixture dir
#     (qemu-arm-baremetal, stm32f4 — see NROS_FIXTURE_SHARED_PLATFORMS);
#   * the manifest row did NOT author its own `--target-dir` (authored
#     dirs such as target-zenoh / target-safety win and stay per-example).
#
# The emitted dir is repo-root-ABSOLUTE (`$NROS_REPO_ROOT/build/
# fixtures-cargo/<group>`): fixtures-build.sh runs cargo after
# `cd "$dir"`, so a relative dir would resolve against the example dir.

# Platforms whose default Rust fixture rows share one fixture target dir.
# ESP32 flash packaging and RTOS rows are deferred to a later patch
# (they carry extra postprocessing that reads fixed per-fixture paths).
# MUST be `export`ed: fixtures-build.sh schedules eligible rows as `make`
# leaves and ships the resolver functions to those leaves via `export -f`.
# A make leaf is a fresh bash that inherits this var only through the
# environment — a plain (non-exported) assignment is visible in the
# sourcing process but vanishes in the leaf, making every row look
# ineligible and silently fall back to the example-local `target/`.
export NROS_FIXTURE_SHARED_PLATFORMS="${NROS_FIXTURE_SHARED_PLATFORMS:-qemu-arm-baremetal stm32f4}"

# _nros_fixture_variant_sig <cargo-args> <envstr>
# Signature of everything in the grouping key BEYOND platform+triple.
# `--target <triple>` is dropped (the triple is implied 1:1 by the
# migrated platforms) and `--target-dir` is dropped (handled by
# eligibility); `--no-default-features` / `--features` and the sorted
# env DO change shared-crate compilation and are kept. Empty signature =
# the default group (slug is just the platform).
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

# nros_fixture_group <platform> <cargo-args> <envstr>
# Echoes the group slug for an eligible row, or nothing (not eligible).
nros_fixture_group() {
    local platform="$1" cargo_args="${2:-}" envstr="${3:-}"
    case " $NROS_FIXTURE_SHARED_PLATFORMS " in
        *" $platform "*) ;;
        *) return 0 ;;
    esac
    local variant
    variant="$(_nros_fixture_variant_sig "$cargo_args" "$envstr")"
    if [ -z "$variant" ]; then
        printf '%s' "$platform"
    else
        printf '%s-%s' "$platform" "$(printf '%s' "$variant" | cksum | cut -d' ' -f1)"
    fi
}

# nros_fixture_target_dir_flag <platform> <cargo-args> <envstr>
# Echoes ` --target-dir <abs>` (leading space) for an eligible row that
# did not author its own --target-dir, else nothing.
nros_fixture_target_dir_flag() {
    local platform="$1" cargo_args="${2:-}" envstr="${3:-}"
    case " $cargo_args " in
        *" --target-dir "*) return 0 ;; # authored target_dir wins
    esac
    local group
    group="$(nros_fixture_group "$platform" "$cargo_args" "$envstr")" || return 0
    [ -n "$group" ] || return 0
    local root="${NROS_REPO_ROOT:-$PWD}"
    printf ' --target-dir %s/build/fixtures-cargo/%s' "$root" "$group"
}
