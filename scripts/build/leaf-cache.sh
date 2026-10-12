#!/usr/bin/env bash
# Sourced, never run. Shared by `workspace-fixtures-build.sh` and
# `census-prepass.sh`: every script that runs `nros build --offline` warms the
# cargo cache for that image first, through this one function. The prepass had
# no warm step and died on a cold cache (issue 1848) while its sibling builder
# had one (issue 0967), which is why there is one copy now.
#
# The caller sets `repo_root` and `nros_cli` before calling.

# nros_warm_leaf_cache <workspace-dir> [<image>]
#
# Populate the cargo registry cache for one workspace leaf, ONCE, before the
# `--offline` build that follows. Issue 0967.
#
# `nros build --offline` passes `--frozen` (= `--locked --offline`, issue 0676),
# and for a GENERATED root it first runs `cargo generate-lockfile --offline`.
# Both can only use crates already in the cache. `examples/workspaces/*` leaves
# are SEPARATE cargo workspace roots: CI provisioning warms the cache for the
# ROOT graph, and nothing warmed it for the leaves. So the resolve wanted crates
# nobody had fetched and cargo reported
#
#     error: failed to download `toml v0.9.12+spec-1.1.0`
#     Caused by: attempting to make an HTTP request, but --frozen was specified
#
# — a headline that reads as a network flake and invites a retry that cannot
# help. `host-tests` died on it for 20 consecutive runs, in the step BEFORE any
# test, so the only CI lane that runs E2E ran nothing at all.
#
# This does NOT relax `--locked` or `--frozen`. The BUILD stays hermetic, which
# is the property issue 0676 wants; only this prepare step reaches the network,
# exactly as `nros setup --source` already does. A lockfile that moves because
# CI could not resolve is the drift issues 0359/0378 exist to prevent, and
# fetching is what avoids it — permitting re-resolution would not.
#
# Local runs pay nothing: a warm cache makes this a no-op, which is also why the
# defect never showed up in a developer sweep — only `--offline` exposes it.
_NROS_WARMED_LEAVES=""
_NROS_WARMED_ROOT=""
_NROS_WARMED_IMAGES=""
nros_warm_leaf_cache() {
    local ws="${1:?nros_warm_leaf_cache: workspace dir}"
    case " $_NROS_WARMED_LEAVES " in *" $ws "*) return 0 ;; esac
    _NROS_WARMED_LEAVES="$_NROS_WARMED_LEAVES $ws"

    if [ "${NROS_SKIP_LEAF_FETCH:-0}" != "0" ]; then
        echo "  leaf-fetch: SKIPPED for $ws (NROS_SKIP_LEAF_FETCH set)"
        return 0
    fi

    # TWO roots, because the generated root draws from both and MEASUREMENT
    # showed the leaf alone is not enough. Against a cold CARGO_HOME, a leaf
    # fetch pulled `allocator-api2` but NOT `toml 0.9.12+spec-1.1.0` — the two
    # crates CI reported. `toml` reaches the generated root through nano-ros
    # PATH deps (cbindgen, nros-bridge, nros-tests), whose registry deps live in
    # the REPO ROOT lock, not the leaf's. Warming only the leaf would have
    # fixed half the failure and looked like a fix.
    if [ -z "$_NROS_WARMED_ROOT" ]; then
        _NROS_WARMED_ROOT=1
        if ( cd "$repo_root" && cargo fetch >/dev/null 2>&1 ); then
            :
        else
            echo "  leaf-fetch: could NOT warm the cargo cache for the repo root." >&2
        fi
    fi

    # The LEAF half warms the graph the build will actually resolve: the
    # GENERATED cargo root `nros build` writes for this image. It used to test
    # `$ws/Cargo.toml` and fetch there — but phase-445 W6 removed every
    # workspace-root `Cargo.toml` under `examples/workspaces/`
    # (`check-workspace-root-build-files`), so that test failed for all 16
    # workspaces and this half returned early, silently, for every one of them.
    # Only the repo-root fetch above still ran. Crates the root graph does not
    # reach went unfetched, and the esp32 image — whose `esp-hal`,
    # `esp-backtrace` and `allocator-api2` live only in its own generated
    # graph — failed every nightly with `failed to download allocator-api2`
    # "but --frozen was specified", the exact headline this function exists to
    # prevent.
    #
    # `--dry-run --offline` runs stages 1-4 (which WRITE the generated root)
    # and prints the stage-5 commands instead of exec'ing them; it touches no
    # network. The fetch below is then the one sanctioned network step, against
    # that root with its own `--config` (its `[patch.crates-io]` rows decide
    # what resolves). A cmake/west-driven image prints no `cargo build` line and
    # has no cargo root to warm, so it is skipped rather than guessed at.
    local img="${2:-}"
    [ -n "$img" ] || return 0
    case " $_NROS_WARMED_IMAGES " in *" $ws|$img "*) return 0 ;; esac
    _NROS_WARMED_IMAGES="$_NROS_WARMED_IMAGES $ws|$img"

    local plan manifest cfg
    if plan="$(cd "$ws" && "$nros_cli" build "$img" --workspace . --offline --dry-run 2>&1)"; then
        manifest="$(printf '%s\n' "$plan" \
            | sed -n 's/^cargo build .*--manifest-path \([^ ]*\).*/\1/p' | head -n 1)"
        [ -n "$manifest" ] || return 0
        cfg="$(printf '%s\n' "$plan" \
            | sed -n 's/^cargo build .*--config \([^ ]*\).*/\1/p' | head -n 1)"
        if ( cd "$ws" && cargo fetch --manifest-path "$manifest" \
                ${cfg:+--config "$cfg"} >/dev/null 2>&1 ); then
            return 0
        fi
    fi
    # Not fatal: an environment that is offline BY DESIGN with a warm cache is
    # legitimate, and failing here would break it. But say so, because the
    # build that follows may now fail with cargo's misleading "failed to
    # download", and this is the sentence that explains it.
    echo "  leaf-fetch: could NOT warm the cargo cache for $ws." >&2
    echo "              If the build below reports 'failed to download <crate>'" >&2
    echo "              with 'but --frozen was specified', that is why — the" >&2
    echo "              crate is missing from the cache, not from the network." >&2
    echo "              (issue 0967; NROS_SKIP_LEAF_FETCH=1 to silence)" >&2
    return 0
}
